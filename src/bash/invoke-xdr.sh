#!/usr/bin/env bash
#
# Build an UNSIGNED invocation of any FeeProxy / BoostTreasury function and
# print the base64 XDR, ready to be signed offline (e.g. by a multisig
# account). Nothing is submitted to the network.
#
# The <source_pubkey> becomes the transaction source, so the envelope
# signatures satisfy the contract's `require_auth()` for that address —
# exactly what a classic multisig account needs. This only works when the
# address the contract authorizes IS the source account (true for all
# admin-gated functions once the admin is the multisig, and for
# accept_admin called by the incoming multisig admin).
#
# Usage:
#   Interactive:  ./invoke-xdr.sh
#   Positional:   ./invoke-xdr.sh <network> <source_pubkey> <contract> <function> [fn args...]
#
# <contract>  is either a key in <workspace_root>/<network>.contracts.json
#             ("fee-proxy" / "boost-treasury") or a literal C... address.
# [fn args]   are forwarded verbatim to the function, e.g. --new_admin G...
#
# Examples (multisig admin handoff and day-2 admin ops):
#   ./invoke-xdr.sh mainnet GMULTISIG... boost-treasury accept_admin --new_admin GMULTISIG...
#   ./invoke-xdr.sh mainnet GMULTISIG... fee-proxy accept_admin --new_admin GMULTISIG...
#   ./invoke-xdr.sh mainnet GMULTISIG... boost-treasury transfer --vault CVAULT... --amount 1000000 --to GDEST...
#   ./invoke-xdr.sh mainnet GMULTISIG... boost-treasury register_campaign --vault CVAULT... --asset CASSET...
#
# The transaction is valid for 1 hour after this script runs, leaving time to
# collect multisig signatures. The sequence number is captured at build time:
# the source account must not submit any other transaction before this one.

set -euo pipefail

cat <<'BANNER'
╔══════════════════════════════════════════════╗
║   APY STABILIZER  ·  INVOKE  ·  XDR ONLY     ║
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

# 1 hour to collect multisig signatures before the tx expires.
readonly TX_VALID_SECONDS=3600

# Inclusion-fee bid in stroops. The CLI default (100) is below the network
# minimum during mainnet fee surges and fails with txINSUFFICIENT_FEE even
# though the account is well funded. 0.1 XLM gives ample headroom; the
# resource fee is added on top by simulation.
# Override for exceptional congestion with:  INCLUSION_FEE=<stroops> ./invoke-xdr.sh
readonly DEFAULT_INCLUSION_FEE=1000000

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

ensure_source() {
  [[ "$SOURCE_PUBKEY" =~ ^G[A-Z2-7]{55}$ ]] \
    || die "source must be a public key (G..., 56 chars), got '$SOURCE_PUBKEY'"

  local balance_output balance_stroops balance_xlm
  if ! balance_output="$(run_with_spinner "checking XLM balance..." \
    stellar contract invoke \
      --id "$XLM_SAC_ID" \
      --source-account "$SOURCE_PUBKEY" \
      --network "$NETWORK" \
      --send no \
      -- balance --id "$SOURCE_PUBKEY")"; then
    die "failed to fetch XLM balance for $SOURCE_PUBKEY:
$balance_output"
  fi

  balance_stroops="${balance_output//\"/}"
  [[ -n "$balance_stroops" && "$balance_stroops" != "0" ]] \
    || die "source has 0 XLM on $NETWORK. Fund $SOURCE_PUBKEY before retrying."

  balance_xlm="$(awk -v s="$balance_stroops" 'BEGIN { printf "%.7f", s / 10000000 }')"
  echo "✓ source $SOURCE_PUBKEY"
  echo "  balance: $balance_xlm XLM ($balance_stroops stroops)"
}

# Resolve <contract> to a C... address: literal addresses pass through, any
# other value is looked up as a key in <network>.contracts.json.
resolve_contract_id() {
  local supplied="$1"
  if [[ "$supplied" =~ ^C[A-Z2-7]{55}$ ]]; then
    CONTRACT_ID="$supplied"
    echo "✓ contract: $CONTRACT_ID"
    return
  fi

  command -v jq >/dev/null 2>&1 || die "jq is required (install: brew install jq)"
  local contracts_file="$WORKSPACE_ROOT/$NETWORK.contracts.json"
  [[ -f "$contracts_file" ]] || die "$contracts_file not found (pass a C... address instead)"

  CONTRACT_ID="$(jq -r --arg k "$supplied" '.[$k] // empty' "$contracts_file")"
  [[ -n "$CONTRACT_ID" ]] \
    || die "'$supplied' not found in $contracts_file (pass a C... address instead)"
  echo "✓ $supplied ($NETWORK): $CONTRACT_ID  (from $contracts_file)"
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

SOURCE_PUBKEY=$(resolve_required "Source public key (the account require_auth authorizes, e.g. the multisig)" "${2-__UNSET__}")

ensure_network
ensure_source

CONTRACT=$(resolve_required "Contract (contracts.json key or C... address)" "${3-__UNSET__}")
resolve_contract_id "$CONTRACT"
echo

FUNCTION=$(resolve_required "Function name" "${4-__UNSET__}")

# Remaining positional args are forwarded verbatim; prompt for them as one
# line when running interactively (values must not contain spaces).
if [[ $# -ge 5 ]]; then
  FN_ARGS=("${@:5}")
else
  read -rp "Function args (e.g. --new_admin G..., empty for none): " -ra FN_ARGS
fi

INCLUSION_FEE="${INCLUSION_FEE:-$DEFAULT_INCLUSION_FEE}"
[[ "$INCLUSION_FEE" =~ ^[1-9][0-9]*$ ]] \
  || die "inclusion fee must be a positive integer, got '$INCLUSION_FEE'"

echo
echo "──────────────────────────────────────"
echo " network:        $NETWORK"
echo " source:         $SOURCE_PUBKEY"
echo " contract:       $CONTRACT_ID"
echo " function:       $FUNCTION ${FN_ARGS[*]-}"
echo " inclusion fee:  $INCLUSION_FEE stroops"
echo " tx validity:    $TX_VALID_SECONDS s"
echo "──────────────────────────────────────"

read -rp "Build unsigned XDR now? [y/N] " confirm
[[ "$confirm" =~ ^[yY]$ ]] || { echo "aborted"; exit 0; }

# Build the invoke transaction without signing or sending. The source's
# current sequence number is baked in here.
BUILT_XDR="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source-account "$SOURCE_PUBKEY" \
  --network "$NETWORK" \
  --inclusion-fee "$INCLUSION_FEE" \
  --build-only \
  -- "$FUNCTION" ${FN_ARGS+"${FN_ARGS[@]}"})"

# Simulate to attach the Soroban footprint and resource fee. This also
# verifies the invocation would succeed with the source's authorization.
if ! SIM_XDR="$(run_with_spinner "simulating transaction..." \
  stellar tx simulate \
    --source-account "$SOURCE_PUBKEY" \
    --network "$NETWORK" \
    "$BUILT_XDR")"; then
  die "simulation failed — the invocation would not succeed as built:
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
echo "✅ Unsigned $FUNCTION transaction built"
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
echo "⚠  Sequence number was captured at build time: $SOURCE_PUBKEY must not"
echo "   submit any other transaction before this one, or it will fail."
