import {
  Account,
  Address,
  Contract,
  Keypair,
  rpc,
  TransactionBuilder,
  xdr,
} from "@stellar/stellar-sdk";
import {
  HELPER_ADDRESS,
  NETWORK_PASSPHRASE,
  POLL_INTERVAL_MS,
  POLL_TIMEOUT_MS,
  RouterMethods,
  STELLAR_ROUTER,
  TX_FEE,
  TX_TIMEOUT_SECONDS,
} from "./constants";
import type {
  ContractTxResult,
  Invocation,
  SupportedNetwork,
} from "./types";
import { log, sleep } from "../helpers/log";

const rpcServer = new rpc.Server(process.env.SOROBAN_RPC as string);

export function getRpcServer(): rpc.Server {
  return rpcServer;
}

export function getFeeManagerKeypair(): Keypair {
  const secret = process.env.FEE_MANAGER_SECRET_KEY;
  if (!secret) {
    throw new Error("FEE_MANAGER_SECRET_KEY is not set");
  }
  return Keypair.fromSecret(secret);
}

// ---- Build an Invocation (port of GetInvocation from API helpers) ----------

export function GetInvocation(
  contractAddress: string,
  method: string,
  args: xdr.ScVal[] = [],
): Invocation {
  return {
    contract: new Address(contractAddress),
    method,
    args,
    can_fail: false,
  };
}

// ---- Read-only: single contract call simulation ----------------------------

export async function simulateContractCall(
  contractId: string,
  method: string,
  params: xdr.ScVal[],
  network: SupportedNetwork,
  source: string = HELPER_ADDRESS,
): Promise<rpc.Api.SimulateTransactionSuccessResponse> {
  const contract = new Contract(contractId);
  const operation = contract.call(method, ...params);
  const account = new Account(source, "0");

  const tx = new TransactionBuilder(account, {
    fee: TX_FEE,
    timebounds: { minTime: 0, maxTime: 0 },
    networkPassphrase: NETWORK_PASSPHRASE[network],
  })
    .addOperation(operation)
    .build();

  const sim = await rpcServer.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`simulate ${contractId}.${method} failed: ${sim.error}`);
  }
  if (!sim.result) {
    throw new Error(`simulate ${contractId}.${method} returned no result`);
  }
  return sim as rpc.Api.SimulateTransactionSuccessResponse;
}

// ---- Read-only: multi-invocation via DeFindex router contract --------------
//
// The router contract's `exec` method takes a caller (any G-account, since
// simulations don't enforce auth) and a vec of (contract, method, args, can_fail)
// tuples. Its return value is a vec aligned with the input.

export async function simulateMultipleInvocations(
  network: SupportedNetwork,
  from: string,
  invocations: Invocation[],
): Promise<rpc.Api.SimulateTransactionSuccessResponse> {
  return simulateContractCall(
    STELLAR_ROUTER[network],
    RouterMethods.EXEC,
    [
      new Address(from).toScVal(),
      xdr.ScVal.scvVec(
        invocations.map((inv) =>
          xdr.ScVal.scvVec([
            new Address(inv.contract.toString()).toScVal(),
            xdr.ScVal.scvSymbol(inv.method),
            xdr.ScVal.scvVec(inv.args),
            xdr.ScVal.scvBool(inv.can_fail),
          ]),
        ),
      ),
    ],
    network,
    from,
  );
}

// ---- Write: sign + submit ---------------------------------------------------

export async function signAndSubmit(
  network: SupportedNetwork,
  contractAddress: string,
  method: string,
  params: xdr.ScVal[],
  keypair: Keypair,
): Promise<ContractTxResult> {
  try {
    const sourceAccount = await rpcServer.getAccount(keypair.publicKey());

    const contract = new Contract(contractAddress);
    const operation = contract.call(method, ...params);

    const tx = new TransactionBuilder(sourceAccount, {
      fee: TX_FEE,
      networkPassphrase: NETWORK_PASSPHRASE[network],
    })
      .addOperation(operation)
      .setTimeout(TX_TIMEOUT_SECONDS)
      .build();

    const sim = await rpcServer.simulateTransaction(tx);
    if (rpc.Api.isSimulationError(sim)) {
      throw new Error(`simulate ${contractAddress}.${method} failed: ${sim.error}`);
    }

    const prepped = rpc.assembleTransaction(tx, sim).build();
    prepped.sign(keypair);

    const sent = await rpcServer.sendTransaction(prepped);
    if (sent.status === "ERROR") {
      throw new Error(`sendTransaction failed: ${JSON.stringify(sent.errorResult)}`);
    }
    log(`submitted ${method} tx ${sent.hash}`);

    const result = await pollTransaction(sent.hash);
    if (result.status !== "SUCCESS") {
      throw new Error(`tx ${sent.hash} not successful (status=${result.status})`);
    }
    return { txHash: sent.hash, success: true };
  } catch (err) {
    log(`signAndSubmit error: ${(err as Error).message}`);
    return { txHash: "", success: false };
  }
}

async function pollTransaction(hash: string): Promise<rpc.Api.GetTransactionResponse> {
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const tx = await rpcServer.getTransaction(hash);
    if (tx.status !== "NOT_FOUND") return tx;
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(`timed out waiting for tx ${hash}`);
}
