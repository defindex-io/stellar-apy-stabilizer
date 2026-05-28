import { Networks } from "@stellar/stellar-sdk";
import type { SupportedNetwork } from "./types";

// ---- FeeProxy contract addresses (per network) ------------------------------
// Testnet placeholder kept to mirror the API; never read in this iteration.
export const FEE_PROXY_ADDRESS: Record<SupportedNetwork, string> = {
  mainnet:
    process.env.FEE_PROXY_ADDRESS_MAINNET ??
    "CDEFLWJMPR6DDNOEGP6KNPSPRWKPUG3DJLIOQZIS6EHIGNK7EGTQSA7R",
  testnet: "C_TESTNET_TBD_AFTER_DEPLOYMENT",
};

// ---- DeFindex Stellar Router (used for multi-invocation simulation) --------
// Source: defindex-api/src/helpers/constants.ts (STELLAR_ROUTER_MAINNET).
export const STELLAR_ROUTER: Record<SupportedNetwork, string> = {
  mainnet: "CDAW42JDSDEI2DXEPP4E7OAYNCRUA4LGCZHXCJ4BV5WVI4O4P77FO4UV",
  testnet: "CAG7OQAN4YO65ZLOYA5PWJKPYYE5BVH7QSRI4KAW7VBMIH6N6LG5ECSL",
};

// Burner G-account used as the "source" for read-only simulations.
export const HELPER_ADDRESS = "GALAXYVOIDAOPZTDLHILAJQKCVVFMD4IKLXLSZV5YHO7VY74IWZILUTO";

// ---- Reactive controller ----------------------------------------------------

/** Skip fee adjustment if |required - current| <= this (bps). */
export const DEAD_ZONE_BPS = 50;

/** Maximum fee change applied in a single cycle (bps). */
export const MAX_FEE_DELTA_BPS_PER_CYCLE = 100;

/** Lookback window the controller uses for APY math (days). 1d matches the cron cadence. */
export const CONTROLLER_APY_WINDOW_DAYS = 1;

/** How far back of the requested fromDate the snapshot search reaches. */
export const SNAPSHOT_SEARCH_WINDOW_DAYS = 30;

// ---- Runtime flags ----------------------------------------------------------

/** When true, computes everything but skips lock_fees submit. Default: true. */
export const DRY_RUN = (process.env.VAULT_OPS_DRY_RUN ?? "true") !== "false";

/** Hourly by default; can be tightened via env for local testing. */
export const STABILIZER_INTERVAL_MS = Number(
  process.env.STABILIZER_INTERVAL_MS ?? 60 * 60 * 1000,
);

// ---- Stellar tx mechanics --------------------------------------------------

export const NETWORK_PASSPHRASE: Record<SupportedNetwork, string> = {
  mainnet: Networks.PUBLIC,
  testnet: Networks.TESTNET,
};
export const TX_FEE = "2000";
export const TX_TIMEOUT_SECONDS = 300;
export const POLL_INTERVAL_MS = 2_000;
export const POLL_TIMEOUT_MS = 60_000;

// ---- Contract method names -------------------------------------------------

export const ProxyMethods = {
  GET_VAULT_CONFIG: "get_vault_config",
  GET_FEE_MANAGER: "get_fee_manager",
  LOCK_FEES: "lock_fees",
} as const;

export const VaultMethods = {
  FETCH_TOTAL_MANAGED_FUNDS: "fetch_total_managed_funds",
  TOTAL_SUPPLY: "total_supply",
  REPORT: "report",
} as const;

export const RouterMethods = { EXEC: "exec" } as const;
