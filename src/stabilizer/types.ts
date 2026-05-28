import { Address, xdr } from "@stellar/stellar-sdk";

export type SupportedNetwork = "mainnet" | "testnet";

// --- FeeProxy contract -------------------------------------------------------

export interface ProxyVaultConfig {
  admin: string;
  targetApyBps: number;
  minFeeBps: number;
  maxFeeBps: number;
}

// --- Indexer DB shapes -------------------------------------------------------

/** Row returned by getManagedVaultsWithApy. APY values are decimals (0.0977 = 9.77%). */
export interface ManagedVaultApyData {
  vaultId: string;
  vaultFeeBps: number;
  strategyApy24h: number | null;
  strategyApy7d: number | null;
  apy7dNet: number | null;
  tvl: string;
  totalSupply: string;
  currentPpsNet: number | null;
}

/** One historical vault snapshot returned by getVaultHistoricalStates. */
export interface VaultStateSnapshot {
  vaultAddress: string;
  date: Date;
  totalSupplyBefore: string;
  totalManagedFundsBefore: Array<{
    asset: string;
    total_amount: string;
  }>;
  ledger: number;
}

// --- Strategy report (Soroban REPORT return) ---------------------------------

export interface StrategyReport {
  strategy: string;
  prev_balance: bigint;
  gains_or_losses: bigint;
  locked_fee: bigint;
}

// --- Live gross APY ---------------------------------------------------------

export interface LiveGrossApyResult {
  /** Gross (pre-fee) annualized return as decimal (0.09 = 9%). null if unavailable. */
  apy: number | null;
  requestedDays: number;
  actualDays: number | null;
  startDate: Date | null;
  endDate: Date;
  startPps: number | null;
  endPps: number | null;
}

// --- Stabilization cycle outputs --------------------------------------------

export type StabilizationAction =
  | "adjusted"
  | "skipped_dead_zone"
  | "skipped_no_data"
  | "error";

export interface VaultStabilizationResult {
  vaultAddress: string;
  grossApy: number | null;
  currentNetApy: number | null;
  projectedNetApy: number | null;
  targetApyBps: number;
  currentFeeBps: number;
  requiredFeeBps: number;
  appliedFeeBps: number | null;
  clampedByRateLimit: boolean;
  action: StabilizationAction;
  error?: string;
  txHash?: string;
}

export interface StabilizationCycleResult {
  network: SupportedNetwork;
  timestamp: string;
  vaultsDiscovered: number;
  vaultsProcessed: number;
  vaultsSkipped: number;
  adjustmentsMade: number;
  results: VaultStabilizationResult[];
}

// --- Stellar helpers --------------------------------------------------------

export interface ContractTxResult {
  txHash: string;
  success: boolean;
}

export interface Invocation {
  contract: Address;
  method: string;
  args: xdr.ScVal[];
  can_fail: boolean;
}
