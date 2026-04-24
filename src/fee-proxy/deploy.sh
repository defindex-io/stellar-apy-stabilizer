#!/usr/bin/env bash
#
# Deploy the FeeProxy contract on testnet or mainnet from a local WASM build.
#
# Usage:
#   Interactive:  ./deploy.sh
#   Positional:   ./deploy.sh <network> <source_account> <admin> <fee_manager>
#
# Pass "-" for <admin> to default it to the deployer's own public key.
# Any missing positional arg is prompted for interactively.

set -euo pipefail

cat <<'BANNER'
░▒▓████████▓▒░▒▓████████▓▒░▒▓████████▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░      ░▒▓█▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░      ░▒▓█▓▒░
░▒▓██████▓▒░ ░▒▓██████▓▒░ ░▒▓██████▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░      ░▒▓█▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░      ░▒▓█▓▒░
░▒▓█▓▒░      ░▒▓████████▓▒░▒▓████████▓▒░

░▒▓███████▓▒░░▒▓███████▓▒░ ░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░
░▒▓███████▓▒░░▒▓███████▓▒░░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░ ░▒▓██████▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
BANNER

# --- Network constants ---

readonly MAINNET_RPC_URL="https://rpc.lightsail.network"
readonly MAINNET_PASSPHRASE="Public Global Stellar Network ; September 2015"
readonly MAINNET_XLM_SAC="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"

readonly TESTNET_RPC_URL="https://soroban-testnet.stellar.org"
readonly TESTNET_PASSPHRASE="Test SDF Network ; September 2015"
readonly TESTNET_XLM_SAC="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
readonly WASM_PATH="$WORKSPACE_ROOT/target/wasm32v1-none/release/fee_proxy.wasm"

# --- Helpers ---

die() { echo "error: $*" >&2; exit 1; }

prompt_required() {
  local label="$1" answer
  while true; do
    read -rp "$label: " answer
    [[ -n "$answer" ]] && { printf '%s' "$answer"; return; }
    echo "  value required" >&2
  done
}

prompt_default() {
  local label="$1" default="$2" answer
  read -rp "$label [$default]: " answer
  printf '%s' "${answer:-$default}"
}

# Resolve a positional arg: prompt when unset, use default when empty or "-",
# otherwise use the supplied value.
resolve_with_default() {
  local label="$1" default="$2" supplied="${3-__UNSET__}"
  if [[ "$supplied" == "__UNSET__" ]]; then
    prompt_default "$label" "$default"
  elif [[ -z "$supplied" || "$supplied" == "-" ]]; then
    printf '%s' "$default"
  else
    printf '%s' "$supplied"
  fi
}

resolve_required() {
  local label="$1" supplied="${2-__UNSET__}"
  if [[ "$supplied" == "__UNSET__" ]]; then
    prompt_required "$label"
  else
    [[ -n "$supplied" && "$supplied" != "-" ]] || die "$label is required"
    printf '%s' "$supplied"
  fi
}

# Rotating spinner — command's stdout+stderr is captured and replayed, spinner
# renders on stderr so this wrapper is safe inside $(...) substitutions.
run_with_spinner() {
  local label="$1"; shift
  local tmp rc_file rc
  tmp="$(mktemp)"
  rc_file="$(mktemp)"

  ( "$@" >"$tmp" 2>&1; echo $? >"$rc_file" ) &
  local pid=$!

  local chars='|/-\' i=0
  while kill -0 "$pid" 2>/dev/null; do
    printf '\r  %s %s' "${chars:$((i++ % ${#chars})):1}" "$label" >&2
    sleep 0.1
  done
  wait "$pid" 2>/dev/null || true
  printf '\r\033[K' >&2

  rc="$(cat "$rc_file")"
  rm -f "$rc_file"
  cat "$tmp"
  rm -f "$tmp"
  return "${rc:-1}"
}

# --- Ensure network exists in stellar CLI config ---
ensure_network() {
  if stellar network ls 2>/dev/null | grep -qw "$NETWORK"; then
    echo "✓ network '$NETWORK' configured"
  else
    echo "→ adding network '$NETWORK'"
    stellar network add "$NETWORK" \
      --rpc-url "$NETWORK_RPC_URL" \
      --network-passphrase "$NETWORK_PASSPHRASE"
  fi
}

