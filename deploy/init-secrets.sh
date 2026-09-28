#!/usr/bin/env bash
# Creates / completes deploy/.env from .env.example: every empty *_SECRET / *_PASSWORD
# gets a fresh random value. Values already present in .env are kept (safe to re-run);
# --force regenerates all secrets (only before the first `up` — volumes keep old ones).
# --set KEY=VALUE (repeatable) overrides a non-secret setting, e.g. --set LAB_MODE=proxy.
set -euo pipefail
cd "$(dirname "$0")"
force=0; declare -a sets=()
while [ $# -gt 0 ]; do
  case "$1" in
    --force) force=1; shift ;;
    --set) sets+=("$2"); shift 2 ;;
    *) echo "unknown option $1" >&2; exit 1 ;;
  esac
done
override() { local kv; for kv in ${sets[@]+"${sets[@]}"}; do [ "${kv%%=*}" = "$1" ] && { printf '%s' "${kv#*=}"; return 0; }; done; return 1; }

rand() { LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c "$1" || true; }
# satisfies the default password policy (upper + lower + digit, >= 8)
password() { printf '%sZs7k' "$(rand 16)"; }
current() { [ -f .env ] && grep -E "^$1=" .env | tail -1 | cut -d= -f2- || true; }

tmp=$(mktemp)
while IFS= read -r line || [ -n "$line" ]; do
  if [[ "$line" =~ ^([A-Z0-9_]+)=(.*)$ ]]; then
    key=${BASH_REMATCH[1]}; val=${BASH_REMATCH[2]}
    have=$(current "$key")
    [ -n "$have" ] && val=$have
    if o=$(override "$key"); then val=$o; have=$o; fi
    if { [ -z "$have" ] || [ $force = 1 ]; }; then
      case "$key" in
        *_SECRET) val=$(rand 48) ;;
        *_PASSWORD) val=$(password) ;;
      esac
    fi
    printf '%s=%s\n' "$key" "$val"
  else
    printf '%s\n' "$line"
  fi
done < .env.example > "$tmp"
mv "$tmp" .env
chmod 600 .env
echo "wrote $(pwd)/.env"
