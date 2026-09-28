#!/usr/bin/env bash
# zebra-lab control script (runs on the machine that hosts the topology).
#
#   ./lab.sh up          render configs from .env, start everything, wait until healthy
#   ./lab.sh provision   configure all sites end-to-end (idempotent)
#   ./lab.sh verify      place real orders, print the PASS/FAIL table
#   ./lab.sh all         up + provision + verify
#   ./lab.sh creds       print URLs and credentials
#   ./lab.sh ps | logs [svc] | down | reset (down + delete this project's volumes and state)
#   ./lab.sh render      only render build/runtime/* (configs, Caddyfile, host snippets)
#   ./lab.sh docs        build ../handbook (VitePress) into docs/ (served as docs.<domain>)
#
# Mode and domain come from .env (LAB_MODE, LAB_DOMAIN); run ./init-secrets.sh first.
set -euo pipefail
cd "$(dirname "$0")"
HERE=$(pwd)

[ -f .env ] || { echo "no .env — run ./init-secrets.sh first" >&2; exit 1; }
set -a; . ./.env; set +a
: "${LAB_MODE:?}" "${LAB_DOMAIN:?}"

case "$LAB_MODE" in
  rehearsal) SITE_PREFIX="http://"; CADDY_GLOBAL="auto_https off"; LAB_SCHEME=http; ALLOW_PRIVATE_ADDRESSES=true ;;
  proxy)     SITE_PREFIX="http://"; CADDY_GLOBAL="auto_https off"; LAB_SCHEME=https; ALLOW_PRIVATE_ADDRESSES=false ;;
  caddy)     SITE_PREFIX="";        CADDY_GLOBAL="email ${ACME_EMAIL}"; LAB_SCHEME=https; ALLOW_PRIVATE_ADDRESSES=false ;;
  *) echo "LAB_MODE must be rehearsal|proxy|caddy" >&2; exit 1 ;;
esac
LAB_DIR=$HERE
export SITE_PREFIX CADDY_GLOBAL LAB_SCHEME ALLOW_PRIVATE_ADDRESSES LAB_DIR

compose() { docker compose -f compose.yml -f "compose.${LAB_MODE}.yml" "$@"; }

# ${VAR} substitution from the environment (perl is part of every Debian/macOS base)
tmpl() { perl -pe 's/\$\{(\w+)\}/exists $ENV{$1} ? $ENV{$1} : die "unset \$$1 in $ARGV\n"/ge' "$1"; }

