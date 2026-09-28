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

Use `TARGET=aarch64` on an ARM64 server. Pin a release with `RELEASE=v0.1.2`; the default is the latest release.
The SQLite database and uploads use the `zebra-store_store_data` and
`zebra-store_store_uploads` volumes. Put Caddy or Nginx in front of `127.0.0.1:18180`.
