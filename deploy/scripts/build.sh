#!/usr/bin/env bash
# Builds every zebra-lab image for one platform and loads it into the local Docker.
#
#   scripts/build.sh                          # native arch (rehearsal), tag :local
#   scripts/build.sh --platform linux/amd64   # server images, tag :amd64
#
# Steps:
#   1. Zebra backend: cargo-zigbuild -> static musl binary (fast cross-compile on a Mac;
#      needs `zig` + `cargo install cargo-zigbuild`). ZEBRA_BUILD=docker uses
#      docker/zebra-builder.Dockerfile instead (no host toolchain, slower).
#   2. storefront + admin SPAs (vite; admin with base /admin/)
#   3. third-party compatibility sources cloned at pinned commits
#      (ACG_FAKA_REPO / DUJIAO_REPO may point at local clones)
#   4. application, edge, tools, and compatibility-test images
# Build outputs stay in deploy/build/ (gitignored). deploy.sh builds zebra,web,edge,tools here
# and acg,dujiao natively on the server (`--only acg,dujiao`, sources rsynced).
set -euo pipefail
DEPLOY=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$DEPLOY/.." && pwd)
cd "$DEPLOY"

PLATFORM=""; TAG=""; ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --platform) PLATFORM=$2; shift 2 ;;
    --tag) TAG=$2; shift 2 ;;
    --only) ONLY=$2; shift 2 ;;   # comma list: zebra,web,edge,tools,acg,dujiao
    *) echo "unknown arg $1" >&2; exit 1 ;;
  esac
done
native="linux/$(docker version -f '{{.Server.Arch}}')"
PLATFORM=${PLATFORM:-$native}
ARCH=${PLATFORM#linux/}
[ -n "$TAG" ] || { [ "$PLATFORM" = "$native" ] && TAG=local || TAG=$ARCH; }
want() { if [ -z "$ONLY" ]; then [ "$1" != src ]; else [[ ",$ONLY," == *",$1,"* ]]; fi; }

ACG_FAKA_REPO=${ACG_FAKA_REPO:-https://github.com/lizhipay/acg-faka.git}
ACG_FAKA_REF=${ACG_FAKA_REF:-5120942d2c13ac900d614b09cfd6fbf672b62840}
DUJIAO_REPO=${DUJIAO_REPO:-https://github.com/dujiao-next/dujiao-next.git}
DUJIAO_REF=${DUJIAO_REF:-d2e44618de03068cbbe874a23a6d7c6a249a79ec}

step() { printf '\n==> %s\n' "$*"; }
mkdir -p build/bin build/web build/src build/seed

if want zebra && [ "${ZEBRA_BUILD:-zig}" = zig ]; then
  case "$ARCH" in
    amd64) TRIPLE=x86_64-unknown-linux-musl ;;
    arm64) TRIPLE=aarch64-unknown-linux-musl ;;
    *) echo "unsupported arch $ARCH" >&2; exit 1 ;;
  esac
  step "zebra backend ($TRIPLE, cargo-zigbuild)"
  command -v cargo-zigbuild >/dev/null || { echo "install: brew install zig && cargo install cargo-zigbuild" >&2; exit 1; }
  rustup target add "$TRIPLE" >/dev/null
  (cd "$REPO/backend" && CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-target-deploy} \
    cargo zigbuild --release --locked -p zs-server --target "$TRIPLE" -j "${BUILD_JOBS:-4}")
  cp "$REPO/backend/${CARGO_TARGET_DIR:-target-deploy}/$TRIPLE/release/zebra-store" "build/bin/zebra-store-$ARCH"
fi

if want web; then
  step "storefront + admin SPAs"
  for app in storefront admin; do
    [ -d "$REPO/$app/node_modules" ] || (cd "$REPO/$app" && npm ci)
  done
  (cd "$REPO/storefront" && npx vite build --outDir "$DEPLOY/build/web/storefront" --emptyOutDir)
  (cd "$REPO/admin" && npx vite build --base=/admin/ --outDir "$DEPLOY/build/web/admin" --emptyOutDir)
  step "seed data (backend/scripts/seed_store.py + covers)"
  rm -rf build/seed && mkdir -p build/seed
  cp "$REPO/backend/scripts/seed_store.py" build/seed/
  cp -R "$REPO/backend/scripts/covers" build/seed/covers
fi

fetch() { # dir repo ref — a source tree without .git (rsynced by deploy.sh) is used as is
  if [ -d "build/src/$1" ] && [ ! -d "build/src/$1/.git" ]; then return 0; fi
  if [ ! -d "build/src/$1/.git" ]; then git clone -q "$2" "build/src/$1"; fi
  git -C "build/src/$1" fetch -q origin "$3" 2>/dev/null || true
  git -C "build/src/$1" checkout -q "$3"
}

# ZEBRA_BUILDER=<name>: build in a dedicated BuildKit container (its cache is removed with
# `docker buildx rm <name>`, nothing lands in the shared default builder). deploy.sh uses
# this on the server; it is stopped again after the build.
builder_args=()
if [ -n "${ZEBRA_BUILDER:-}" ]; then
  docker buildx inspect "$ZEBRA_BUILDER" >/dev/null 2>&1 \
    || docker buildx create --name "$ZEBRA_BUILDER" --driver docker-container >/dev/null
  builder_args=(--builder "$ZEBRA_BUILDER")
  trap 'docker buildx stop "$ZEBRA_BUILDER" >/dev/null 2>&1 || true' EXIT
fi

img() { # name context [dockerfile]
  step "image zebra-lab/$1:$TAG ($PLATFORM)"
  docker buildx build ${builder_args[@]+"${builder_args[@]}"} --platform "$PLATFORM" --load \
    -t "zebra-lab/$1:$TAG" ${3:+-f "$3"} "$2"
}

if want zebra; then
  if [ "${ZEBRA_BUILD:-zig}" = docker ]; then
    img zebra "$REPO" "$DEPLOY/docker/zebra-builder.Dockerfile"
  else
    img zebra "$DEPLOY" "$DEPLOY/docker/zebra.Dockerfile"
  fi
fi
want edge && img edge "$DEPLOY" "$DEPLOY/docker/edge.Dockerfile"
want tools && img tools "$DEPLOY" "$DEPLOY/docker/tools.Dockerfile"
if want src; then   # only fetch the pinned third-party sources (deploy.sh ships them to the server)
  fetch acg-faka "$ACG_FAKA_REPO" "$ACG_FAKA_REF"; fetch dujiao-next "$DUJIAO_REPO" "$DUJIAO_REF"
fi
if want acg; then fetch acg-faka "$ACG_FAKA_REPO" "$ACG_FAKA_REF"; img acg-faka build/src/acg-faka; fi
if want dujiao; then fetch dujiao-next "$DUJIAO_REPO" "$DUJIAO_REF"; img dujiao-next build/src/dujiao-next; fi

step "done: zebra-lab/*:$TAG for $PLATFORM"
docker images --filter "reference=zebra-lab/*:$TAG" --format '  {{.Repository}}:{{.Tag}}  {{.Size}}'
