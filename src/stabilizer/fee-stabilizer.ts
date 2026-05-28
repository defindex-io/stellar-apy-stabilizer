import {
  CONTROLLER_APY_WINDOW_DAYS,
  DEAD_ZONE_BPS,
  DRY_RUN,
  FEE_PROXY_ADDRESS,
  MAX_FEE_DELTA_BPS_PER_CYCLE,
} from "./constants";
import { getManagedVaultsWithApy } from "./indexer-db";
import { calculateLiveGrossApy } from "./apy-calculation";
import { getVaultConfig, lockFees } from "./proxy-contract";
import { log } from "../helpers/log";
import type {
  LiveGrossApyResult,
  ManagedVaultApyData,
  ProxyVaultConfig,
  StabilizationCycleResult,
  SupportedNetwork,
  VaultStabilizationResult,
} from "./types";

// ===========================================================================
// PURE MATH (kept from Task 4)
// ===========================================================================

export function calculateRequiredFee(strategyApy: number, targetApyBps: number): number {
  if (strategyApy <= 0) return 0;
  const targetApy = targetApyBps / 10_000;
  if (strategyApy <= targetApy) return 0;
  const rawFee = (1 - targetApy / strategyApy) * 10_000;
  return Math.round(Math.max(0, rawFee));
}

export function shouldAdjust(currentFeeBps: number, requiredFeeBps: number): boolean {
  return Math.abs(currentFeeBps - requiredFeeBps) > DEAD_ZONE_BPS;
}

export function applyRateLimit(
  currentFeeBps: number,
  requiredFeeBps: number,
  maxDelta: number = MAX_FEE_DELTA_BPS_PER_CYCLE,
): { appliedFeeBps: number; clamped: boolean } {
  const delta = requiredFeeBps - currentFeeBps;
  if (Math.abs(delta) <= maxDelta) {
    return { appliedFeeBps: requiredFeeBps, clamped: false };
  }
  const sign = delta > 0 ? 1 : -1;
  return { appliedFeeBps: currentFeeBps + sign * maxDelta, clamped: true };
}

function computeNetApy(grossApy: number | null, feeBps: number): number | null {
  if (grossApy === null) return null;
  return grossApy * (1 - feeBps / 10_000);
}

// ===========================================================================
// CYCLE
// ===========================================================================

export async function runStabilizationCycle(
  network: SupportedNetwork,
): Promise<StabilizationCycleResult> {
  const proxyAddress = FEE_PROXY_ADDRESS[network];
  log(`=== stabilization tick (${network} · proxy=${proxyAddress} · dryRun=${DRY_RUN}) ===`);

  const vaults = await getManagedVaultsWithApy(network, proxyAddress);
  log(`discovered ${vaults.length} managed vault(s)`);

  const results: VaultStabilizationResult[] = [];
  let adjustmentsMade = 0;
  let vaultsSkipped = 0;

  for (const vault of vaults) {
    const result = await processVault(network, vault);
    results.push(result);
    logVaultResult(result);
    if (result.action === "adjusted") adjustmentsMade++;
    if (result.action.startsWith("skipped")) vaultsSkipped++;
  }

  const cycle: StabilizationCycleResult = {
    network,
    timestamp: new Date().toISOString(),
    vaultsDiscovered: vaults.length,
    vaultsProcessed: vaults.length,
    vaultsSkipped,
    adjustmentsMade,
    results,
  };

  log(
    `=== tick done · processed=${cycle.vaultsProcessed} adjusted=${cycle.adjustmentsMade} skipped=${cycle.vaultsSkipped} errors=${
      results.filter((r) => r.action === "error").length
    } ===`,
  );
  return cycle;
}

