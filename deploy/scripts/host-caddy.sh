#!/usr/bin/env bash
# Hooks zebra-lab into an existing host Caddy (behind-proxy mode) — and back out.
#
#   scripts/host-caddy.sh install   <host Caddyfile> <install dir>
#   scripts/host-caddy.sh uninstall <host Caddyfile> <install dir>
#
# install: copies build/runtime/zebra.caddy to <dir>/caddy/zebra.caddy and makes sure the
#   host Caddyfile contains exactly one line `import <dir>/caddy/zebra.caddy` (appended once,
#   after a timestamped backup). The full config is validated before `systemctl reload caddy`
#   (reload only, never restart); on any failure the previous Caddyfile and snippet are
#   restored and re-validated.
# uninstall: removes that line (restores the pristine backup if nothing else changed since),
#   validates, reloads.
set -euo pipefail
ACTION=${1:?install|uninstall}; CF=${2:?host Caddyfile}; DIR=${3:?install dir}
SNIP="$DIR/caddy/zebra.caddy"
LINE="import $SNIP"
MARK="# zebra-lab: remove with $DIR/lab.sh uninstall"
TS=$(date +%Y%m%d%H%M%S)

die() { echo "host-caddy: $*" >&2; exit 1; }
validate() { caddy validate --config "$CF" --adapter caddyfile >/tmp/zebra-lab-caddy-validate.log 2>&1; }

command -v caddy >/dev/null || die "caddy binary not found"
[ -f "$CF" ] || die "$CF not found"
systemctl is-active --quiet caddy || die "systemd unit caddy is not active — refusing to touch it"

case "$ACTION" in
install)
  validate || die "the CURRENT host config does not validate (see /tmp/zebra-lab-caddy-validate.log) — refusing"
  mkdir -p "$DIR/caddy"
  [ -f "$SNIP" ] && cp -p "$SNIP" "$SNIP.prev"
  cp "$DIR/build/runtime/zebra.caddy" "$SNIP"
  added=0
  if ! grep -qxF "$LINE" "$CF"; then
    cp -p "$CF" "$CF.bak.zebra-$TS"
    printf '\n%s\n%s\n' "$MARK" "$LINE" >> "$CF"
    added=1
    echo "    appended import line (backup: $CF.bak.zebra-$TS)"
  fi
  rollback() {
    echo "host-caddy: rolling back" >&2
    [ $added = 1 ] && cp -p "$CF.bak.zebra-$TS" "$CF"
    if [ -f "$SNIP.prev" ]; then cp -p "$SNIP.prev" "$SNIP"; elif [ $added = 1 ]; then rm -f "$SNIP"; fi
    validate && systemctl reload caddy || echo "host-caddy: WARNING re-validation after rollback failed" >&2
  }
  if ! validate; then cat /tmp/zebra-lab-caddy-validate.log >&2; rollback; die "new config does not validate"; fi
  systemctl reload caddy || { rollback; die "systemctl reload caddy failed"; }
  echo "    host caddy reloaded with $SNIP"
  ;;
uninstall)
  if grep -qxF "$LINE" "$CF"; then
    cp -p "$CF" "$CF.pre-uninstall.zebra-$TS"
    # the newest pristine backup is used only if the Caddyfile is still exactly backup+our lines
    latest=$(ls -1t "$CF".bak.zebra-* 2>/dev/null | head -1 || true)
    if [ -n "$latest" ] && [ "$(cat "$latest"; printf '\n%s\n%s\n' "$MARK" "$LINE")" = "$(cat "$CF")" ]; then
      cp -p "$latest" "$CF"
      echo "    restored $latest"
    else
      grep -vxF -e "$LINE" -e "$MARK" "$CF.pre-uninstall.zebra-$TS" > "$CF.tmp.zebra" && cat "$CF.tmp.zebra" > "$CF" && rm -f "$CF.tmp.zebra"
      echo "    removed the import line"
    fi
    if ! validate; then
      cp -p "$CF.pre-uninstall.zebra-$TS" "$CF"
      die "config without zebra-lab does not validate — restored, nothing changed"
    fi
    systemctl reload caddy
    echo "    host caddy reloaded without zebra-lab"
  else
    echo "    no zebra-lab import in $CF"
  fi
  ;;
*) die "unknown action $ACTION" ;;
esac
