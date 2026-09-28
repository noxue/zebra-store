#!/bin/sh
set -eu

DEPLOY_HOST=${1:-${DOCS_DEPLOY_HOST:-}}
DEPLOY_ROOT=${DOCS_DEPLOY_ROOT:-/opt/zebra-store}
DOCS_URL=${DOCS_URL:-https://zebra-store-docs.noxue.com}

if [ -z "$DEPLOY_HOST" ]; then
  echo "Usage: $0 user@server" >&2
  echo "Or set DOCS_DEPLOY_HOST, DOCS_DEPLOY_ROOT and DOCS_URL." >&2
  exit 1
fi

for command in npm git ssh rsync curl; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "Missing required command: $command" >&2
    exit 1
  }
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
REVISION=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo dev)
RELEASE_ID=$(date -u +%Y%m%dT%H%M%SZ)-$REVISION
RELEASE_DIR="$DEPLOY_ROOT/docs-releases/$RELEASE_ID"

echo "Building handbook"
(cd "$REPO_ROOT/handbook" && npm run docs:build)

echo "Preparing $RELEASE_DIR"
ssh "$DEPLOY_HOST" sh -s -- "$DEPLOY_ROOT" "$RELEASE_ID" <<'REMOTE'
set -eu
root=$1
release_id=$2
release_dir="$root/docs-releases/$release_id"
current="$root/docs-current"
legacy="$root/docs"

install -d "$release_dir/assets"

# Keep existing hashed assets so cached HTML from an earlier release continues
# to load after the atomic switch.
source_dir=
if [ -L "$current" ]; then
  source_dir=$(readlink -f "$current")
elif [ -d "$legacy" ]; then
  source_dir=$legacy
fi
if [ -n "$source_dir" ] && [ -d "$source_dir/assets" ]; then
  cp -a "$source_dir/assets/." "$release_dir/assets/"
fi
REMOTE

echo "Uploading complete release"
rsync -az "$REPO_ROOT/handbook/.vitepress/dist/" "$DEPLOY_HOST:$RELEASE_DIR/"

echo "Validating and switching atomically"
ssh "$DEPLOY_HOST" sh -s -- "$DEPLOY_ROOT" "$RELEASE_ID" <<'REMOTE'
set -eu
root=$1
release_id=$2
release_dir="$root/docs-releases/$release_id"
current="$root/docs-current"
next_link="$root/.docs-current-$release_id"

test -s "$release_dir/index.html"
test -s "$release_dir/404.html"
test -d "$release_dir/assets"
test "$(find "$release_dir/assets" -type f | wc -l)" -gt 0

rm -f "$next_link"
ln -s "$release_dir" "$next_link"
mv -Tf "$next_link" "$current"

# Keep three complete releases for quick rollback. Hashed assets are also
# carried into the active release, so deleting an old release is safe.
set -- "$root"/docs-releases/*
if [ -e "$1" ]; then
  ls -1dt "$root"/docs-releases/* | tail -n +4 | xargs -r rm -rf
fi
REMOTE

echo "Checking public site"
curl -fsS --retry 3 "$DOCS_URL/" >/dev/null
curl -fsS --retry 3 "$DOCS_URL/deploy/" >/dev/null
echo "Handbook deployed: $RELEASE_ID"