function logVaultResult(r: VaultStabilizationResult): void {
  const v = r.vaultAddress.slice(0, 8);
  if (r.action === "error") {
    log(`✗ ${v}… error: ${r.error}`);
    return;
  }
  if (r.action === "skipped_no_data") {
    log(`↺ ${v}… skipped_no_data (grossApy=${r.grossApy})`);
    return;
  }
  if (r.action === "skipped_dead_zone") {
    log(
      `↺ ${v}… skipped_dead_zone (current=${r.currentFeeBps} required=${r.requiredFeeBps} dead=${DEAD_ZONE_BPS})`,
    );
    return;
  }
  // adjusted
  const tail = r.txHash ? `tx=${r.txHash}` : "";
  log(
    `✓ ${v}… current=${r.currentFeeBps} required=${r.requiredFeeBps} applied=${r.appliedFeeBps}${
      r.clampedByRateLimit ? " (rate-limited)" : ""
    } ${tail}`,
  );
}

async function processVault(
  network: SupportedNetwork,
  vault: ManagedVaultApyData,
): Promise<VaultStabilizationResult> {
  const base: VaultStabilizationResult = {
    vaultAddress: vault.vaultId,
    grossApy: null,
    currentNetApy: null,
    projectedNetApy: null,
    targetApyBps: 0,
    currentFeeBps: vault.vaultFeeBps,
    requiredFeeBps: 0,
    appliedFeeBps: null,
    clampedByRateLimit: false,
    action: "skipped_no_data",
  };

  let config: ProxyVaultConfig;
  try {
    config = await getVaultConfig(network, vault.vaultId);
  } catch (err) {
    return { ...base, action: "error", error: errMsg(err) };
  }
  base.targetApyBps = config.targetApyBps;
  const targetApy = config.targetApyBps / 10_000;

  let liveApy: LiveGrossApyResult;
  try {
    liveApy = await calculateLiveGrossApy(network, vault.vaultId, CONTROLLER_APY_WINDOW_DAYS);
  } catch (err) {
    return { ...base, action: "error", error: errMsg(err) };
  }

  base.grossApy = liveApy.apy;
  base.currentNetApy = computeNetApy(liveApy.apy, vault.vaultFeeBps);
  base.projectedNetApy = base.currentNetApy;

  if (liveApy.apy === null || liveApy.apy <= 0) return base;

  // --- Branch 1: strategy at or below target → clamp to minFee
  if (liveApy.apy <= targetApy) {
    const requiredFee = config.minFeeBps;
    base.requiredFeeBps = requiredFee;
    if (!shouldAdjust(vault.vaultFeeBps, requiredFee)) {
      return { ...base, action: "skipped_dead_zone" };
    }
    return submit(network, vault, base, liveApy.apy, requiredFee);
  }

  // --- Branch 2: required = clamp(min, max, formula)
  let requiredFee = calculateRequiredFee(liveApy.apy, config.targetApyBps);
  requiredFee = Math.max(config.minFeeBps, Math.min(config.maxFeeBps, requiredFee));
  base.requiredFeeBps = requiredFee;

  if (!shouldAdjust(vault.vaultFeeBps, requiredFee)) {
    return { ...base, action: "skipped_dead_zone" };
  }
  return submit(network, vault, base, liveApy.apy, requiredFee);
}

async function submit(
  network: SupportedNetwork,
  vault: ManagedVaultApyData,
  base: VaultStabilizationResult,
  grossApy: number,
  requiredFee: number,
): Promise<VaultStabilizationResult> {
  const { appliedFeeBps, clamped } = applyRateLimit(vault.vaultFeeBps, requiredFee);
  base.appliedFeeBps = appliedFeeBps;
  base.clampedByRateLimit = clamped;
  base.projectedNetApy = computeNetApy(grossApy, appliedFeeBps);

  if (DRY_RUN) {
    return { ...base, action: "adjusted", txHash: "DRY_RUN" };
  }
  const tx = await lockFees(network, vault.vaultId, appliedFeeBps);
  if (!tx.success) {
    return { ...base, action: "error", error: "lock_fees transaction failed" };
  }
  return { ...base, action: "adjusted", txHash: tx.txHash };
}

function errMsg(err: unknown): string {
  return err instanceof Error ? err.message : "Unknown error";
}
