# Zebra Store

A self-hosted digital-goods commerce platform with a **Rust (axum + sea-orm)** backend and two independent
**Vue 3 + TSX** frontends in an anime (二次元) style.

| Project | Path | Stack | Dev port |
|---|---|---|---|
| Backend API | `backend/` | Rust 2024, axum 0.8, sea-orm 2.0 | 8081 |
| Storefront (shop) | `storefront/` | Vue 3 + TSX, Vite, Pinia, vue-i18n, Tailwind v4 | 5185 |
| Admin panel | `admin/` | Vue 3 + TSX, Vite, Pinia, vue-i18n, Tailwind v4 | 5186 |

The API uses stable paths, JSON fields, a `{status_code, msg, data, pagination}` envelope, consistent error
keys, and money values such as `"12.30"`. Supplier adapters provide compatibility with supported third-party sites.

Documentation: [`CLAUDE.md`](CLAUDE.md) (engineering rules), [`docs/PLAN.md`](docs/PLAN.md),
[`docs/TODO.md`](docs/TODO.md), [`docs/BACKEND_GUIDE.md`](docs/BACKEND_GUIDE.md),
[`docs/DESIGN.md`](docs/DESIGN.md), and the reference material in [`docs/reference/`](docs/reference/)
(API specifications, screenshots, and regression lessons).

## 文档

The user handbook (Chinese, written for beginners: deployment via 宝塔 / Docker / Nginx / Caddy, storefront and
admin usage, payment gateways, site integrations, reseller subsites, FAQ) lives in [`handbook/`](handbook/) as a
VitePress site and is published at **https://docs.dot2.com**.

```bash
cd handbook && npm install
npm run docs:dev        # http://localhost:5190 (live reload)
npm run docs:build      # static site in handbook/.vitepress/dist (fails on dead links)
```

The handbook includes 89 application screenshots captured from the tested dot2.com lab and a fresh local
instance. The six screenshots that require a separate 宝塔 panel are tracked in
[`handbook/SCREENSHOTS.md`](handbook/SCREENSHOTS.md). The manual/end-to-end checklist for the lab is
[`docs/TEST_FLOWS.md`](docs/TEST_FLOWS.md).

## Quick start (development)

Requirements: Rust ≥ 1.90, Node ≥ 20.

```bash
# 1. Backend
cd backend
cp config.example.yml config.yml      # then set the three secrets and the admin password
cargo run -p zs-server                # http://localhost:8081 — creates data/zebra.db (SQLite)

# 2. Storefront
cd storefront && npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5185

# 3. Admin panel
cd admin && npm install
VITE_API_TARGET=http://localhost:8081 npm run dev     # http://localhost:5186  (login: bootstrap admin)
```

On first start the server creates every table, the six built-in RBAC roles and the super administrator
from `bootstrap.default_admin_username` / `default_admin_password`.

## Choosing the database

Only the URL changes — no code or rebuild:

```yaml
database:
  url: sqlite://data/zebra.db?mode=rwc                  # default
  # url: mysql://user:pass@127.0.0.1:3306/zebra          # MySQL 8+
  # url: postgres://user:pass@127.0.0.1:5432/zebra       # PostgreSQL 14+
```

or via environment: `ZS__DATABASE__URL=postgres://…`. Tables are created/extended automatically on start
(entity-first schema sync) and dialect-specific data migrations (partial unique indexes, MySQL
`DATETIME(6)` columns) run afterwards. The database must exist; the user needs DDL rights.

## Configuration

`backend/config.example.yml` documents every key. Any key can be overridden with
`ZS__SECTION__KEY` environment variables (double underscores), e.g. `ZS__SERVER__PORT=9000`.

The server refuses to start unless `app.secret_key`, `jwt.secret` and `user_jwt.secret` are set to
three **different** values of at least 16 characters. Site name, logo, favicon, theme colours,
background and mascot image are configured in the admin panel (系统设置 → 站点设置 / 主题外观).

Redis is optional (not required): cache, rate limiting and the job queue run in-process / in the database.

## Build & deploy

```bash
cd backend && cargo build --release -p zs-server        # target/release/zebra-store
cd storefront && npm ci && npm run build                 # storefront/dist
cd admin && npm ci && npm run build                      # admin/dist
```

Run `zebra-store --config /etc/zebra/config.yml` behind a reverse proxy that serves the two `dist`
folders and forwards `/api`, `/uploads`, `/sitemap.xml`, `/robots.txt` (plus `/shared/` and `/plugin/open-api/`
when the acg-faka / mcy provider-compat protocols are enabled) to the backend. List the proxy in
`server.trusted_proxies` so client IPs are resolved correctly.

Operator commands: `zebra-store admin list-admins | reset-password --username X | reset-2fa --username X`.

## Tests

```bash
cd backend
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                   # in-memory SQLite

# The same integration tests against PostgreSQL / MySQL (a fresh database per test):
ZS_TEST_DATABASE_URL=postgres://zebra:zebra@127.0.0.1:15432/zebra cargo test -p zs-api
ZS_TEST_DATABASE_URL=mysql://root:zebra@127.0.0.1:13306/zebra cargo test -p zs-api
scripts/drop_test_dbs.sh                                 # remove the zs_t_* test databases
scripts/db_smoke.sh                                      # real server on SQLite, PostgreSQL, MySQL

cd storefront && npm run typecheck && npm run lint && npm run test && npm run build
cd admin && npm run typecheck && npm run lint && npm run test && npm run build
```

Test databases for the commands above:

```bash
docker run -d --name zebra-pg -e POSTGRES_USER=zebra -e POSTGRES_PASSWORD=zebra -e POSTGRES_DB=zebra -p 15432:5432 postgres:16-alpine
docker run -d --name zebra-mysql -e MYSQL_ROOT_PASSWORD=zebra -e MYSQL_DATABASE=zebra -p 13306:3306 mysql:8.4
```

## Architecture (backend)

```
zs-shared ← zs-domain ← zs-app ← zs-infra ← zs-api ← zs-server
                                  zs-migration ↗
```

Domain models, business rules and ports live in `zs-domain`; use cases in `zs-app`; database, gateways,
mail and the job queue in `zs-infra`; HTTP in `zs-api`. The crate graph enforces the layering. Each business
area (identity, catalog, content, marketing, order, payment, wallet, affiliate, reseller, integration,
notify, dashboard) is a folder in every layer. See [`docs/BACKEND_GUIDE.md`](docs/BACKEND_GUIDE.md).
