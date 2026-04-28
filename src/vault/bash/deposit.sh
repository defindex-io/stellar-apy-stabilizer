#!/usr/bin/env bash
#
# Deposit into a DeFindex vault.
#
# Wraps `stellar contract invoke -- deposit`. Amounts are entered as human-readable
# floats with up to 7 fractional digits and converted to the i128 stroop units the
# contract expects (e.g. 0.1 → 1000000). Multi-asset vaults take a comma-separated
# list: "0.1,0.5" → [1000000, 5000000].
#
# Usage:
#   Interactive:  ./deposit.sh
#   Positional:   ./deposit.sh <network> <source_account> <vault> \
#                              <amounts_desired> <amounts_min> <invest>
#
# Pass "-" for any optional arg to accept its default.

set -euo pipefail

cat <<'BANNER'
░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░   ░▒▓████████▓▒░
░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
 ░▒▓█▓▒▒▓█▓▒░░▒▓████████▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
  ░▒▓█▓▓█▓▒░ ░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░░▒▓█▓▒░▒▓█▓▒░      ░▒▓█▓▒░
   ░▒▓██▓▒░  ░▒▓█▓▒░░▒▓█▓▒░░▒▓██████▓▒░░▒▓████████▓▒░▒▓█▓▒░
╔══════════════════════════════════════════════╗
║   DEFINDEX VAULT  ·  DEPOSIT                 ║
╚══════════════════════════════════════════════╝
BANNER

# --- Network constants ---

readonly MAINNET_RPC_URL="https://rpc.lightsail.network"
readonly MAINNET_PASSPHRASE="Public Global Stellar Network ; September 2015"
readonly MAINNET_XLM_SAC="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"

readonly TESTNET_RPC_URL="https://soroban-testnet.stellar.org"
readonly TESTNET_PASSPHRASE="Test SDF Network ; September 2015"
readonly TESTNET_XLM_SAC="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"

readonly TOKEN_DECIMALS=7

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

# Convert a human-readable decimal string (e.g. "0.1", "1.5", "1000") to the
# integer stroop representation with TOKEN_DECIMALS digits. Pure string math so
# we don't lose precision on binary floats.
float_to_stroops() {
  local value="$1"
  local sign="" int_part frac_part=""

  # Strip optional leading sign
  if [[ "$value" == -* ]]; then
    sign="-"
    value="${value#-}"
  fi

  if [[ "$value" == *.* ]]; then
    int_part="${value%%.*}"
    frac_part="${value##*.}"
  else
    int_part="$value"
  fi

  [[ "$int_part" =~ ^[0-9]+$ ]] || die "invalid number: '$1'"
  [[ -z "$frac_part" || "$frac_part" =~ ^[0-9]+$ ]] || die "invalid number: '$1'"
  (( ${#frac_part} <= TOKEN_DECIMALS )) \
    || die "too many fractional digits in '$1' (max $TOKEN_DECIMALS)"

  # Pad fractional to exactly TOKEN_DECIMALS digits
  while (( ${#frac_part} < TOKEN_DECIMALS )); do
    frac_part="${frac_part}0"
  done

  local combined="${int_part}${frac_part}"
  # Strip leading zeros but keep at least "0"
  combined="${combined#"${combined%%[!0]*}"}"
  [[ -z "$combined" ]] && combined="0"

  printf '%s%s' "$sign" "$combined"
}

# Convert a comma-separated list of floats to a JSON array of stroop strings.
# "0.1,0.5" → ["1000000","5000000"]
# stellar-cli expects i128 values as quoted strings inside Vec<i128> JSON.
floats_to_json_array() {
  local csv="$1"
  local -a parts=()
  local part stroops
  IFS=',' read -ra raw <<< "$csv"
  for part in "${raw[@]}"; do
    # trim whitespace
    part="${part#"${part%%[![:space:]]*}"}"
    part="${part%"${part##*[![:space:]]}"}"
    [[ -n "$part" ]] || die "empty entry in list: '$csv'"
    stroops="$(float_to_stroops "$part")"
    parts+=("\"$stroops\"")
  done
  local joined
  joined="$(IFS=','; printf '%s' "${parts[*]}")"
  printf '[%s]' "$joined"
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

SOURCE_ACCOUNT=$(resolve_required "Signer identity (also the depositor 'from')" "${2-__UNSET__}")

ensure_network
ensure_signer
echo

VAULT=$(resolve_required              "Vault contract id"                                       "${3-__UNSET__}")
AMOUNTS_DESIRED_RAW=$(resolve_with_default "Amounts desired (comma-separated floats, e.g. 0.1,0.5)" "0.1"   "${4-__UNSET__}")
AMOUNTS_MIN_RAW=$(resolve_with_default     "Amounts min (comma-separated floats)"                   "0"     "${5-__UNSET__}")
INVEST=$(resolve_with_default         "Invest (true/false)"                                     "true"  "${6-__UNSET__}")

case "$INVEST" in
  true|false) ;;
  *) die "invest must be 'true' or 'false', got '$INVEST'" ;;
esac

AMOUNTS_DESIRED_JSON="$(floats_to_json_array "$AMOUNTS_DESIRED_RAW")"
AMOUNTS_MIN_JSON="$(floats_to_json_array "$AMOUNTS_MIN_RAW")"

# Auto-pad amounts_min with leading zeros if it's shorter than amounts_desired.
# Convenience so users can type "0" to mean "no slippage floor for any asset".
desired_len="$(awk -F',' '{print NF}' <<< "$AMOUNTS_DESIRED_JSON")"
min_len="$(awk -F',' '{print NF}' <<< "$AMOUNTS_MIN_JSON")"
if (( desired_len > min_len )); then
  if [[ "$AMOUNTS_MIN_RAW" == "0" ]]; then
    # Expand single "0" to one zero per desired asset.
    padded=""
    for ((i=0; i<desired_len; i++)); do
      (( i > 0 )) && padded+=","
      padded+="\"0\""
    done
    AMOUNTS_MIN_JSON="[$padded]"
    min_len=$desired_len
  fi
fi
(( desired_len == min_len )) \
  || die "amounts_desired has $desired_len entries but amounts_min has $min_len"

echo
echo "──────────────────────────────────────"
echo " network:           $NETWORK"
echo " signer (from):     $SOURCE_ACCOUNT ($SIGNER_PUBKEY)"
echo " vault:             $VAULT"
echo " amounts_desired:   $AMOUNTS_DESIRED_RAW  →  $AMOUNTS_DESIRED_JSON"
echo " amounts_min:       $AMOUNTS_MIN_RAW  →  $AMOUNTS_MIN_JSON"
echo " invest:            $INVEST"
echo "──────────────────────────────────────"
echo
echo "one-liner (copy/paste):"
echo "stellar contract invoke --id $VAULT --source-account $SOURCE_ACCOUNT --network $NETWORK -- deposit --amounts_desired '$AMOUNTS_DESIRED_JSON' --amounts_min '$AMOUNTS_MIN_JSON' --from $SIGNER_PUBKEY --invest $INVEST"
echo

read -rp "Deposit now? [y/N] " confirm
[[ "$confirm" =~ ^[yY]$ ]] || { echo "aborted"; exit 0; }

stellar contract invoke \
  --id "$VAULT" \
  --source-account "$SOURCE_ACCOUNT" \
  --network "$NETWORK" \
  -- deposit \
  --amounts_desired "$AMOUNTS_DESIRED_JSON" \
  --amounts_min "$AMOUNTS_MIN_JSON" \
  --from "$SIGNER_PUBKEY" \
  --invest "$INVEST"

echo
echo "✅ Deposit submitted to vault $VAULT"
