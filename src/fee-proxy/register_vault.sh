#!/usr/bin/env bash
#
# Register a vault with the deployed FeeProxy contract.
#
# Reads the fee-proxy address from <workspace_root>/<network>.contracts.json
# (falls back to prompting if the file or key is missing).
#
# Usage:
#   Interactive:  ./register_vault.sh
#   Positional:   ./register_vault.sh <network> <source_account> <vault> \
#                                      <config_admin> <target_apy_bps> \
#                                      <min_fee_bps> <max_fee_bps>
#
# Pass "-" for <config_admin> to default it to the signer's own public key.
# Any missing positional arg is prompted for interactively.
#
# The signer MUST be the vault's current Manager — the proxy will call
# `vault.set_manager(proxy)` and require that auth.

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
╔══════════════════════════════════════════════╗
║               REGISTER VAULT                 ║
╚══════════════════════════════════════════════╝
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
readonly FEE_PROXY_KEY="fee-proxy"

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

require_u32() {
  local label="$1" value="$2"
  [[ "$value" =~ ^[0-9]+$ ]] || die "$label must be a non-negative integer, got '$value'"
  # u32 max = 4_294_967_295
  (( value <= 4294967295 )) || die "$label exceeds u32::MAX (4294967295)"
}

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

ensure_signer() {
  if ! stellar keys ls 2>/dev/null | grep -qw "$SOURCE_ACCOUNT"; then
    die "identity '$SOURCE_ACCOUNT' not found. Create one first with:
    stellar keys generate $SOURCE_ACCOUNT --network $NETWORK
    stellar keys add $SOURCE_ACCOUNT --secret-key
  Then fund the account with XLM before re-running this script."
  fi

  SIGNER_PUBKEY="$(stellar keys public-key "$SOURCE_ACCOUNT")"
  echo "✓ identity '$SOURCE_ACCOUNT' → $SIGNER_PUBKEY"

  local balance_output balance_stroops balance_xlm
  if ! balance_output="$(run_with_spinner "checking XLM balance..." \
    stellar contract invoke \
      --id "$XLM_SAC_ID" \
      --source-account "$SOURCE_ACCOUNT" \
      --network "$NETWORK" \
      --send no \
      -- balance --id "$SIGNER_PUBKEY")"; then
    die "failed to fetch XLM balance for $SIGNER_PUBKEY:
$balance_output"
  fi

  balance_stroops="${balance_output//\"/}"
  [[ -n "$balance_stroops" && "$balance_stroops" != "0" ]] \
    || die "signer has 0 XLM on $NETWORK. Fund $SIGNER_PUBKEY before retrying."

  balance_xlm="$(awk -v s="$balance_stroops" 'BEGIN { printf "%.7f", s / 10000000 }')"
  echo "  balance: $balance_xlm XLM ($balance_stroops stroops)"
}

load_fee_proxy_id() {
  command -v jq >/dev/null 2>&1 || die "jq is required (install: brew install jq)"

  local contracts_file="$WORKSPACE_ROOT/$NETWORK.contracts.json"
  if [[ -f "$contracts_file" ]]; then
    FEE_PROXY_ID="$(jq -r --arg k "$FEE_PROXY_KEY" '.[$k] // empty' "$contracts_file")"
    if [[ -n "$FEE_PROXY_ID" ]]; then
      echo "✓ fee-proxy ($NETWORK): $FEE_PROXY_ID  (from $contracts_file)"
      return
    fi
    echo "⚠  '$FEE_PROXY_KEY' not found in $contracts_file"
  else
    echo "⚠  $contracts_file not found"
  fi
  FEE_PROXY_ID=$(prompt_required "FeeProxy contract id")
}

# --- Collect args ---

NETWORK=$(resolve_with_default "Network (testnet/mainnet)" "mainnet" "${1-__UNSET__}")
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
  *) die "unknown network: '$NETWORK' (expected 'testnet' or 'mainnet')" ;;
esac

SOURCE_ACCOUNT=$(resolve_required "Signer identity (must be the vault's current Manager)" "${2-__UNSET__}")

ensure_network
ensure_signer
load_fee_proxy_id
echo

VAULT=$(resolve_required            "Vault contract id"                                      "${3-__UNSET__}")
CONFIG_ADMIN=$(resolve_with_default "Config admin (address that will control via the proxy)" "$SIGNER_PUBKEY" "${4-__UNSET__}")
TARGET_APY_BPS=$(resolve_required   "Target APY (bps, u32)"                                  "${5-__UNSET__}")
MIN_FEE_BPS=$(resolve_with_default  "Min fee (bps)"                                          "0"   "${6-__UNSET__}")
MAX_FEE_BPS=$(resolve_required      "Max fee (bps, ≤ 10000)"                                 "${7-__UNSET__}")

require_u32 "target_apy_bps" "$TARGET_APY_BPS"
require_u32 "min_fee_bps"    "$MIN_FEE_BPS"
require_u32 "max_fee_bps"    "$MAX_FEE_BPS"
(( MIN_FEE_BPS <= MAX_FEE_BPS )) || die "min_fee_bps ($MIN_FEE_BPS) must be ≤ max_fee_bps ($MAX_FEE_BPS)"
(( MAX_FEE_BPS <= 10000 ))       || die "max_fee_bps ($MAX_FEE_BPS) must be ≤ 10000"

# Struct args are passed as JSON to `stellar contract invoke`.
CONFIG_JSON=$(printf '{"admin":"%s","target_apy_bps":%s,"max_fee_bps":%s,"min_fee_bps":%s}' \
  "$CONFIG_ADMIN" "$TARGET_APY_BPS" "$MAX_FEE_BPS" "$MIN_FEE_BPS")

echo
echo "──────────────────────────────────────"
echo " network:         $NETWORK"
echo " signer:          $SOURCE_ACCOUNT ($SIGNER_PUBKEY)"
echo " fee-proxy:       $FEE_PROXY_ID"
echo " vault:           $VAULT"
echo " config.admin:    $CONFIG_ADMIN"
echo " target_apy_bps:  $TARGET_APY_BPS"
echo " min_fee_bps:     $MIN_FEE_BPS"
echo " max_fee_bps:     $MAX_FEE_BPS"
echo "──────────────────────────────────────"
echo " config json:     $CONFIG_JSON"
echo "──────────────────────────────────────"

read -rp "Register now? [y/N] " confirm
[[ "$confirm" =~ ^[yY]$ ]] || { echo "aborted"; exit 0; }

stellar contract invoke \
  --id "$FEE_PROXY_ID" \
  --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" \
  -- register_vault \
  --admin "$SIGNER_PUBKEY" \
  --vault "$VAULT" \
  --config "$CONFIG_JSON"

echo
echo "✅ Vault $VAULT registered with FeeProxy $FEE_PROXY_ID"
