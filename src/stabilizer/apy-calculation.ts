import { scValToNative, xdr } from "@stellar/stellar-sdk";
import {
  SNAPSHOT_SEARCH_WINDOW_DAYS,
  VaultMethods,
} from "./constants";
import {
  GetInvocation,
  simulateMultipleInvocations,
} from "./stellar-rpc";
import {
  getHistoricalLockedFees,
  getVaultHistoricalStates,
} from "./indexer-db";
import type {
  LiveGrossApyResult,
  StrategyReport,
  SupportedNetwork,
  VaultStateSnapshot,
} from "./types";

/**
 * Sums locked_fee across a StrategyReport array (port of sumLockedFeesFromReport).
 */
function sumLockedFees(reports: StrategyReport[]): bigint {
  let total = 0n;
  for (const r of reports) {
    total += BigInt(r.locked_fee);
  }
  return total;
}

export async function calculateLiveGrossApy(
  network: SupportedNetwork,
  vaultAddress: string,
  days: number,
): Promise<LiveGrossApyResult> {
  const endDate = new Date();

  // --- End point: live chain read via router exec(REPORT, TMF, SUPPLY) ----
  // Note: the API uses [FETCH_TMF, TOTAL_SUPPLY, REPORT] but here we ask the
  // router to dispatch using the vault as source — the router only enforces
  // auth checks on `caller`, which we set to the vault address so manager-
  // gated REPORT works without a real signature in simulation.
  const invocations = [
    GetInvocation(vaultAddress, VaultMethods.FETCH_TOTAL_MANAGED_FUNDS),
    GetInvocation(vaultAddress, VaultMethods.TOTAL_SUPPLY),
    GetInvocation(vaultAddress, VaultMethods.REPORT),
  ];
  const live = await simulateMultipleInvocations(network, vaultAddress, invocations);
  const liveValues = scValToNative(live.result!.retval) as unknown[];
  const liveFunds = liveValues[0] as Array<{ total_amount: bigint | string }>;
  const liveSupply: string = (liveValues[1] as bigint | string).toString() ?? "0";
  const reportResult = liveValues[2] as StrategyReport[];

  const liveTotalAmount = BigInt(liveFunds?.[0]?.total_amount ?? 0);
  const liveStoredLockedFees = sumLockedFees(reportResult);
  const liveGrossTotal = liveTotalAmount + liveStoredLockedFees;
  const liveSupplyBigInt = BigInt(liveSupply);

  if (liveSupplyBigInt === 0n || liveGrossTotal === 0n) {
    return {
      apy: null,
      requestedDays: days,
      actualDays: null,
      startDate: null,
      endDate,
      startPps: null,
      endPps: null,
    };
  }
  const endPps = Number(liveGrossTotal) / Number(liveSupplyBigInt);

  // --- Start point: nearest DB snapshot <= (now − days) -------------------
  const fromDate = new Date(endDate.getTime() - days * 24 * 60 * 60 * 1000);
  const startSnapshot = await findStartSnapshot(network, vaultAddress, fromDate, endDate);

  if (!startSnapshot) {
    return {
      apy: null,
      requestedDays: days,
      actualDays: null,
      startDate: null,
      endDate,
      startPps: null,
      endPps,
    };
  }

  const historicalLockedFees = BigInt(
    await getHistoricalLockedFees(network, vaultAddress, startSnapshot.ledger),
  );
  const startTotalAmount = BigInt(startSnapshot.totalManagedFundsBefore[0].total_amount);
  const startGrossTotal = startTotalAmount + historicalLockedFees;
  const startSupplyBigInt = BigInt(startSnapshot.totalSupplyBefore);

  if (startSupplyBigInt === 0n || startGrossTotal === 0n) {
    return {
      apy: null,
      requestedDays: days,
      actualDays: null,
      startDate: startSnapshot.date,
      endDate,
      startPps: null,
      endPps,
    };
  }
  const startPps = Number(startGrossTotal) / Number(startSupplyBigInt);

  const actualDays =
    (endDate.getTime() - startSnapshot.date.getTime()) / (1000 * 60 * 60 * 24);

  // Below 10 minutes the annualization is too noisy to be useful.
  if (actualDays < 10 / (60 * 24)) {
    return {
      apy: null,
      requestedDays: days,
      actualDays,
      startDate: startSnapshot.date,
      endDate,
      startPps,
      endPps,
    };
  }

  // APY = (endPps / startPps) ^ (365.2425 / actualDays) − 1, as decimal.
  const ratio = endPps / startPps;
  const apy = Math.pow(ratio, 365.2425 / actualDays) - 1;

  return {
    apy,
    requestedDays: days,
    actualDays,
    startDate: startSnapshot.date,
    endDate,
    startPps,
    endPps,
  };
}

async function findStartSnapshot(
  network: SupportedNetwork,
  vaultAddress: string,
  fromDate: Date,
  toDate: Date,
): Promise<VaultStateSnapshot | null> {
  const searchFrom = new Date(
    fromDate.getTime() - SNAPSHOT_SEARCH_WINDOW_DAYS * 24 * 60 * 60 * 1000,
  );
  const snapshots = await getVaultHistoricalStates(
    network,
    vaultAddress,
    searchFrom,
    toDate,
  );
  if (!snapshots || snapshots.length === 0) return null;

  snapshots.sort((a, b) => a.date.getTime() - b.date.getTime());

  const beforeTarget = snapshots.filter((s) => s.date.getTime() <= fromDate.getTime());
  return beforeTarget.length > 0
    ? beforeTarget[beforeTarget.length - 1]
    : snapshots[0];
}
