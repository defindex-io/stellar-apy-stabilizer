import { Address, scValToNative, xdr } from "@stellar/stellar-sdk";
import {
  FEE_PROXY_ADDRESS,
  ProxyMethods,
} from "./constants";
import {
  getFeeManagerKeypair,
  signAndSubmit,
  simulateContractCall,
} from "./stellar-rpc";
import type {
  ContractTxResult,
  ProxyVaultConfig,
  SupportedNetwork,
} from "./types";

export async function getVaultConfig(
  network: SupportedNetwork,
  vaultAddress: string,
): Promise<ProxyVaultConfig> {
  const proxyAddress = FEE_PROXY_ADDRESS[network];
  const sim = await simulateContractCall(
    proxyAddress,
    ProxyMethods.GET_VAULT_CONFIG,
    [new Address(vaultAddress).toScVal()],
    network,
  );

  const raw = scValToNative(sim.result!.retval);
  return {
    admin: raw.admin,
    targetApyBps: Number(raw.target_apy_bps),
    minFeeBps: Number(raw.min_fee_bps),
    maxFeeBps: Number(raw.max_fee_bps),
  };
}

export async function lockFees(
  network: SupportedNetwork,
  vaultAddress: string,
  newFeeBps: number,
): Promise<ContractTxResult> {
  const proxyAddress = FEE_PROXY_ADDRESS[network];
  const keypair = getFeeManagerKeypair();
  const feeManagerAddress = keypair.publicKey();

  return signAndSubmit(
    network,
    proxyAddress,
    ProxyMethods.LOCK_FEES,
    [
      new Address(feeManagerAddress).toScVal(),
      new Address(vaultAddress).toScVal(),
      xdr.ScVal.scvU32(newFeeBps),
    ],
    keypair,
  );
}
