import { Address, Contract, Keypair, nativeToScVal, xdr } from "@stellar/stellar-sdk";
import {
  DEPOSIT_AMOUNT_STROOPS,
  INVEST,
  MIN_AMOUNT_OUT_STROOPS,
  VAULTS,
  WITHDRAW_SHARES,
} from "../constants/index";
import { log } from "./log";
import { sendAndConfirm } from "./stellar";

// deposit(amounts_desired: Vec<i128>, amounts_min: Vec<i128>, from: Address, invest: bool)
// withdraw(withdraw_shares: i128, min_amounts_out: Vec<i128>, from: Address)

export async function depositToVault(vaultId: string, kp: Keypair): Promise<void> {
  const amountScVal = nativeToScVal(DEPOSIT_AMOUNT_STROOPS, { type: "i128" });
  const params: xdr.ScVal[] = [
    xdr.ScVal.scvVec([amountScVal]),
    xdr.ScVal.scvVec([amountScVal]),
    new Address(kp.publicKey()).toScVal(),
    xdr.ScVal.scvBool(INVEST),
  ];

  const op = new Contract(vaultId).call("deposit", ...params);
  await sendAndConfirm(op, kp);
}

export async function withdrawFromVault(vaultId: string, kp: Keypair): Promise<void> {
  const params: xdr.ScVal[] = [
    nativeToScVal(WITHDRAW_SHARES, { type: "i128" }),
    xdr.ScVal.scvVec([nativeToScVal(MIN_AMOUNT_OUT_STROOPS, { type: "i128" })]),
    new Address(kp.publicKey()).toScVal(),
  ];

  const op = new Contract(vaultId).call("withdraw", ...params);
  await sendAndConfirm(op, kp);
}

export async function runOnAllVaults(
  label: string,
  fn: (vaultId: string, kp: Keypair) => Promise<void>,
  kp: Keypair,
): Promise<number> {
  let failures = 0;

  for (const vault of VAULTS) {
    log(`→ ${label} on ${vault}`);
    try {
      await fn(vault, kp);
      log(`✓ ${label} ok`);
    } catch (err) {
      failures += 1;
      log(`✗ ${label} failed: ${(err as Error).message}`);
    }
  }
  return failures;
}
