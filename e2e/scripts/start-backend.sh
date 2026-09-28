#!/usr/bin/env bash
# Starts the Rust backend on a fresh temporary SQLite database for E2E runs.
# Usage: scripts/start-backend.sh [port]   (default 8082)
# Env: ZS_E2E_DATA_DIR (default: $TMPDIR/zebra-e2e), CARGO_TARGET_DIR (default: backend/target-e2e)
set -euo pipefail
PORT="${1:-8082}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DATA_DIR="${ZS_E2E_DATA_DIR:-${TMPDIR:-/tmp}/zebra-e2e}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/backend/target-e2e}"
BIN="$TARGET_DIR/debug/zebra-store"
STATE_DIR="$ROOT/e2e/.state"

if [[ ! -x "$BIN" || "${ZS_E2E_BUILD:-0}" == "1" ]]; then
  (cd "$ROOT/backend" && CARGO_TARGET_DIR="$TARGET_DIR" cargo build -p zs-server)
fi

rm -rf "$DATA_DIR" && mkdir -p "$DATA_DIR/uploads"
# A fresh database invalidates IDs and sessions persisted by the previous E2E run.
rm -f "$STATE_DIR/state.json" "$STATE_DIR/admin-session.json"
export ZS__SERVER__PORT="$PORT"
export ZS__DATABASE__URL="sqlite://$DATA_DIR/e2e.db?mode=rwc"
export ZS__APP__SECRET_KEY="e2e-app-secret-key-0123456789"
export ZS__JWT__SECRET="e2e-admin-jwt-secret-0123456789"
export ZS__USER_JWT__SECRET="e2e-user-jwt-secret-0123456789"
export ZS__BOOTSTRAP__DEFAULT_ADMIN_USERNAME="admin"
export ZS__BOOTSTRAP__DEFAULT_ADMIN_PASSWORD="Admin12345"
export ZS__UPLOAD__DIR="$DATA_DIR/uploads"
# The suite signs in many times from one IP; keep the login limiter out of the way.
export ZS__SECURITY__LOGIN_RATE_LIMIT__MAX_ATTEMPTS="${ZS_E2E_LOGIN_MAX_ATTEMPTS:-1000}"
exec "$BIN" --config /nonexistent.yml
