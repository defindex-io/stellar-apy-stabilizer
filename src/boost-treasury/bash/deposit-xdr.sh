#!/usr/bin/env bash
#
# Build an UNSIGNED deposit transaction for a BoostTreasury campaign and print
# the base64 XDR, ready to be signed offline (e.g. by a multisig account).
# Nothing is submitted to the network.
#
# Reads the boost-treasury address from <workspace_root>/<network>.contracts.json
# (falls back to prompting if the file or key is missing).
#
# Usage:
#   Interactive:  ./deposit-xdr.sh
#   Positional:   ./deposit-xdr.sh <network> <depositor_pubkey> <vault> <amount>
#
# <depositor_pubkey> is a G... public key (no local identity / secret needed):
# it becomes both the transaction source and the deposit `caller`, so the
# envelope signatures satisfy the contract's `caller.require_auth()` — exactly
# what a classic multisig account needs.
#
# <amount> is the token's raw stroops/units (positive i128). The depositor
# needs a sufficient balance (and trustline) in the campaign's asset.
#
# The transaction is valid for 1 hour after this script runs, leaving time to
# collect multisig signatures. The sequence number is captured at build time:
# the depositor account must not submit any other transaction before this one.

set -euo pipefail

cat <<'BANNER'
░▒▓███████▓▒░ ░▒▓██████▓▒░ ░▒▓██████▓▒░ ░▒▓███████▓▒░▒▓████████▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░         ░▒▓█▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░         ░▒▓█▓▒░
░▒▓███████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░   ░▒▓█▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░      ░▒▓█▓▒░  ░▒▓█▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░      ░▒▓█▓▒░  ░▒▓█▓▒░
░▒▓███████▓▒░ ░▒▓██████▓▒░ ░▒▓██████▓▒░░▒▓███████▓▒░   ░▒▓█▓▒░

░▒▓████████▓▒░▒▓███████▓▒░░▒▓████████▓▒░░▒▓██████▓▒░ ░▒▓███████▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓███████▓▒░░▒▓█▓▒░░▒▓█▓▒░
   ░▒▓█▓▒░   ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░
   ░▒▓█▓▒░   ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░
   ░▒▓█▓▒░   ░▒▓███████▓▒░░▒▓██████▓▒░ ░▒▓████████▓▒░░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓███████▓▒░ ░▒▓██████▓▒░
   ░▒▓█▓▒░   ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░      ░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
   ░▒▓█▓▒░   ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░░▒▓█▓▒░      ░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
   ░▒▓█▓▒░   ░▒▓█▓▒░░▒▓█▓▒░▒▓████████▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓███████▓▒░ ░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░  ░▒▓█▓▒░
╔══════════════════════════════════════════════╗
║   BOOST TREASURY  ·  DEPOSIT  ·  XDR ONLY    ║
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
readonly WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
readonly BOOST_TREASURY_KEY="boost-treasury"

# 1 hour to collect multisig signatures before the tx expires.
readonly TX_VALID_SECONDS=3600

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

