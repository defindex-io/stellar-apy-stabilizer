import { Networks } from "@stellar/stellar-sdk";

export const VAULTS: readonly string[] = [
  "CD7T34Y5SZ6MBEZDMXDIQWQ6JICO7TYH7E6DKZJ7BHXOMR2EQ65WYSZG",
  "CB5YXWIDBQAOTTPEQE3SRNUFM2PTOXFHKGUWCBJJSF2GPW37DN725FDA",
  "CAEPJIHET2TBI2VCLJZI6QHMN366KUGNK4AOKE3YY7AOKMU4KX4RDRGB",
  "CD3HR7WNGPDUGK5ITNMZSRM36O2IFJF3N4RFHOITP4DCXMVGHMANN3XR",
] as const;

// Vaults tracked by the APY-history poller. Kept separate from VAULTS (the
// deposit/withdraw stabilization cycle) so polling scope can change without
// affecting which vaults the cron moves funds on.
export const APY_POLL_VAULTS: readonly string[] = [
  "CCA2ZJP5BVRXYTQH4FAGHCAUMRYCXVC4CRYC2NXHWMR7TIVX36U7F5HR",
] as const;

// Human-readable labels used by the APY-history poller so historical rows are
// interpretable without joining to another table.
export const VAULT_NAMES: Record<string, string> = {
  CAEPJIHET2TBI2VCLJZI6QHMN366KUGNK4AOKE3YY7AOKMU4KX4RDRGB: "DeFindex-Vault-targetAPY",
  CB5YXWIDBQAOTTPEQE3SRNUFM2PTOXFHKGUWCBJJSF2GPW37DN725FDA: "DeFindex-Vault-controlAPY",
  CCA2ZJP5BVRXYTQH4FAGHCAUMRYCXVC4CRYC2NXHWMR7TIVX36U7F5HR: "Meru",
  CD3HR7WNGPDUGK5ITNMZSRM36O2IFJF3N4RFHOITP4DCXMVGHMANN3XR: "DeFindex-Vault-variableAPY",
  CD7T34Y5SZ6MBEZDMXDIQWQ6JICO7TYH7E6DKZJ7BHXOMR2EQ65WYSZG: "DeFindex-Vault-boostAPY",
};

// 0.1 USDC at 7 decimals = 1_000_000 stroops.
export const DEPOSIT_AMOUNT_STROOPS = 1_000_000n;

// Withdraw is denominated in vault shares, not USDC. Initial 1:1 vaults make
// these comparable; for non-1:1 vaults this should ideally track shares minted
// per deposit, but a fixed value is good enough for the stabilization cycle.
export const WITHDRAW_SHARES = 1_000_000n;

export const MIN_AMOUNT_OUT_STROOPS = 0n;
export const INVEST = true;

export const NETWORK_PASSPHRASE = Networks.PUBLIC;

export const TX_FEE = "2000";
export const TX_TIMEOUT_SECONDS = 300;

export const POLL_INTERVAL_MS = 2_000;
export const POLL_TIMEOUT_MS = 60_000;

// 4 hours between cron ticks.
export const CRON_INTERVAL_MS = 4 * 60 * 60 * 1000;

// 1 hour between APY-history poll ticks.
export const APY_POLL_INTERVAL_MS = 60 * 60 * 1000;

// Network passed to the partner /vault/:id/apy endpoint.
export const APY_POLL_NETWORK = "mainnet";
