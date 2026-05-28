import { Pool } from "pg";
import type {
  ManagedVaultApyData,
  SupportedNetwork,
  VaultStateSnapshot,
} from "./types";

let pool: Pool | null = null;

function getIndexerPool(): Pool {
  if (pool) return pool;
  const conn = process.env.INDEXER_DATABASE_URL;
  if (!conn) throw new Error("INDEXER_DATABASE_URL is not set");
  pool = new Pool({ connectionString: conn });
  return pool;
}

// ---- getManagedVaultsWithApy -----------------------------------------------

interface VaultApyRow {
  vault_id: string;
  vault_fee_bps: number;
  strategy_apy_24h: number | null;
  strategy_apy_7d: number | null;
  apy_7d_net: number | null;
  tvl: string;
  total_supply: string;
  current_pps_net: number | null;
}

const VAULT_APY_COLUMNS = `
  v.vault_id,
  v.vault_fee_bps,
  v.strategy_apy_24h,
  v.strategy_apy_7d,
  v.apy_7d_net,
  v.tvl,
  v.total_supply,
  v.current_pps_net`;

function mapVaultApyRow(row: VaultApyRow): ManagedVaultApyData {
  return {
    vaultId: row.vault_id,
    vaultFeeBps: row.vault_fee_bps ?? 0,
    strategyApy24h: row.strategy_apy_24h,
    strategyApy7d: row.strategy_apy_7d,
    apy7dNet: row.apy_7d_net,
    tvl: row.tvl ?? "0",
    totalSupply: row.total_supply ?? "0",
    currentPpsNet: row.current_pps_net,
  };
}

/**
 * Discover all vaults whose most recent `manager` role-change pointed at the
 * given proxy. role_type filter is critical (see API source comment).
 */
export async function getManagedVaultsWithApy(
  _network: SupportedNetwork,
  proxyAddress: string,
): Promise<ManagedVaultApyData[]> {
  const sql = `
    SELECT ${VAULT_APY_COLUMNS}
    FROM parsed.v_vault_apy v
    JOIN (
      SELECT DISTINCT ON (vault_id) vault_id, new_address
      FROM parsed.vault_role_change
      WHERE role_type = 'manager'
      ORDER BY vault_id, ledger DESC
    ) r ON r.vault_id = v.vault_id
    WHERE r.new_address = $1
  `;
  const result = await getIndexerPool().query<VaultApyRow>(sql, [proxyAddress]);
  return result.rows.map(mapVaultApyRow);
}

// ---- getVaultHistoricalStates ---------------------------------------------

interface HistoricalStateRow {
  tx_id: string;
  vault_id: string;
  event_type: "deposit" | "withdraw";
  df_token_amount: string;
  total_supply_before: string;
  ledger: number;
  timestamp: Date;
  asset: string;
  idle_amount: string;
  invested_amount: string;
  total_amount: string;
}

export async function getVaultHistoricalStates(
  _network: SupportedNetwork,
  vaultAddress: string,
  fromDate: Date,
  toDate: Date,
): Promise<VaultStateSnapshot[]> {
  const sql = `
    SELECT
      vt.id as tx_id,
      vt.vault_id,
      vt.event_type,
      vt.df_token_amount,
      vt.total_supply_before,
      vt.ledger,
      vt.timestamp,
      vta.asset,
      vta.idle_amount,
      vta.invested_amount,
      vta.total_amount
    FROM parsed.vault_transaction vt
    JOIN parsed.vault_transaction_asset vta ON vta.vault_transaction_id = vt.id
    WHERE vt.vault_id = $1
      AND vt.timestamp >= $2
      AND vt.timestamp <= $3
    ORDER BY vt.timestamp ASC, vt.ledger ASC, vta.asset
  `;
  const result = await getIndexerPool().query<HistoricalStateRow>(sql, [
    vaultAddress,
    fromDate,
    toDate,
  ]);

  // Group rows by transaction id; each tx becomes one snapshot whose
  // totalManagedFundsBefore is the array of per-asset rows.
  const byTx = new Map<string, { meta: HistoricalStateRow; assets: VaultStateSnapshot["totalManagedFundsBefore"] }>();
  for (const row of result.rows) {
    let entry = byTx.get(row.tx_id);
    if (!entry) {
      entry = { meta: row, assets: [] };
      byTx.set(row.tx_id, entry);
    }
    entry.assets.push({ asset: row.asset, total_amount: row.total_amount });
  }

  return Array.from(byTx.values()).map(({ meta, assets }) => ({
    vaultAddress: meta.vault_id,
    date: new Date(meta.timestamp),
    totalSupplyBefore: meta.total_supply_before,
    totalManagedFundsBefore: assets,
    ledger: meta.ledger,
  }));
}

// ---- getHistoricalLockedFees ----------------------------------------------

export async function getHistoricalLockedFees(
  _network: SupportedNetwork,
  vaultAddress: string,
  asOfLedger: number,
): Promise<string> {
  const sql = `
    SELECT COALESCE(SUM(locked_fee::numeric), 0)::text as total
    FROM (
      SELECT DISTINCT ON (strategy_id) locked_fee
      FROM parsed.vault_strategy_report
      WHERE vault_id = $1 AND ledger <= $2
      ORDER BY strategy_id, ledger DESC
    ) latest_per_strategy
  `;
  const result = await getIndexerPool().query<{ total: string }>(sql, [
    vaultAddress,
    asOfLedger,
  ]);
  return result.rows[0]?.total ?? "0";
}
