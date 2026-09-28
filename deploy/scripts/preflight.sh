#!/usr/bin/env bash
# Read-only server checks run by deploy.sh BEFORE anything is uploaded or changed
# (`ssh host bash -s -- <args> < scripts/preflight.sh`). Exits non-zero on any doubt.
#   preflight.sh <mode> <proxy_port> <lab_subnet> <host_caddyfile|none> <install dir>
set -uo pipefail
MODE=$1; PORT=$2; SUBNET=$3; CF=$4; DIR=$5
fail=0
ok() { echo "    ok    $*"; }
bad() { echo "    FAIL  $*"; fail=1; }

[ "$(uname -m)" = x86_64 ] && ok "arch x86_64" || bad "arch $(uname -m) (images are built for amd64)"
docker version >/dev/null 2>&1 && ok "docker $(docker version -f '{{.Server.Version}}')" || bad "docker not usable"
docker compose version >/dev/null 2>&1 && ok "compose $(docker compose version --short)" || bad "docker compose v2 missing"
docker buildx version >/dev/null 2>&1 && ok "buildx (acg-faka / dujiao-next are built here)" || bad "docker buildx plugin missing"
command -v rsync >/dev/null && ok "rsync" || bad "rsync missing on the server (apt-get install rsync)"
command -v perl >/dev/null && ok "perl" || bad "perl missing"
command -v ss >/dev/null && ok "ss (port checks)" || bad "ss missing (iproute2) — cannot check ports"

root=$(docker info -f '{{.DockerRootDir}}' 2>/dev/null || echo /var/lib/docker)
for path in / "$root"; do
  free=$(df -Pk "$path" | awk 'NR==2 {print int($4/1048576)}')
  [ "${free:-0}" -ge 10 ] && ok "disk $path ${free} GB free" || bad "disk $path only ${free} GB free (need >= 10)"
done
mem=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
[ "${mem:-0}" -ge 2560 ] && ok "memory ${mem} MiB available" || bad "only ${mem} MiB memory available (lab caps total ~2.3 GiB)"

ours() { docker ps --filter "name=^zebra-lab-" --format '{{.Ports}}' 2>/dev/null | grep -q "$1"; }
listening() { ss -Hltn "sport = :$1" 2>/dev/null | grep -q .; }
if [ "$MODE" = proxy ]; then
  if listening "$PORT"; then
    ours "127.0.0.1:$PORT->" && ok "port 127.0.0.1:$PORT held by our own edge" || bad "port $PORT already in use"
  else ok "port 127.0.0.1:$PORT free"; fi
else
  for p in 80 443; do
    if listening $p; then ours "0.0.0.0:$p->" && ok "port $p held by our edge" || bad "port $p in use (use --mode proxy)"
    else ok "port $p free"; fi
  done
fi

# our subnet must not come near any other docker network or host route (conservative:
# anything else in the same /16 counts as a clash)
net=${SUBNET%/*}; p16="${net%.*.*}."
clash=$(docker network ls -q | xargs -r docker network inspect -f '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}' \
  | grep -v '^zebra-lab_' | grep -F " $p16" || true)
[ -z "$clash" ] && ok "subnet $SUBNET: no other docker network in ${p16}0.0/16" || bad "subnet $SUBNET near: $clash (set LAB_SUBNET)"
if ip route 2>/dev/null | grep -v 'br-' | grep -qF " $p16" || ip route 2>/dev/null | grep -v 'br-' | grep -q "^$p16"; then
  bad "a host route uses ${p16}0.0/16 (set LAB_SUBNET)"; else ok "no host route in ${p16}0.0/16"; fi

if [ "$MODE" = proxy ] && [ "$CF" != none ]; then
  if [ -f "$CF" ] && command -v caddy >/dev/null; then
    systemctl is-active --quiet caddy && ok "systemd caddy active" || bad "systemd caddy not active"
    caddy validate --config "$CF" --adapter caddyfile >/tmp/zebra-lab-preflight-caddy.log 2>&1 \
      && ok "current host Caddyfile validates ($CF)" || bad "current $CF does not validate: see /tmp/zebra-lab-preflight-caddy.log"
  else bad "host Caddyfile $CF or caddy binary not found (pass --host-caddyfile none to skip)"; fi
fi
[ -d "$DIR" ] && [ ! -f "$DIR/lab.sh" ] && [ -n "$(ls -A "$DIR")" ] && bad "$DIR exists and is not a zebra-lab install"
exit $fail
