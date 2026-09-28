#!/bin/sh
set -eu

REPOSITORY=${REPOSITORY:-noxue/zebra-store}
INSTALL_DIR=${INSTALL_DIR:-/opt/zebra-store}
BIN_PATH=${BIN_PATH:-/usr/local/bin/zebra-store}
SERVICE_USER=${SERVICE_USER:-zebra-store}

if [ "$(id -u)" -ne 0 ]; then
  echo "Please run this installer as root." >&2
  exit 1
fi
for command in curl tar sha256sum systemctl; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "Missing required command: $command" >&2
    exit 1
  }
done

case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) echo "Unsupported CPU architecture: $(uname -m)" >&2; exit 1 ;;
esac

asset="zebra-store-linux-${arch}-musl.tar.gz"
base="https://github.com/${REPOSITORY}/releases/latest/download"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading $asset"
curl -fL --retry 3 "$base/$asset" -o "$tmp/$asset"
curl -fL --retry 3 "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
(cd "$tmp" && grep "  $asset\$" SHA256SUMS | sha256sum -c -)
tar -xzf "$tmp/$asset" -C "$tmp"

if ! id "$SERVICE_USER" >/dev/null 2>&1; then
  useradd --system --home "$INSTALL_DIR" --shell /usr/sbin/nologin "$SERVICE_USER"
fi
install -d -m 750 -o "$SERVICE_USER" -g "$SERVICE_USER" "$INSTALL_DIR"
systemctl stop zebra-store.service 2>/dev/null || true
install -m 755 "$tmp/zebra-store" "$BIN_PATH"

if [ ! -f "$INSTALL_DIR/config.yml" ]; then
  su -s /bin/sh "$SERVICE_USER" -c "cd '$INSTALL_DIR' && '$BIN_PATH' init"
fi
chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR"

cat > /etc/systemd/system/zebra-store.service <<EOF
[Unit]
Description=Zebra Store
After=network-online.target
Wants=network-online.target

[Service]
User=$SERVICE_USER
Group=$SERVICE_USER
WorkingDirectory=$INSTALL_DIR
ExecStart=$BIN_PATH --config $INSTALL_DIR/config.yml serve
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
PrivateTmp=true
ReadWritePaths=$INSTALL_DIR
Environment=NO_COLOR=1

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now zebra-store.service

for _ in 1 2 3 4 5 6 7 8 9 10; do
  if curl -fsS http://127.0.0.1:8081/health >/dev/null 2>&1; then
    echo "Zebra Store is running at http://127.0.0.1:8081"
    echo "Data and configuration: $INSTALL_DIR"
    echo "Next: point Caddy or Nginx at 127.0.0.1:8081."
    exit 0
  fi
  sleep 2
done

systemctl status zebra-store.service --no-pager || true
journalctl -u zebra-store.service -n 50 --no-pager || true
echo "Zebra Store did not become healthy." >&2
exit 1
