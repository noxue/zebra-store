#!/usr/bin/env bash
# Builds the VitePress handbook (repo dir handbook/) into deploy/docs/, served as
# docs.<LAB_DOMAIN> (static files: by the host Caddy in proxy mode, by the edge otherwise).
# No-op when handbook/ does not exist.
set -euo pipefail
DEPLOY=$(cd "$(dirname "$0")/.." && pwd)
HB="$DEPLOY/../handbook"
mkdir -p "$DEPLOY/docs"
if [ ! -f "$HB/package.json" ]; then
  echo "handbook/ not found — docs.<domain> keeps its current content"
  exit 0
fi
(cd "$HB" && npm ci && npm run docs:build)
rsync -a --delete "$HB/.vitepress/dist/" "$DEPLOY/docs/"
echo "handbook built into $DEPLOY/docs ($(find "$DEPLOY/docs" -type f | wc -l | tr -d ' ') files)"
