#!/usr/bin/env bash
# Boots fresh instances of the compatibility backend and the Rust
# backend on empty databases, then runs contract_diff.py against both.
#
# Usage:
#   scripts/contract-diff/run.sh                # build (if needed), boot both, diff, stop both
#   KEEP=1 scripts/contract-diff/run.sh         # leave both servers running afterwards
#
# Environment:
#   GO_SRC      compatibility checkout containing ./dujiao-server + config.yml
#               (default: the session scratchpad copy)
#   GO_PORT     port for the fresh Go instance     (default 8092)
#   RS_PORT     port for the fresh Rust instance   (default 8091)
#   REDIS_DB    Redis db index for Go cache; queue uses REDIS_DB+1 (default 4)
#   CARGO_TARGET_DIR  (default target-contract); SKIP_BUILD=1 reuses the existing binary
# Needs a Redis on 127.0.0.1:6379 (the Go server cannot create orders without it).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
BACKEND="$(cd "$HERE/../.." && pwd)"
GO_SRC="${GO_SRC:-/private/tmp/claude-501/-Volumes-KINGSTON-codes-rust-zebra-store/24fd5e70-eecd-446c-8cc3-d3c8ef96339a/scratchpad/dujiao-next}"
GO_PORT="${GO_PORT:-8092}"
RS_PORT="${RS_PORT:-8091}"
REDIS_DB="${REDIS_DB:-4}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-contract}"
for port in "$GO_PORT" "$RS_PORT"; do
  if lsof -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "port $port is already in use; stop that server or pick GO_PORT/RS_PORT" >&2; exit 2
  fi
done
WORK="$(mktemp -d "${TMPDIR:-/tmp}/contract-diff.XXXXXX")"
echo "work dir: $WORK"

[ "${SKIP_BUILD:-0}" = "1" ] || (cd "$BACKEND" && cargo build -q -p zs-server)
case "$CARGO_TARGET_DIR" in /*) BIN="$CARGO_TARGET_DIR/debug/zebra-store" ;; *) BIN="$BACKEND/$CARGO_TARGET_DIR/debug/zebra-store" ;; esac

# --- fresh Go instance -------------------------------------------------------
mkdir -p "$WORK/go/db"
cp "$GO_SRC/dujiao-server" "$WORK/go/"
python3 - "$GO_SRC/config.yml" "$WORK/go/config.yml" "$GO_PORT" "$REDIS_DB" <<'EOF'
import re, sys
src, dst, port, db = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
s = open(src).read()
s = re.sub(r'(server:\n(?:  .*\n)*?  port: )\d+', r'\g<1>' + port, s)
s = re.sub(r'(redis:\n(?:  .*\n)*?  db: )\d+', r'\g<1>' + str(db), s)
s = re.sub(r'(queue:\n(?:  .*\n)*?  db: )\d+', r'\g<1>' + str(db + 1), s)
open(dst, 'w').write(s)
EOF
# Flush the Redis dbs used by this Go instance so cached config/rate limits do not leak between runs.
python3 - "$REDIS_DB" <<'EOF'
import socket, sys
for db in (int(sys.argv[1]), int(sys.argv[1]) + 1):
    s = socket.create_connection(("127.0.0.1", 6379), timeout=3)
    s.sendall(f"SELECT {db}\r\nFLUSHDB\r\n".encode()); s.recv(100); s.close()
EOF
(cd "$WORK/go"; ./dujiao-server > server.log 2>&1 & echo $! > pid)

# --- fresh Rust instance -----------------------------------------------------
mkdir -p "$WORK/rs"
(
  cd "$WORK/rs"
  ZS__SERVER__PORT="$RS_PORT" \
  ZS__DATABASE__URL="sqlite://$WORK/rs/c.db?mode=rwc" \
  ZS__APP__SECRET_KEY=contract-app-secret-0123456789 \
  ZS__JWT__SECRET=contract-admin-jwt-0123456789 \
  ZS__USER_JWT__SECRET=contract-user-jwt-0123456789 \
  ZS__BOOTSTRAP__DEFAULT_ADMIN_USERNAME=admin \
  ZS__BOOTSTRAP__DEFAULT_ADMIN_PASSWORD=Admin12345 \
  "$BIN" --config /nonexistent.yml > server.log 2>&1 & echo $! > pid
)

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    kill "$(cat "$WORK/go/pid")" "$(cat "$WORK/rs/pid")" 2>/dev/null || true
  fi
}
trap cleanup EXIT

for url in "http://127.0.0.1:$GO_PORT/health" "http://127.0.0.1:$RS_PORT/health"; do
  for _ in $(seq 1 60); do curl -sf "$url" >/dev/null && break; sleep 0.5; done
done

python3 "$HERE/contract_diff.py" \
  --orig "http://127.0.0.1:$GO_PORT" --ours "http://127.0.0.1:$RS_PORT" \
  --out "$HERE/out" "$@"
