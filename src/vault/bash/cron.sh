#!/usr/bin/env bash
#
# One deposit → withdraw cycle on all configured vaults, sequentially.
# Designed to be driven by cron — auto-answers "y" to each confirmation and
# keeps going when a single vault call fails (next one still runs).
#
# Usage:
#   Manually:   ./cron.sh
#   From cron:  * * * * * /abs/path/to/cron.sh >> /tmp/vault-cron.log 2>&1
#
# Tunables via env:
#   NETWORK  (default: mainnet)
#   SOURCE   (default: vaultManager)
#   AMOUNT   (default: 0.1)
#   INVEST   (default: true)
#
# All 4 vaults run sequentially because they share one source account and
# parallel submissions would collide on tx sequence numbers.

set -uo pipefail

# Cron runs with a minimal PATH; make sure `stellar` and friends are found.
export PATH="$HOME/.cargo/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin"

NETWORK="${NETWORK:-mainnet}"
SOURCE="${SOURCE:-vaultManager}"
AMOUNT="${AMOUNT:-0.1}"
INVEST="${INVEST:-true}"

readonly VAULTS=(
  "CD7T34Y5SZ6MBEZDMXDIQWQ6JICO7TYH7E6DKZJ7BHXOMR2EQ65WYSZG"
  "CB5YXWIDBQAOTTPEQE3SRNUFM2PTOXFHKGUWCBJJSF2GPW37DN725FDA"
  "CAEPJIHET2TBI2VCLJZI6QHMN366KUGNK4AOKE3YY7AOKMU4KX4RDRGB"
  "CD3HR7WNGPDUGK5ITNMZSRM36O2IFJF3N4RFHOITP4DCXMVGHMANN3XR"
)

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPOSIT="$SCRIPT_DIR/deposit.sh"
readonly WITHDRAW="$SCRIPT_DIR/withdraw.sh"

log() { printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*"; }

log "=== start cycle (network=$NETWORK source=$SOURCE amount=$AMOUNT invest=$INVEST) ==="

fail_count=0

for vault in "${VAULTS[@]}"; do
  log "--- vault $vault ---"

  log "→ deposit $AMOUNT"
  if printf 'y\n' | "$DEPOSIT" "$NETWORK" "$SOURCE" "$vault" "$AMOUNT" - "$INVEST"; then
    log "✓ deposit ok"
  else
    rc=$?
    log "✗ deposit failed (exit $rc)"
    fail_count=$((fail_count + 1))
  fi

  log "→ withdraw $AMOUNT"
  if printf 'y\n' | "$WITHDRAW" "$NETWORK" "$SOURCE" "$vault" "$AMOUNT" -; then
    log "✓ withdraw ok"
  else
    rc=$?
    log "✗ withdraw failed (exit $rc)"
    fail_count=$((fail_count + 1))
  fi
done

log "=== done (failures: $fail_count) ==="
exit "$fail_count"
