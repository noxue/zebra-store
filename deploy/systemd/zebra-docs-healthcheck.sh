#!/bin/sh
set -eu

HOST=${DOCS_HOST:-zebra-store-docs.noxue.com}
URL="https://$HOST"
INDEX=$(mktemp)
trap 'rm -f "$INDEX"' EXIT INT TERM

check_site() {
  curl -fsS --max-time 10 --resolve "$HOST:443:127.0.0.1" "$URL/" -o "$INDEX" || return 1
  test "$(wc -c < "$INDEX")" -gt 1000 || return 1
  asset=$(sed -n 's#.*src="\(/assets/app\.[^"]*\.js\)".*#\1#p' "$INDEX" | head -n 1)
  test -n "$asset" || return 1
  curl -fsS --max-time 10 --resolve "$HOST:443:127.0.0.1" "$URL$asset" >/dev/null
}

if check_site; then
  exit 0
fi

logger -t zebra-docs-healthcheck "documentation origin failed; restarting Caddy"
systemctl restart caddy
sleep 2
check_site
