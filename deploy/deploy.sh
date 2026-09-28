#!/usr/bin/env bash
# One-command deploy of zebra-lab to a Linux server with Docker + Compose v2.
#
#   ./deploy.sh root@107.174.142.210                 # proxy mode behind the host Caddy, *.dot2.com
#   ./deploy.sh root@host --mode caddy --domain example.com --acme-email me@example.com
#   ./deploy.sh root@host --uninstall                # remove our stack, import line and install dir
#   ./deploy.sh root@host --docs                     # only rebuild ../handbook and publish docs.<domain>
#
# Options:
#   --mode proxy|caddy       proxy (default): the host's existing proxy owns :80/:443 and forwards
#                            our hostnames to 127.0.0.1:<proxy-port>; caddy: our edge owns :80/:443
#   --domain D               base domain, sites are <name>.D          (default dot2.com)
#   --dir DIR                install directory on the server           (default /opt/zebra-lab)
#   --host-caddyfile PATH    proxy mode: host Caddyfile to extend with ONE `import DIR/caddy/zebra.caddy`
#                            line (default: auto = the --config of the systemd caddy unit; none = skip,
#                            e.g. for nginx — see caddy/host-snippet.nginx.conf.tmpl)
#   --proxy-port N           127.0.0.1 port of our edge in proxy mode  (default 18180)
#   --acme-email E           caddy mode: ACME account e-mail
#   --skip-build             reuse the zebra-lab/*:amd64 images already built locally
#   --no-provision | --no-verify
#
# Guarantees (the server is shared): read-only preflight first and abort on any doubt; only
# `zebra-lab`-prefixed containers / volumes / networks / images are created or removed; every
# published port is bound to 127.0.0.1 (proxy mode); no daemon config, no prune, host Caddy is
# only ever *reloaded* after `caddy validate` with automatic rollback. Secrets are generated on
# the server (init-secrets.sh) and never copied back. Credentials: ssh <host> DIR/lab.sh creds
set -euo pipefail
cd "$(dirname "$0")"

TARGET=${1:?usage: ./deploy.sh user@host [options]}; shift
MODE=proxy; DOMAIN=dot2.com; DIR=/opt/zebra-lab; HOST_CADDYFILE=auto
PROXY_PORT=18180; ACME_EMAIL=""; BUILD=1; PROVISION=1; VERIFY=1; UNINSTALL=0; DOCS_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --mode) MODE=$2; shift 2 ;;
    --domain) DOMAIN=$2; shift 2 ;;
    --dir) DIR=$2; shift 2 ;;
    --host-caddyfile) HOST_CADDYFILE=$2; shift 2 ;;
    --proxy-port) PROXY_PORT=$2; shift 2 ;;
    --acme-email) ACME_EMAIL=$2; shift 2 ;;
    --skip-build) BUILD=0; shift ;;
    --no-provision) PROVISION=0; VERIFY=0; shift ;;
    --no-verify) VERIFY=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --docs) DOCS_ONLY=1; shift ;;
    *) echo "unknown option $1" >&2; exit 1 ;;
  esac