# --- Ensure deployer identity exists and has a funded balance ---
ensure_deployer() {
  if ! stellar keys ls 2>/dev/null | grep -qw "$SOURCE_ACCOUNT"; then
    die "identity '$SOURCE_ACCOUNT' not found. Create one first with:
    stellar keys generate $SOURCE_ACCOUNT --network $NETWORK     # new key
    stellar keys add $SOURCE_ACCOUNT --secret-key                # import existing
  Then fund the account with XLM before re-running this script."
  fi

  DEPLOYER_PUBKEY="$(stellar keys public-key "$SOURCE_ACCOUNT")"
  echo "✓ identity '$SOURCE_ACCOUNT' → $DEPLOYER_PUBKEY"

  local balance_output balance_stroops balance_xlm
  if ! balance_output="$(run_with_spinner "checking XLM balance..." \
    stellar contract invoke \
      --id "$XLM_SAC_ID" \
      --source-account "$SOURCE_ACCOUNT" \
      --network "$NETWORK" \
      --send no \
      -- balance --id "$DEPLOYER_PUBKEY")"; then
    die "failed to fetch XLM balance for $DEPLOYER_PUBKEY:
$balance_output"
  fi

  balance_stroops="${balance_output//\"/}"

  if [[ -z "$balance_stroops" || "$balance_stroops" == "0" ]]; then
    die "deployer has 0 XLM on $NETWORK. Fund $DEPLOYER_PUBKEY before retrying."
  fi

  balance_xlm="$(awk -v s="$balance_stroops" 'BEGIN { printf "%.7f", s / 10000000 }')"
  echo "  balance: $balance_xlm XLM ($balance_stroops stroops)"
}

# --- Build WASM if missing ---
ensure_wasm() {
  if [[ -f "$WASM_PATH" ]]; then
    echo "✓ wasm found: $WASM_PATH"
    return
  fi
  echo "→ wasm not found, building fee-proxy..."
  ( cd "$WORKSPACE_ROOT" && stellar contract build --package fee-proxy )
  [[ -f "$WASM_PATH" ]] || die "build completed but wasm not at expected path: $WASM_PATH"
  echo "✓ built: $WASM_PATH"
}

# --- Collect args ---

NETWORK=$(resolve_with_default "Network (testnet/mainnet)" "testnet" "${1-__UNSET__}")
case "$NETWORK" in
  mainnet)
    NETWORK_RPC_URL="$MAINNET_RPC_URL"
    NETWORK_PASSPHRASE="$MAINNET_PASSPHRASE"
    XLM_SAC_ID="$MAINNET_XLM_SAC"
    ;;
  testnet)
    NETWORK_RPC_URL="$TESTNET_RPC_URL"
    NETWORK_PASSPHRASE="$TESTNET_PASSPHRASE"
    XLM_SAC_ID="$TESTNET_XLM_SAC"
    ;;
  *)
    die "unknown network: '$NETWORK' (expected 'testnet' or 'mainnet')"
    ;;
esac

SOURCE_ACCOUNT=$(resolve_required "Deployer identity (stellar keys name)" "${2-__UNSET__}")

ensure_network
ensure_deployer
ensure_wasm
echo

# Default admin to the deployer's pubkey — the constructor requires admin auth,
# and the source-account's signature only satisfies that if admin == deployer.
ADMIN=$(resolve_with_default     "Admin address"       "$DEPLOYER_PUBKEY" "${3-__UNSET__}")
FEE_MANAGER=$(resolve_required   "Fee manager address" "${4-__UNSET__}")

echo
echo "──────────────────────────────────────"
echo " network:      $NETWORK"
echo " source:       $SOURCE_ACCOUNT ($DEPLOYER_PUBKEY)"
echo " wasm:         $WASM_PATH"
echo " admin:        $ADMIN"
echo " fee_manager:  $FEE_MANAGER"
echo "──────────────────────────────────────"

if [[ "$ADMIN" != "$DEPLOYER_PUBKEY" ]]; then
  echo
  echo "⚠  admin ≠ deployer. The constructor calls admin.require_auth(); this will"
  echo "   fail unless the admin key signs the deployment transaction."
fi

read -rp "Deploy now? [y/N] " confirm
[[ "$confirm" =~ ^[yY]$ ]] || { echo "aborted"; exit 0; }

DEPLOYED_ADDRESS="$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" \
  -- \
  --admin "$ADMIN" \
  --fee_manager "$FEE_MANAGER")"

echo
echo "✅ Deployed FeeProxy: $DEPLOYED_ADDRESS"
