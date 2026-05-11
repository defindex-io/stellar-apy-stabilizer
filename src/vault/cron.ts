import "dotenv/config";
import { CRON_INTERVAL_MS } from "../constants/index";
import { log, sleep } from "../helpers/log";
import { getCallerKeypair } from "../helpers/stellar";
import { depositToVault, runOnAllVaults, withdrawFromVault } from "../helpers/vault";

const BANNER = `
░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░   ░▒▓████████▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓████████▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
   ░▒▓██▓▒░  ░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓████████▓▒░▒▓█▓▒░
╔══════════════════════════════════════════════╗
║   DEFINDEX VAULT  ·  CRON  (deposit ⇄ withdraw)
╚══════════════════════════════════════════════╝
`;

type Action = "deposit" | "withdraw";

async function runTick(action: Action): Promise<void> {
  const kp = getCallerKeypair();
  const fn = action === "deposit" ? depositToVault : withdrawFromVault;

  log(`=== tick: ${action} ===`);
  const failures = await runOnAllVaults(action, fn, kp);
  log(`=== tick done (failures: ${failures}) ===`);
}

async function main(): Promise<void> {
  console.log(BANNER);
  log(`cron started · interval ${CRON_INTERVAL_MS}ms`);

  let action: Action = "deposit";
  while (true) {
    try {
      await runTick(action);
    } catch (err) {
      log(`tick crashed: ${(err as Error).message}`);
    }

    log(`sleeping ${CRON_INTERVAL_MS}ms until next tick`);
    await sleep(CRON_INTERVAL_MS);
    action = action === "deposit" ? "withdraw" : "deposit";
  }
}

main();
