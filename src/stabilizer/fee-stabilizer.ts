import { MAX_FEE_DELTA_BPS_PER_CYCLE, DEAD_ZONE_BPS } from "./constants";

/**
 * fee_bps = max(0, (1 - target_apy / strategy_apy) * 10000), rounded.
 * Returns 0 when strategy APY is ≤ target.
 */
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
