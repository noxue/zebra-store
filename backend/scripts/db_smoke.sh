#!/usr/bin/env bash
# Multi-database smoke test: runs the real server against SQLite, PostgreSQL and MySQL
# (only the database URL changes) and exercises schema sync, bootstrap, auth,
# decimal money, JSON columns, unique indexes and restart idempotency.
#
# Usage: scripts/db_smoke.sh [binary]
#   PG_URL / MYSQL_URL override the defaults (docker containers zebra-pg / zebra-mysql).
set -euo pipefail

BIN="${1:-target-main/debug/zebra-store}"
PORT=18091
PG_URL="${PG_URL:-postgres://zebra:zebra@127.0.0.1:15432/zebra}"
MYSQL_URL="${MYSQL_URL:-mysql://root:zebra@127.0.0.1:13306/zebra}"
SQLITE_URL="sqlite://$(mktemp -d)/smoke.db?mode=rwc"
BASE="http://127.0.0.1:${PORT}/api/v1"
FAILED=0

export ZS__APP__SECRET_KEY="smoke-app-secret-0123456789abcdef"
export ZS__JWT__SECRET="smoke-admin-jwt-secret-012345678"
export ZS__USER_JWT__SECRET="smoke-user-jwt-secret-0123456789"
export ZS__BOOTSTRAP__DEFAULT_ADMIN_USERNAME="admin"
export ZS__BOOTSTRAP__DEFAULT_ADMIN_PASSWORD="Admin12345"
export ZS__SERVER__PORT="$PORT"
export ZS__SERVER__HOST="127.0.0.1"
export ZS__LOG__LEVEL="warn"

start() {
  ZS__DATABASE__URL="$1" "$BIN" --config /nonexistent.yml >"/tmp/zs-smoke-$2.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 60); do
    curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1 && return 0
    sleep 0.5
  done
  echo "  ✗ server did not start (see /tmp/zs-smoke-$2.log)"; tail -20 "/tmp/zs-smoke-$2.log"; return 1
}

stop() { kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }

check() { # name, actual, expected
  if [[ "$2" == "$3" ]]; then echo "  ✓ $1"; else echo "  ✗ $1: expected [$3] got [$2]"; FAILED=1; fi
}

json() { python3 -c "import sys,json;d=json.load(sys.stdin);print(eval(sys.argv[1]))" "$1"; }

run_suite() {
  local name="$1" url="$2" slug="smoke-$RANDOM$RANDOM"
  echo "== $name"
  start "$url" "$name" || { FAILED=1; return; }
  local token
  token=$(curl -s -X POST "$BASE/admin/login" -H 'content-type: application/json' \
    -d '{"username":"admin","password":"Admin12345"}' | json 'd["data"]["token"]')
  check "admin login" "$([[ -n "$token" ]] && echo ok)" "ok"
  local auth=(-H "Authorization: Bearer $token" -H 'content-type: application/json')

  local cat
  cat=$(curl -s -X POST "$BASE/admin/categories" "${auth[@]}" \
    -d "{\"slug\":\"$slug\",\"name\":{\"zh-CN\":\"测试分类\",\"en-US\":\"Smoke\"}}")
  check "create category" "$(echo "$cat" | json 'd["status_code"]')" "0"
  check "json column roundtrip" "$(echo "$cat" | json 'd["data"]["name"]["zh-CN"]')" "测试分类"
  local cid; cid=$(echo "$cat" | json 'd["data"]["id"]')

  local dup
  dup=$(curl -s -X POST "$BASE/admin/categories" "${auth[@]}" -d "{\"slug\":\"$slug\",\"name\":{\"zh-CN\":\"x\"}}")
  check "unique slug rejected" "$(echo "$dup" | json 'd["status_code"]')" "400"

  local prod
  prod=$(curl -s -X POST "$BASE/admin/products" "${auth[@]}" -d "{\"title\":{\"zh-CN\":\"冒烟商品\"},\"slug\":\"$slug-p\",\"category_id\":$cid,\"price_amount\":\"12.30\",\"purchase_type\":\"guest\",\"fulfillment_type\":\"manual\",\"manual_stock_total\":5,\"is_active\":true}")
  check "create product" "$(echo "$prod" | json 'd["status_code"]')" "0"
  local pid; pid=$(echo "$prod" | json 'd["data"]["id"]' 2>/dev/null || echo 0)
  local detail
  detail=$(curl -s "$BASE/admin/products/$pid" "${auth[@]}")
  check "decimal money as string" "$(echo "$detail" | json 'd["data"]["price_amount"]' 2>/dev/null)" "12.30"

  local pub
  pub=$(curl -s "$BASE/public/products?page=1&page_size=50")
  check "public list paginated" "$(echo "$pub" | json '"pagination" in d')" "True"

  stop
  # Restart: schema sync + bootstrap must be idempotent.
  start "$url" "$name-restart" || { FAILED=1; return; }
  local again
  again=$(curl -s -X POST "$BASE/admin/login" -H 'content-type: application/json' \
    -d '{"username":"admin","password":"Admin12345"}' | json 'd["status_code"]')
  check "restart idempotent" "$again" "0"
  stop
}

run_suite sqlite "$SQLITE_URL"
run_suite postgres "$PG_URL"
run_suite mysql "$MYSQL_URL"

if [[ "$FAILED" -ne 0 ]]; then echo "SMOKE FAILED"; exit 1; fi
echo "ALL DATABASES OK"
