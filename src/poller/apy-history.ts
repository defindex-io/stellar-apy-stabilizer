import "dotenv/config";
import {
  APY_POLL_INTERVAL_MS,
  APY_POLL_NETWORK,
  VAULTS,
  VAULT_NAMES,
} from "../constants";
import { ensureApyHistoryTable, insertApySample } from "../helpers/db";
import { log, sleep } from "../helpers/log";

const BANNER = `
░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░   ░▒▓████████▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓████████▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
   ░▒▓██▓▒░  ░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓████████▓▒░▒▓█▓▒░
╔══════════════════════════════════════════════╗
║   DEFINDEX VAULT  ·  APY HISTORY POLLER      ║
╚══════════════════════════════════════════════╝
`;

interface ApyResponse {
  apy: number;
}

async function fetchVaultApy(vaultAddress: string): Promise<number> {
  const baseUrl = process.env.DEFINDEX_API_URL as string;
  const apiKey = process.env.DEFINDEX_API_KEY as string;
  const url = `${baseUrl}/vault/${vaultAddress}/apy?network=${APY_POLL_NETWORK}`;

  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${apiKey}` },
  });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} from ${url}`);
  }

  const body = (await res.json()) as ApyResponse;
  if (typeof body?.apy !== "number") {
    throw new Error(`unexpected response shape from ${url}: ${JSON.stringify(body)}`);
  }
  return body.apy;
}

async function recordOneVault(vaultAddress: string): Promise<void> {
  const vaultName = VAULT_NAMES[vaultAddress] ?? vaultAddress;
  try {
    const apy = await fetchVaultApy(vaultAddress);
    await insertApySample(vaultAddress, vaultName, apy);
    log(`✓ ${vaultName} (${vaultAddress}) → ${apy}%`);
  } catch (err) {
    log(`✗ ${vaultName} (${vaultAddress}) failed: ${(err as Error).message}`);
  }
}

async function runTick(): Promise<void> {
  log(`=== poll tick (${VAULTS.length} vaults) ===`);
  for (const vault of VAULTS) {
    await recordOneVault(vault);
  }
}

async function main(): Promise<void> {
  console.log(BANNER);

  await ensureApyHistoryTable();
  log(`apy-history poller started · interval ${APY_POLL_INTERVAL_MS}ms`);

  while (true) {
    try {
      await runTick();
    } catch (err) {
      log(`tick crashed: ${(err as Error).message}`);
    }
    log(`sleeping ${APY_POLL_INTERVAL_MS}ms until next tick`);
    await sleep(APY_POLL_INTERVAL_MS);
  }
}

main();
