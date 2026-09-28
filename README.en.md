# Zebra Store

[简体中文](README.md) | [English](README.en.md)

Zebra Store is a self-hosted digital-goods commerce platform. Its backend uses **Rust (axum + SeaORM)**, while the storefront and admin panel use **Vue 3 + TSX**.

- Repository: [github.com/noxue/zebra-store](https://github.com/noxue/zebra-store)
- User guide: [zebra-store-docs.noxue.com](https://zebra-store-docs.noxue.com)
- Downloads: [GitHub Releases](https://github.com/noxue/zebra-store/releases)

## Recommended: single-binary release

Each Linux release is one executable containing the storefront, admin panel, API, and background jobs. You do not need Node.js, Rust, separate `dist` directories, or a clone of the repository.

On an x86_64 or aarch64 Linux server with systemd, run as root:

```bash
curl -fsSL https://raw.githubusercontent.com/noxue/zebra-store/main/scripts/install.sh | bash
```

The installer downloads only the release archive and `SHA256SUMS` for the current architecture. It verifies and installs the executable, generates a configuration file, and creates a systemd service. The initial admin account and random password are printed when installation finishes.

Reverse proxy the entire domain from Caddy, Nginx, or aaPanel Nginx to:

```text
http://127.0.0.1:8081
```

The web server only handles HTTPS and reverse proxying. It does not need separate rules for the storefront, admin panel, or API. See the [deployment guide](https://zebra-store-docs.noxue.com/deploy/) for complete instructions.

For Docker, follow the [Docker single-binary release guide](https://zebra-store-docs.noxue.com/deploy/docker). It downloads only the Dockerfile, Compose file, example configuration, and release binary.

## Automated releases

Pushing a Git tag beginning with `v` triggers GitHub Actions to build and publish:

- x86_64 Linux musl
- aarch64 Linux musl
- x86_64 Linux GNU
- aarch64 Linux GNU

Every release includes `SHA256SUMS`. Downloads are available from [GitHub Releases](https://github.com/noxue/zebra-store/releases).

## Components

| Component | Directory | Stack | Development port |
|---|---|---|---|
| Backend API | `backend/` | Rust 2024, axum 0.8, SeaORM 2.0 | 8081 |
| Storefront | `storefront/` | Vue 3 + TSX, Vite, Pinia, vue-i18n, Tailwind CSS v4 | 5185 |
| Admin panel | `admin/` | Vue 3 + TSX, Vite, Pinia, vue-i18n, Tailwind CSS v4 | 5186 |
| User guide | `handbook/` | VitePress | 5190 |

The API uses a consistent `{status_code, msg, data, pagination}` response envelope and string-formatted money values such as `"12.30"`. Supplier adapters integrate supported third-party sites.

## Development quick start

Requirements: Rust 1.90 or newer and Node.js 20 or newer.

```bash
# Backend
cd backend
cp config.example.yml config.yml      # set the three secrets and admin password
cargo run -p zs-server                # http://localhost:8081

# Storefront
cd ../storefront
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5185

# Admin panel
cd ../admin
npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5186
```

On first start, the server creates the tables, six built-in RBAC roles, and the configured super administrator.

## Database

SQLite is the default. MySQL 8+ and PostgreSQL 14+ are also supported. Switching databases only requires changing the URL:

```yaml
database:
  url: sqlite://data/zebra.db?mode=rwc
  # url: mysql://user:pass@127.0.0.1:3306/zebra
  # url: postgres://user:pass@127.0.0.1:5432/zebra
```

You can also use an environment variable such as `ZS__DATABASE__URL=postgres://…`. The application creates or extends tables on startup. The database itself must already exist, and the configured user needs DDL permissions.

## Configuration

[`backend/config.example.yml`](backend/config.example.yml) documents every setting. Settings can be overridden with `ZS__SECTION__KEY` environment variables, for example `ZS__SERVER__PORT=9000`.

`app.secret_key`, `jwt.secret`, and `user_jwt.secret` must all differ and contain at least 16 characters. The site name, logo, favicon, theme colors, and images can be changed in the admin panel. Redis is optional.

Common administrator commands:

```bash
zebra-store admin list-admins
zebra-store admin reset-password --username admin --password 'new-password'
zebra-store admin reset-2fa --username admin
```

## Building from source

Source builds are mainly intended for development. Build both web applications first; the Rust build script embeds them in the final `zebra-store` executable:

```bash
cd storefront && npm ci && npm run build
cd ../admin && npm ci && npm run build
cd ../backend && cargo build --release -p zs-server
```

The result is still a single `backend/target/release/zebra-store` executable. No separate frontend deployment is required.

## Tests

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd ../storefront && npm run typecheck && npm run lint && npm run test && npm run build
cd ../admin && npm run typecheck && npm run lint && npm run test && npm run build
```

## Documentation and architecture

The Chinese user guide source lives in [`handbook/`](handbook/). Engineering references are available in [`CLAUDE.md`](CLAUDE.md), [`docs/PLAN.md`](docs/PLAN.md), [`docs/TODO.md`](docs/TODO.md), [`docs/BACKEND_GUIDE.md`](docs/BACKEND_GUIDE.md), [`docs/DESIGN.md`](docs/DESIGN.md), and [`docs/reference/`](docs/reference/).

```text
zs-shared ← zs-domain ← zs-app ← zs-infra ← zs-api ← zs-server
                                  zs-migration ↗
```

Domain models and business rules live in `zs-domain`; use cases in `zs-app`; databases, gateways, mail, and the job queue in `zs-infra`; and HTTP endpoints in `zs-api`.
