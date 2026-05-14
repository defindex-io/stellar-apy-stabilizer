import { Pool } from "pg";

let pool: Pool | null = null;

export function getPool(): Pool {
  if (pool) return pool;
  pool = new Pool({ connectionString: process.env.DATABASE_URL as string });
  return pool;
}

export async function ensureApyHistoryTable(): Promise<void> {
  await getPool().query(`
    CREATE TABLE IF NOT EXISTS apy_history (
      id BIGSERIAL PRIMARY KEY,
      recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      vault_address TEXT NOT NULL,
      vault_name TEXT NOT NULL,
      apy NUMERIC NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_apy_history_vault_time
      ON apy_history (vault_address, recorded_at DESC);
  `);
}

export async function insertApySample(
  vaultAddress: string,
  vaultName: string,
  apy: number,
): Promise<void> {
  await getPool().query(
    `INSERT INTO apy_history (vault_address, vault_name, apy) VALUES ($1, $2, $3)`,
    [vaultAddress, vaultName, apy],
  );
}
