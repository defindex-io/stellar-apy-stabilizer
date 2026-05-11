import { Keypair, rpc, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import {
  NETWORK_PASSPHRASE,
  POLL_INTERVAL_MS,
  POLL_TIMEOUT_MS,
  TX_FEE,
  TX_TIMEOUT_SECONDS,
} from "../constants/index";
import { log, sleep } from "./log";

export const rpcServer = new rpc.Server(process.env.SOROBAN_RPC as string);

export function getCallerKeypair(): Keypair {
  return Keypair.fromSecret(process.env.STELLAR_SECRET_KEY as string);
}

export async function sendAndConfirm(
  operation: xdr.Operation,
  kp: Keypair,
): Promise<rpc.Api.GetSuccessfulTransactionResponse> {
  const signedTx = await buildAndSign(operation, kp);
  const sentTx = await rpcServer.sendTransaction(signedTx);

  if (sentTx.status === "ERROR") {
    throw new Error(`sendTransaction failed: ${JSON.stringify(sentTx.errorResult)}`);
  }

  log(`submitted tx ${sentTx.hash}`);
  const result = await pollTransaction(sentTx.hash);

  if (result.status !== "SUCCESS") {
    throw new Error(`tx ${sentTx.hash} did not succeed (status=${result.status})`);
  }
  return result;
}

async function buildAndSign(operation: xdr.Operation, kp: Keypair) {
  const source = await rpcServer.getAccount(kp.publicKey());
  const tx = new TransactionBuilder(source, {
    fee: TX_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(operation)
    .setTimeout(TX_TIMEOUT_SECONDS)
    .build();

  const sim = await rpcServer.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`simulation failed: ${sim.error}`);
  }

  const prepped = rpc.assembleTransaction(tx, sim).build();
  prepped.sign(kp);
  return prepped;
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