render() {
  mkdir -p build/runtime build/state docs
  tmpl caddy/Caddyfile.tmpl > build/runtime/Caddyfile
  zebra_cfg() { # instance host reseller_enabled reseller_base title
    INSTANCE=$1 HOST=$2 RESELLER_ENABLED=$3 RESELLER_BASE=$4 SITE_TITLE=$5 \
    SITE_URL="${LAB_SCHEME}://$2" \
    APP_SECRET=$6 JWT_SECRET=$7 USER_JWT_SECRET=$8 ADMIN_PASSWORD=$9 \
      tmpl config/zebra.yml.tmpl > "build/runtime/$1.yml"
  }
  zebra_cfg store "store.${LAB_DOMAIN}" true "${LAB_DOMAIN}" Zebra-Store \
    "$STORE_APP_SECRET" "$STORE_JWT_SECRET" "$STORE_USER_JWT_SECRET" "$STORE_ADMIN_PASSWORD"
  zebra_cfg zs2 "zs2.${LAB_DOMAIN}" false "" Zebra-ZS2 \
    "$ZS2_APP_SECRET" "$ZS2_JWT_SECRET" "$ZS2_USER_JWT_SECRET" "$ZS2_ADMIN_PASSWORD"
  tmpl config/dujiao.yml.tmpl > build/runtime/dujiao.yml
  tmpl caddy/zebra.caddy.tmpl > build/runtime/zebra.caddy
  tmpl caddy/host-snippet.nginx.conf.tmpl > build/runtime/nginx-zebra.conf
  chmod 600 build/runtime/*.yml
  # The Zebra image runs as its non-root `zebra` user (uid assigned by `adduser -S`), so the
  # 0600 configs must belong to that uid on a Linux host (Docker Desktop/colima ignore this).
  if [ "$(id -u)" = 0 ]; then
    local zuid
    zuid=$(docker run --rm --entrypoint id "zebra-lab/zebra:${IMAGE_TAG}" -u 2>/dev/null || true)
    [ -n "$zuid" ] && chown "$zuid" build/runtime/store.yml build/runtime/zs2.yml
  fi
  echo "rendered build/runtime/ (mode=$LAB_MODE domain=$LAB_DOMAIN scheme=$LAB_SCHEME)"
}

wait_healthy() {
  local deadline=$((SECONDS + ${1:-300})) svc url
  for svc in store zs2; do
    until compose exec -T "$svc" wget -q -O /dev/null http://127.0.0.1:8081/api/v1/public/config 2>/dev/null; do
      [ $SECONDS -lt $deadline ] || { echo "timeout waiting for $svc" >&2; compose logs --tail 50 "$svc"; exit 1; }
      sleep 2
    done
  done
  until compose exec -T dujiao wget -q -O /dev/null http://127.0.0.1:8080/health 2>/dev/null; do
    [ $SECONDS -lt $deadline ] || { echo "timeout waiting for dujiao" >&2; compose logs --tail 50 dujiao; exit 1; }
    sleep 2
  done
  until compose exec -T acg php -r 'exit(trim((string)@file_get_contents("http://127.0.0.1/healthz")) === "pong" ? 0 : 1);' 2>/dev/null; do
    [ $SECONDS -lt $deadline ] || { echo "timeout waiting for acg" >&2; compose logs --tail 50 acg; exit 1; }
    sleep 2
  done
  # the edge routes by Host: ask it for the store's config the way every client does
  until compose exec -T edge wget -q -O /dev/null --header "Host: store.${LAB_DOMAIN}" http://127.0.0.1/api/v1/public/config 2>/dev/null; do
    [ $SECONDS -lt $deadline ] || { echo "timeout waiting for edge" >&2; compose logs --tail 50 edge; exit 1; }
    sleep 2
  done
  echo "all services healthy"
}

tools() { compose --profile tools run --rm -e LAB_SCHEME -e LAB_MODE -e VERIFY_ONLY tools "$@"; }

# acg-faka ships with an image captcha on the admin login and no HTTP way around it on a
# fresh install. This is the ONE non-HTTP provisioning step: switch that single setting off
# in its database (the provisioner then turns the other captchas off through the admin API).
acg_unlock() {
  printf "INSERT INTO acg_config(\`key\`,\`value\`) VALUES ('admin_login_verification','0') ON DUPLICATE KEY UPDATE \`value\`='0';" \
    | compose exec -T acg sh -c 'mariadb -h127.0.0.1 -uacg -p"$(cat /data/secrets/mysql_app)" acg_faka && rm -f /data/runtime/config'
}

cmd=${1:-help}; shift || true
case "$cmd" in
  render) render ;;
  docs) scripts/build-docs.sh ;;
  up) render; compose up -d --remove-orphans; wait_healthy 600 ;;
  provision)
    tools python provision/provision.py wait acg-install
    acg_unlock
    tools python provision/provision.py "$@" ;;
  verify) tools python provision/verify.py "$@" ;;
  all) "$0" up && "$0" provision && "$0" verify ;;
  creds) cat build/state/credentials.txt 2>/dev/null || echo "not provisioned yet" ;;
  ps) compose ps ;;
  logs) compose logs --tail "${TAIL:-100}" "$@" ;;
  down) compose --profile tools down ;;
  reset|destroy)
    # only this compose project (zebra-lab): its containers, network and volumes
    compose --profile tools down -v --remove-orphans
    rm -rf build/state build/runtime
    echo "all lab volumes and provisioning state deleted" ;;
  compose) compose "$@" ;;
  *) sed -n '2,13p' "$0" ;;
esac
