# Docker deployment from GitHub Release

This deployment runs one container and downloads the embedded-web Linux binary from
[GitHub Releases](https://github.com/noxue/zebra-store/releases) while building the small runtime image.

```bash
cp ../../backend/config.example.yml config.yml
# Set the secrets and initial admin password in config.yml.
docker compose build --no-cache
docker compose up -d
curl http://127.0.0.1:18180/health
```

The initial admin credentials come from `bootstrap.default_admin_username` and
`bootstrap.default_admin_password` in `config.yml`. After the password is changed, it cannot be
read back because the database stores only its hash. Reset a forgotten password with:

```bash
NEW_PASSWORD="Zs9-$(openssl rand -hex 10)"
docker compose exec store zebra-store --config /app/config.yml \
  admin reset-password --username admin --password "$NEW_PASSWORD"
printf 'New admin password: %s\n' "$NEW_PASSWORD"
```

Use `TARGET=aarch64` on an ARM64 server. Pin a release with `RELEASE=v0.1.4`; the default is the latest release.
The SQLite database and uploads use the `zebra-store_store_data` and
`zebra-store_store_uploads` volumes. Put Caddy or Nginx in front of `127.0.0.1:18180`.