done
case "$MODE" in proxy|caddy) ;; *) echo "--mode must be proxy or caddy" >&2; exit 1 ;; esac
[ "$MODE" = caddy ] && [ -z "$ACME_EMAIL" ] && { echo "--mode caddy needs --acme-email" >&2; exit 1; }
case "$DIR" in /*/*|/opt/*) ;; *) echo "--dir must be an absolute, non-root path" >&2; exit 1 ;; esac

IMAGES="zebra edge tools acg-faka dujiao-next"
LOCAL_IMAGES="zebra edge tools"
SUBNET=$(grep -E '^LAB_SUBNET=' .env.example | cut -d= -f2)
step() { printf '\n==> %s\n' "$*"; }
remote() { ssh -o BatchMode=yes "$TARGET" "$@"; }

if [ "$HOST_CADDYFILE" = auto ]; then
  if [ "$MODE" = proxy ]; then
    HOST_CADDYFILE=$(remote "systemctl show -p ExecStart caddy 2>/dev/null | grep -o -- '--config [^ ;]*' | head -1 | cut -d' ' -f2" || true)
    HOST_CADDYFILE=${HOST_CADDYFILE:-none}
  else HOST_CADDYFILE=none; fi
fi

if [ $UNINSTALL = 1 ]; then
  step "uninstalling zebra-lab from $TARGET:$DIR"
  remote "test -f $DIR/lab.sh" || { echo "no zebra-lab install at $DIR" >&2; exit 1; }
  [ "$HOST_CADDYFILE" != none ] && remote "cd $DIR && scripts/host-caddy.sh uninstall '$HOST_CADDYFILE' '$DIR'"
  remote "cd $DIR && ./lab.sh destroy"
  for name in $IMAGES; do remote "docker image rm zebra-lab/$name:amd64 >/dev/null 2>&1 || true"; done
  remote "docker buildx rm zebra-lab-builder >/dev/null 2>&1 || true"   # our BuildKit container + cache
  remote "rm -rf -- '$DIR'"
  step "removed (host Caddyfile backups *.bak.zebra-* were kept next to it)"
  exit 0
fi

sync_docs() {  # static handbook, readable by the host proxy's user
  remote "mkdir -p '$DIR/docs'"
  mkdir -p docs
  rsync -rlptz --delete docs/ "$TARGET:$DIR/docs/"   # macOS openrsync has no --chmod
  remote "find '$DIR/docs' -type d -exec chmod 755 {} + && find '$DIR/docs' -type f -exec chmod 644 {} +"
}
if [ $DOCS_ONLY = 1 ]; then
  remote "test -f '$DIR/lab.sh'" || { echo "no zebra-lab install at $DIR — deploy first" >&2; exit 1; }
  step "handbook -> $TARGET:$DIR/docs"
  scripts/build-docs.sh
  sync_docs
  step "done: https://docs.$DOMAIN/ (served from $DIR/docs)"
  exit 0
fi

step "preflight on $TARGET (read-only)"
remote "bash -s -- '$MODE' '$PROXY_PORT' '$SUBNET' '$HOST_CADDYFILE' '$DIR'" < scripts/preflight.sh \
  || { echo "preflight failed — nothing was changed" >&2; exit 1; }

if [ $BUILD = 1 ]; then
  step "building Zebra images locally (linux/amd64: cross-compiled binary, no emulation)"
  scripts/build.sh --platform linux/amd64 --tag amd64 --only zebra,web,edge,tools,src
  # docs are optional here (publish later with --docs); a broken handbook must not block the stack
  scripts/build-docs.sh || echo "    WARNING: handbook build failed — skipped (run ./deploy.sh $TARGET --docs later)"
fi

step "uploading changed images (docker save | ssh docker load)"
for name in $LOCAL_IMAGES; do
  ref="zebra-lab/$name:amd64"
  local_id=$(docker image inspect -f '{{.Id}}' "$ref")
  remote_id=$(remote "docker image inspect -f '{{.Id}}' $ref 2>/dev/null" || true)
  if [ "$local_id" = "$remote_id" ]; then echo "    $ref unchanged"; continue; fi
  echo "    $ref"
  docker save "$ref" | gzip -1 | remote 'gunzip | docker load' | sed 's/^/      /'
done
remote "docker image inspect redis:7-alpine >/dev/null 2>&1 || docker pull -q redis:7-alpine"

step "syncing files to $TARGET:$DIR"
remote "mkdir -p '$DIR'"
rsync -az --delete \
  --exclude '/.env' --exclude '/build/src' --exclude '/build/bin' --exclude '/build/web' \
  --exclude '/build/state' --exclude '/build/runtime' --exclude '/caddy/zebra.caddy' --exclude '/caddy/zebra.caddy.prev' --exclude '/docs' \
  --exclude '*.log' --exclude '__pycache__' \
  ./ "$TARGET:$DIR/"
sync_docs
# pinned third-party compatibility sources are built natively on the server
# (official Dockerfiles; avoids emulating a 1.2 GB PHP image build on the laptop)
for src in acg-faka dujiao-next; do
  remote "mkdir -p '$DIR/build/src/$src'"
  rsync -az --delete --exclude '.git' --exclude 'node_modules' --exclude 'dist' \
    "build/src/$src/" "$TARGET:$DIR/build/src/$src/"
done

if [ $BUILD = 1 ]; then
  step "building acg-faka + dujiao-next on the server (native amd64, BuildKit cache kept)"
  remote "cd '$DIR' && ZEBRA_BUILDER=zebra-lab-builder scripts/build.sh --platform linux/amd64 --tag amd64 --only acg,dujiao"
fi

step "configuring (secrets are generated on the server)"
set_args="--set LAB_MODE=$MODE --set LAB_DOMAIN=$DOMAIN --set IMAGE_TAG=amd64 --set PROXY_PORT=$PROXY_PORT"
[ -n "$ACME_EMAIL" ] && set_args="$set_args --set ACME_EMAIL=$ACME_EMAIL"
remote "cd '$DIR' && ./init-secrets.sh $set_args && ./lab.sh render"

step "starting the stack"
remote "cd '$DIR' && ./lab.sh up"

if [ "$MODE" = proxy ] && [ "$HOST_CADDYFILE" != none ]; then
  step "host Caddy: import $DIR/caddy/zebra.caddy from $HOST_CADDYFILE"
  remote "cd '$DIR' && scripts/host-caddy.sh install '$HOST_CADDYFILE' '$DIR'"
fi

[ $PROVISION = 1 ] && { step "provisioning"; remote "cd '$DIR' && ./lab.sh provision"; }
[ $VERIFY = 1 ] && { step "verifying"; remote "cd '$DIR' && ./lab.sh verify"; }
step "done — credentials: ssh $TARGET $DIR/lab.sh creds"