require_positive_i128() {
  local label="$1" value="$2"
  # Positive integer, no leading zero. The contract rejects <= 0; i128 upper
  # bound is left to the network to enforce.
  [[ "$value" =~ ^[1-9][0-9]*$ ]] \
    || die "$label must be a positive integer, got '$value'"
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

ensure_depositor() {
  [[ "$DEPOSITOR" =~ ^G[A-Z2-7]{55}$ ]] \
    || die "depositor must be a public key (G..., 56 chars), got '$DEPOSITOR'"

  local balance_output balance_stroops balance_xlm
  if ! balance_output="$(run_with_spinner "checking XLM balance..." \
    stellar contract invoke \
      --id "$XLM_SAC_ID" \
      --source-account "$DEPOSITOR" \
      --network "$NETWORK" \
      --send no \
      -- balance --id "$DEPOSITOR")"; then
    die "failed to fetch XLM balance for $DEPOSITOR:
$balance_output"
  fi

  balance_stroops="${balance_output//\"/}"
  [[ -n "$balance_stroops" && "$balance_stroops" != "0" ]] \
    || die "depositor has 0 XLM on $NETWORK. Fund $DEPOSITOR before retrying."

  balance_xlm="$(awk -v s="$balance_stroops" 'BEGIN { printf "%.7f", s / 10000000 }')"
  echo "✓ depositor $DEPOSITOR"
  echo "  balance: $balance_xlm XLM ($balance_stroops stroops)"
}

load_boost_treasury_id() {
  command -v jq >/dev/null 2>&1 || die "jq is required (install: brew install jq)"

  local contracts_file="$WORKSPACE_ROOT/$NETWORK.contracts.json"
  if [[ -f "$contracts_file" ]]; then
    BOOST_TREASURY_ID="$(jq -r --arg k "$BOOST_TREASURY_KEY" '.[$k] // empty' "$contracts_file")"
    if [[ -n "$BOOST_TREASURY_ID" ]]; then
      echo "✓ boost-treasury ($NETWORK): $BOOST_TREASURY_ID  (from $contracts_file)"
      return
    fi
    echo "⚠  '$BOOST_TREASURY_KEY' not found in $contracts_file"
  else
    echo "⚠  $contracts_file not found"
  fi
  BOOST_TREASURY_ID=$(prompt_required "BoostTreasury contract id")
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

DEPOSITOR=$(resolve_required "Depositor public key (tx source, e.g. the multisig account)" "${2-__UNSET__}")

ensure_network
ensure_depositor
load_boost_treasury_id
echo

VAULT=$(resolve_required  "Vault contract id"            "${3-__UNSET__}")
AMOUNT=$(resolve_required "Amount (positive i128 units)" "${4-__UNSET__}")

require_positive_i128 "amount" "$AMOUNT"

echo
echo "──────────────────────────────────────"
echo " network:         $NETWORK"
echo " depositor:       $DEPOSITOR"
echo " boost-treasury:  $BOOST_TREASURY_ID"
echo " vault:           $VAULT"
echo " amount:          $AMOUNT"
echo " tx validity:     $TX_VALID_SECONDS s"
echo "──────────────────────────────────────"

read -rp "Build unsigned XDR now? [y/N] " confirm
[[ "$confirm" =~ ^[yY]$ ]] || { echo "aborted"; exit 0; }

# Build the invoke transaction without signing or sending. The depositor's
# current sequence number is baked in here.
BUILT_XDR="$(stellar contract invoke \
  --id "$BOOST_TREASURY_ID" \
  --source-account "$DEPOSITOR" \
  --network "$NETWORK" \
  --build-only \
  -- deposit \
  --caller "$DEPOSITOR" \
  --vault "$VAULT" \
  --amount "$AMOUNT")"

# Simulate to attach the Soroban footprint and resource fee. This also
# verifies the deposit would succeed (active campaign, balance, trustline).
if ! SIM_XDR="$(run_with_spinner "simulating transaction..." \
  stellar tx simulate \
    --source-account "$DEPOSITOR" \
    --network "$NETWORK" \
    "$BUILT_XDR")"; then
  die "simulation failed — the deposit would not succeed as built:
$SIM_XDR"
fi

# `contract invoke --build-only` sets no time bounds (valid forever); bound it
# to now + TX_VALID_SECONDS so an unsubmitted envelope eventually dies.
EXPIRES_AT=$(( $(date +%s) + TX_VALID_SECONDS ))
UNSIGNED_XDR="$(printf '%s' "$SIM_XDR" \
  | stellar tx decode \
  | jq -c --argjson mt "$EXPIRES_AT" '.tx.tx.cond = {time: {min_time: 0, max_time: $mt}}' \
  | stellar tx encode)"

TX_HASH="$(stellar tx hash --network "$NETWORK" "$UNSIGNED_XDR")"

echo
echo "✅ Unsigned deposit transaction built"
echo
echo " tx hash:     $TX_HASH"
if EXPIRES_HUMAN="$(date -r "$EXPIRES_AT" 2>/dev/null || date -d "@$EXPIRES_AT" 2>/dev/null)"; then
  echo " expires at:  $EXPIRES_HUMAN (unix $EXPIRES_AT)"
else
  echo " expires at:  unix $EXPIRES_AT"
fi
echo
echo "──────────────── XDR (base64) ────────────────"
echo "$UNSIGNED_XDR"
echo "──────────────────────────────────────────────"
echo
echo "Next steps:"
echo "  1. Collect signatures (each signer):"
echo "       stellar tx sign --sign-with-key <key> --network $NETWORK <XDR>"
echo "     or sign in Stellar Lab / your multisig coordinator."
echo "  2. Submit the signed envelope within the validity window:"
echo "       stellar tx send --network $NETWORK <SIGNED_XDR>"
echo
echo "⚠  Sequence number was captured at build time: $DEPOSITOR must not"
echo "   submit any other transaction before this one, or it will fail."
