# Zebra Store

[简体中文](README.md) | [English](README.en.md)

Zebra Store is a self-hosted digital-goods commerce platform. Its backend uses **Rust (axum + SeaORM)**, while the storefront and admin panel use **Vue 3 + TSX**.

- Repository: [github.com/noxue/zebra-store](https://github.com/noxue/zebra-store)
- User guide: [zebra-store-docs.noxue.com](https://zebra-store-docs.noxue.com)
- Downloads: [GitHub Releases](https://github.com/noxue/zebra-store/releases)
- Huifu SDK: [huifu-pay](https://github.com/noxue/huifu-pay) ([`huifu-pay` on crates.io](https://crates.io/crates/huifu-pay))

Zebra Store supports Alipay and WeChat H5/PC collection through Huifu, including payment creation, RSA-verified notifications, active queries, original-route refunds, and trade-bill downloads. By default, desktop checkout shows a QR code for the complete hosted payment page while mobile invokes the payment flow; full-page redirect is also selectable. See the [Huifu setup guide](https://zebra-store-docs.noxue.com/payment/huifu).

Guest checkout can leave both email and order password empty. The storefront persists a UUID browser identity in `localStorage`, so orders remain available after a refresh or reopening the site. If the buyer enters an email and password, those credentials are also saved and restored for later lookup.

## Screenshots

### Storefront

|  |  |
|---|---|
| <a href="handbook/public/screenshots/storefront/home.png"><img src="handbook/public/screenshots/storefront/home.png" width="440" alt="Storefront home"></a><br>Storefront home | <a href="handbook/public/screenshots/storefront/product-detail.png"><img src="handbook/public/screenshots/storefront/product-detail.png" width="440" alt="Product details"></a><br>Product details |

<details>
<summary>View all storefront screenshots (22)</summary>

|  |  |
|---|---|
| <a href="handbook/public/screenshots/storefront/affiliate.png"><img src="handbook/public/screenshots/storefront/affiliate.png" width="440" alt="Affiliate center"></a><br>Affiliate center | <a href="handbook/public/screenshots/storefront/announcement.png"><img src="handbook/public/screenshots/storefront/announcement.png" width="440" alt="Announcement"></a><br>Announcement |
| <a href="handbook/public/screenshots/storefront/cart.png"><img src="handbook/public/screenshots/storefront/cart.png" width="440" alt="Shopping cart"></a><br>Shopping cart | <a href="handbook/public/screenshots/storefront/checkout.png"><img src="handbook/public/screenshots/storefront/checkout.png" width="440" alt="Checkout"></a><br>Checkout |
| <a href="handbook/public/screenshots/storefront/gift-card.png"><img src="handbook/public/screenshots/storefront/gift-card.png" width="440" alt="Gift card"></a><br>Gift card | <a href="handbook/public/screenshots/storefront/guest-orders.png"><img src="handbook/public/screenshots/storefront/guest-orders.png" width="440" alt="Guest order lookup"></a><br>Guest order lookup |
| <a href="handbook/public/screenshots/storefront/home-dark.png"><img src="handbook/public/screenshots/storefront/home-dark.png" width="440" alt="Dark home page"></a><br>Dark home page | <a href="handbook/public/screenshots/storefront/home-mobile.png"><img src="handbook/public/screenshots/storefront/home-mobile.png" width="440" alt="Mobile home page"></a><br>Mobile home page |
| <a href="handbook/public/screenshots/storefront/home.png"><img src="handbook/public/screenshots/storefront/home.png" width="440" alt="Storefront home"></a><br>Storefront home | <a href="handbook/public/screenshots/storefront/login.png"><img src="handbook/public/screenshots/storefront/login.png" width="440" alt="User login"></a><br>User login |
| <a href="handbook/public/screenshots/storefront/me-api-compat.png"><img src="handbook/public/screenshots/storefront/me-api-compat.png" width="440" alt="Compatible API settings"></a><br>Compatible API settings | <a href="handbook/public/screenshots/storefront/me-api.png"><img src="handbook/public/screenshots/storefront/me-api.png" width="440" alt="API settings"></a><br>API settings |
| <a href="handbook/public/screenshots/storefront/me-orders.png"><img src="handbook/public/screenshots/storefront/me-orders.png" width="440" alt="My orders"></a><br>My orders | <a href="handbook/public/screenshots/storefront/member-level.png"><img src="handbook/public/screenshots/storefront/member-level.png" width="440" alt="Member levels"></a><br>Member levels |
| <a href="handbook/public/screenshots/storefront/order-detail.png"><img src="handbook/public/screenshots/storefront/order-detail.png" width="440" alt="Order details"></a><br>Order details | <a href="handbook/public/screenshots/storefront/order-refunds.png"><img src="handbook/public/screenshots/storefront/order-refunds.png" width="440" alt="Order refunds"></a><br>Order refunds |
| <a href="handbook/public/screenshots/storefront/payment.png"><img src="handbook/public/screenshots/storefront/payment.png" width="440" alt="Payment"></a><br>Payment | <a href="handbook/public/screenshots/storefront/product-detail.png"><img src="handbook/public/screenshots/storefront/product-detail.png" width="440" alt="Product details"></a><br>Product details |
| <a href="handbook/public/screenshots/storefront/products.png"><img src="handbook/public/screenshots/storefront/products.png" width="440" alt="Products"></a><br>Products | <a href="handbook/public/screenshots/storefront/register.png"><img src="handbook/public/screenshots/storefront/register.png" width="440" alt="User registration"></a><br>User registration |
| <a href="handbook/public/screenshots/storefront/security-2fa.png"><img src="handbook/public/screenshots/storefront/security-2fa.png" width="440" alt="Two-factor authentication"></a><br>Two-factor authentication | <a href="handbook/public/screenshots/storefront/wallet.png"><img src="handbook/public/screenshots/storefront/wallet.png" width="440" alt="User wallet"></a><br>User wallet |

</details>

### Admin panel

|  |  |
|---|---|
| <a href="handbook/public/screenshots/admin/dashboard.png"><img src="handbook/public/screenshots/admin/dashboard.png" width="440" alt="Dashboard"></a><br>Dashboard | <a href="handbook/public/screenshots/admin/products.png"><img src="handbook/public/screenshots/admin/products.png" width="440" alt="Products"></a><br>Products |

<details>
<summary>View all admin-panel screenshots (47)</summary>

|  |  |
|---|---|
| <a href="handbook/public/screenshots/admin/affiliate-settings.png"><img src="handbook/public/screenshots/admin/affiliate-settings.png" width="440" alt="Affiliate settings"></a><br>Affiliate settings | <a href="handbook/public/screenshots/admin/affiliate-withdraws.png"><img src="handbook/public/screenshots/admin/affiliate-withdraws.png" width="440" alt="Affiliate withdrawals"></a><br>Affiliate withdrawals |
| <a href="handbook/public/screenshots/admin/api-credentials.png"><img src="handbook/public/screenshots/admin/api-credentials.png" width="440" alt="API credentials"></a><br>API credentials | <a href="handbook/public/screenshots/admin/authz-audit.png"><img src="handbook/public/screenshots/admin/authz-audit.png" width="440" alt="Authorization audit"></a><br>Authorization audit |
| <a href="handbook/public/screenshots/admin/authz.png"><img src="handbook/public/screenshots/admin/authz.png" width="440" alt="Authorization"></a><br>Authorization | <a href="handbook/public/screenshots/admin/banners.png"><img src="handbook/public/screenshots/admin/banners.png" width="440" alt="Banners"></a><br>Banners |
| <a href="handbook/public/screenshots/admin/card-secret-import.png"><img src="handbook/public/screenshots/admin/card-secret-import.png" width="440" alt="Card-secret import"></a><br>Card-secret import | <a href="handbook/public/screenshots/admin/card-secrets.png"><img src="handbook/public/screenshots/admin/card-secrets.png" width="440" alt="Card secrets"></a><br>Card secrets |
| <a href="handbook/public/screenshots/admin/categories.png"><img src="handbook/public/screenshots/admin/categories.png" width="440" alt="Categories"></a><br>Categories | <a href="handbook/public/screenshots/admin/compliance.png"><img src="handbook/public/screenshots/admin/compliance.png" width="440" alt="Compliance"></a><br>Compliance |
| <a href="handbook/public/screenshots/admin/coupons.png"><img src="handbook/public/screenshots/admin/coupons.png" width="440" alt="Coupons"></a><br>Coupons | <a href="handbook/public/screenshots/admin/dashboard.png"><img src="handbook/public/screenshots/admin/dashboard.png" width="440" alt="Dashboard"></a><br>Dashboard |
| <a href="handbook/public/screenshots/admin/gift-cards.png"><img src="handbook/public/screenshots/admin/gift-cards.png" width="440" alt="Gift cards"></a><br>Gift cards | <a href="handbook/public/screenshots/admin/login.png"><img src="handbook/public/screenshots/admin/login.png" width="440" alt="Admin login"></a><br>Admin login |
| <a href="handbook/public/screenshots/admin/media.png"><img src="handbook/public/screenshots/admin/media.png" width="440" alt="Media library"></a><br>Media library | <a href="handbook/public/screenshots/admin/member-levels.png"><img src="handbook/public/screenshots/admin/member-levels.png" width="440" alt="Member levels"></a><br>Member levels |
| <a href="handbook/public/screenshots/admin/notifications.png"><img src="handbook/public/screenshots/admin/notifications.png" width="440" alt="Notifications"></a><br>Notifications | <a href="handbook/public/screenshots/admin/order-detail.png"><img src="handbook/public/screenshots/admin/order-detail.png" width="440" alt="Order details"></a><br>Order details |
| <a href="handbook/public/screenshots/admin/order-refunds.png"><img src="handbook/public/screenshots/admin/order-refunds.png" width="440" alt="Order refunds"></a><br>Order refunds | <a href="handbook/public/screenshots/admin/orders.png"><img src="handbook/public/screenshots/admin/orders.png" width="440" alt="Orders"></a><br>Orders |
| <a href="handbook/public/screenshots/admin/payment-channel-edit.png"><img src="handbook/public/screenshots/admin/payment-channel-edit.png" width="440" alt="Edit payment channel"></a><br>Edit payment channel | <a href="handbook/public/screenshots/admin/payment-channels.png"><img src="handbook/public/screenshots/admin/payment-channels.png" width="440" alt="Payment channels"></a><br>Payment channels |
| <a href="handbook/public/screenshots/admin/payments.png"><img src="handbook/public/screenshots/admin/payments.png" width="440" alt="Payments"></a><br>Payments | <a href="handbook/public/screenshots/admin/posts.png"><img src="handbook/public/screenshots/admin/posts.png" width="440" alt="Posts"></a><br>Posts |
| <a href="handbook/public/screenshots/admin/procurement-orders.png"><img src="handbook/public/screenshots/admin/procurement-orders.png" width="440" alt="Procurement orders"></a><br>Procurement orders | <a href="handbook/public/screenshots/admin/product-edit.png"><img src="handbook/public/screenshots/admin/product-edit.png" width="440" alt="Edit product"></a><br>Edit product |
| <a href="handbook/public/screenshots/admin/product-mappings.png"><img src="handbook/public/screenshots/admin/product-mappings.png" width="440" alt="Product mappings"></a><br>Product mappings | <a href="handbook/public/screenshots/admin/products.png"><img src="handbook/public/screenshots/admin/products.png" width="440" alt="Products"></a><br>Products |
| <a href="handbook/public/screenshots/admin/promotions.png"><img src="handbook/public/screenshots/admin/promotions.png" width="440" alt="Promotions"></a><br>Promotions | <a href="handbook/public/screenshots/admin/reconciliation.png"><img src="handbook/public/screenshots/admin/reconciliation.png" width="440" alt="Reconciliation"></a><br>Reconciliation |
| <a href="handbook/public/screenshots/admin/reseller-operations.png"><img src="handbook/public/screenshots/admin/reseller-operations.png" width="440" alt="Reseller operations"></a><br>Reseller operations | <a href="handbook/public/screenshots/admin/reseller-profiles.png"><img src="handbook/public/screenshots/admin/reseller-profiles.png" width="440" alt="Reseller profiles"></a><br>Reseller profiles |
| <a href="handbook/public/screenshots/admin/reseller-site-configs.png"><img src="handbook/public/screenshots/admin/reseller-site-configs.png" width="440" alt="Reseller site settings"></a><br>Reseller site settings | <a href="handbook/public/screenshots/admin/reseller-withdraws.png"><img src="handbook/public/screenshots/admin/reseller-withdraws.png" width="440" alt="Reseller withdrawals"></a><br>Reseller withdrawals |
| <a href="handbook/public/screenshots/admin/risk-control.png"><img src="handbook/public/screenshots/admin/risk-control.png" width="440" alt="Risk control"></a><br>Risk control | <a href="handbook/public/screenshots/admin/security.png"><img src="handbook/public/screenshots/admin/security.png" width="440" alt="Security settings"></a><br>Security settings |
| <a href="handbook/public/screenshots/admin/settings-basic.png"><img src="handbook/public/screenshots/admin/settings-basic.png" width="440" alt="General settings"></a><br>General settings | <a href="handbook/public/screenshots/admin/settings-theme.png"><img src="handbook/public/screenshots/admin/settings-theme.png" width="440" alt="Theme settings"></a><br>Theme settings |
| <a href="handbook/public/screenshots/admin/site-connection-edit.png"><img src="handbook/public/screenshots/admin/site-connection-edit.png" width="440" alt="Edit site connection"></a><br>Edit site connection | <a href="handbook/public/screenshots/admin/site-connections.png"><img src="handbook/public/screenshots/admin/site-connections.png" width="440" alt="Site connections"></a><br>Site connections |
| <a href="handbook/public/screenshots/admin/telegram-bot.png"><img src="handbook/public/screenshots/admin/telegram-bot.png" width="440" alt="Telegram bot"></a><br>Telegram bot | <a href="handbook/public/screenshots/admin/user-detail.png"><img src="handbook/public/screenshots/admin/user-detail.png" width="440" alt="User details"></a><br>User details |
| <a href="handbook/public/screenshots/admin/user-login-logs.png"><img src="handbook/public/screenshots/admin/user-login-logs.png" width="440" alt="Login logs"></a><br>Login logs | <a href="handbook/public/screenshots/admin/users.png"><img src="handbook/public/screenshots/admin/users.png" width="440" alt="Users"></a><br>Users |
| <a href="handbook/public/screenshots/admin/wallet-config.png"><img src="handbook/public/screenshots/admin/wallet-config.png" width="440" alt="Wallet settings"></a><br>Wallet settings | <a href="handbook/public/screenshots/admin/wallet-recharges.png"><img src="handbook/public/screenshots/admin/wallet-recharges.png" width="440" alt="Wallet recharges"></a><br>Wallet recharges |
| <a href="handbook/public/screenshots/admin/wholesale.png"><img src="handbook/public/screenshots/admin/wholesale.png" width="440" alt="Wholesale settings"></a><br>Wholesale settings |  |

</details>

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
