I've read the whole backend and the report is below. Every route, table and setting key comes from the source; the one-line flow descriptions are my summaries of the code. Repo root is `/private/tmp/claude-501/-Volumes-KINGSTON-codes-rust-zebra-store/24fd5e70-eecd-446c-8cc3-d3c8ef96339a/scratchpad/dujiao-next`, and paths below are relative to it.

**Behaviours you must reproduce exactly (they are easy to get wrong in a rewrite):**
- **Business errors return HTTP 200.** The error goes in `status_code`/`msg` in the JSON body.
- **Money is a JSON string with two decimals** (`"12.30"`), stored as `decimal(20,2)`.
- **Ordering does not work without Redis.** Order creation fails if the job queue (asynq on Redis) is off, because it schedules the payment-timeout cancel job.
- **Admin permissions are checked against the route pattern with `/api/v1` removed**, e.g. `/admin/orders/:id`, not the real URL. Super admins skip the check.
- **Guest orders authenticate with a special header:** `Authorization: Guest <base64url(email + "\n" + password)>`.
- **Encryption is AES-256-GCM** with key = SHA-256 of the secret, output hex(nonce‖ciphertext). It protects TOTP secrets, channel-client secrets, bot tokens and site-connection secrets.
- **Timestamps are UTC**, and deletes are soft (`deleted_at`) almost everywhere.

---

## 1. Database tables

Tables are created in `internal/bootstrap/database/migrations/registry.go` (`AutoMigrate`). Reseller tables are created separately in `internal/modules/reseller/infrastructure/gormstore/store.go` (`Migrate`), with partial unique indexes `WHERE deleted_at IS NULL`. Casbin uses its own table `casbin_rule`.

Most tables have `id`, `created_at`, `updated_at` and a nullable `deleted_at`. `json` below means a JSON column; `ml-json` means a multi-language JSON map (`{"zh-CN":..,"zh-TW":..,"en-US":..}`).

The order-risk data fix-up runs before `resellerstore.Migrate`, so it will be in the ordered list below.

**After-migration steps, in this order:**
1. unique index on user OAuth identities
2. backfill `orders.risk_ip` for pending orders
3. reseller `Migrate`
4. cart SKU unique index
5. product SKU migration
6. manual stock migration
7. category parent migration
8. rename payment provider to `bepusdt`, and its channel config
9. payment fee policy
10. refund payment-fee columns
11. order-item original price
12. cart foreign keys
13. procurement-order foreign key
14. drop `products.price_currency`

### Identity (`internal/modules/identity/*/domain`)
- **admins**
  - username (unique), password_hash (bcrypt), token_version, token_invalid_before, is_super, last_login_at
  - TOTP: totp_secret (encrypted), totp_enabled_at, totp_pending_secret, totp_pending_expires_at
  - recovery_codes: JSON list of `{hash (bcrypt), used_at}`
- **users**
  - email (unique), password_hash, password_setup_required, display_name, locale (default `zh-CN`), status (`active`/`disabled`)
  - member_level_id, total_recharged, total_spent, admin_note
  - token_version, token_invalid_before, the same TOTP and recovery-code fields as admins
  - email_verified_at, last_login_at
- **user_oauth_identities**: user_id, provider (`telegram`/`google`), provider_user_id (unique together with provider), username, avatar_url, auth_at
- **email_verify_codes**
  - email, user_id?, code, expires_at, verified_at, attempt_count, sent_at
  - purpose: `register`, `reset`, `telegram_bind`, `change_email_old`, `change_email_new`

### Catalog
- **categories**: parent_id (0 = top level), slug (unique), name (ml-json), icon, sort_order, is_active
- **products**
  - Basics: category_id → categories, slug (unique), seo_meta, and ml-json title / description / content / instructions
  - Prices: price_amount, cost_price_amount, wholesale_prices (JSON price tiers)
  - Media and labels: images, tags (JSON lists)
  - purchase_type: `guest` or `member`
  - min/max purchase quantity; stock_display_mode: `exact`, `status`, `range` or `hidden`
  - fulfillment_type: `auto`, `manual` or `upstream`; manual_form_schema (JSON)
  - Manual stock: manual_stock_total (-1 = unlimited), manual_stock_locked, manual_stock_sold
  - payment_channel_ids (JSON text; empty = any channel), is_affiliate_enabled, is_mapped, is_active, sort_order
  - Has many product_skus
- **product_skus**: product_id + sku_code (unique together), spec_values (JSON), price_amount, cost_price_amount, the same three manual-stock fields, is_active, sort_order
- **product_mappings**: connection_id, local_product_id (unique), upstream_product_id, upstream_fulfillment_type, upstream_status, is_active, last_synced_at
- **sku_mappings**: product_mapping_id, local_sku_id, upstream_sku_id, upstream_price, upstream_stock, upstream_is_active, stock_synced_at

### Cart, orders and fulfillment
- **cart_items**: user_id + product_id + sku_id (unique together), quantity, fulfillment_type
- **orders**
  - order_no (unique, format `DJ` + `yyyyMMddHHmmss` + 6 random digits), parent_id (child orders), user_id (0 for guests)
  - Guest fields: guest_email, guest_password, guest_locale
  - status, currency
  - Amounts: original, discount (coupon), member_discount, promotion_discount, wholesale_discount, total, wallet_paid, online_paid, refunded
  - Snapshots: member_level_id, coupon_id, promotion_id, affiliate_profile_id, affiliate_code, reseller_id, reseller_domain, reseller_profit_amount
  - Risk: client_ip, risk_ip (IPv6 grouped by /64; composite index)
  - Times: expires_at, paid_at, canceled_at
- **order_items**
  - order_id, product_id, sku_id, title (ml-json), sku_snapshot, tags
  - Prices: original_unit_price, unit_price, cost_price, quantity, original_total_price, total_price
  - Discount shares: coupon, member, promotion, wholesale; promotion_id
  - fulfillment_type, manual_form_schema_snapshot, manual_form_submission, instructions
- **order_refund_records**: user_id, guest_email, order_id, type (`manual`/`wallet`), amount, payment_fee_refunded, payment_fee_refunded_amount, currency, remark
- **order_risk_lock_keys**: key_hash (primary key), created_at — used as a pending-order lock
- **fulfillments**: order_id (unique), type, status (`pending`/`delivered`), payload (text, one line per delivered item), logistics (JSON), delivered_by, delivered_at

**Order status values:**
- `pending_payment`
- `paid`
- `fulfilling`
- `partially_delivered`
- `delivered`
- `completed`
- `canceled`
- `refunded`
- `partially_refunded`

**Allowed status transitions** (`allowedTransitions` in `order/application/order_service.go`):

| From | Allowed next states |
|---|---|
| `pending_payment` | `paid`, `canceled` |
| `paid` | `fulfilling`, `partially_delivered`, `delivered`, `partially_refunded`, `refunded` |
| `fulfilling` | `partially_delivered`, `delivered`, `partially_refunded`, `refunded` |
| `partially_delivered` | `delivered`, `completed`, `partially_refunded`, `refunded` |
| `delivered` | `completed`, `partially_refunded`, `refunded` |
| `completed` | `partially_refunded`, `refunded` |
| `partially_refunded` | `refunded` |

A parent order's status is recalculated from its children (`CalcParentStatus`).

### Card secrets and gift cards
- **card_secrets**: product_id, sku_id, batch_id, secret (text), status (`available`/`reserved`/`used`), order_id, reserved_at, used_at
- **card_secret_batches**: product_id, sku_id, batch_no (unique), source (`manual`/`csv`), total_count, note, created_by
- **gift_cards**: batch_id, name, code (unique; upper-cased on redeem), amount, currency, status (`active`/`redeemed`/`disabled`), expires_at, redeemed_at, redeemed_user_id, wallet_txn_id
- **gift_card_batches**: batch_no, name, amount, currency, quantity, expires_at, created_by

### Payments
- **payment_channels**
  - name, icon, provider_type, channel_type, interaction_mode (`qr`, `redirect`, `wap`, `page`, `balance`)
  - Fees and limits: fee_rate (percent), fixed_fee, min_amount, max_amount, hide_amount_out_range
  - Filters: payment_roles (`guest`/`member`), member_levels, payment_types (`order`/`wallet`)
  - config (per-gateway JSON), is_active, sort_order
- **payments**
  - order_id (for wallet top-ups this links to the recharge), channel_id, provider/channel/interaction snapshot
  - amount, fee_rate, fixed_fee, fee_amount, currency
  - fee_policy: `none`, `merchant_absorbed`, `customer_surcharge` or `legacy_customer_surcharge`
  - status: `initiated`, `pending`, `success`, `failed` or `expired`
  - exception_code: `superseded_payment_succeeded`, `duplicate_payment_succeeded`, `closed_order_payment_succeeded` or `underpaid_payment_succeeded`
  - provider_ref, gateway_order_no, provider_payload (JSON), pay_url, qr_code
  - Times: paid_at, expired_at, superseded_at, superseded_by_payment_id, callback_at

### Coupons, promotions and member levels
- **coupons**
  - code (unique), type (`fixed`/`percent`), value, min_amount, max_discount
  - Limits: usage_limit, used_count, per_user_limit
  - Flags: disabled_wholesale_price, per_item_discount
  - Filters: payment_roles, member_levels, scope_type (`product`), scope_ref_ids (JSON list)
  - starts_at, ends_at, is_active
- **coupon_usages**: coupon_id, user_id, order_id, discount_amount
- **promotions**: name, scope_type (`product`), scope_ref_id, type (`fixed`, `percent` or `special_price`), value, min_amount, starts_at, ends_at, is_active
- **member_levels**: name (ml-json), slug (unique), icon, discount_rate (100 = full price), recharge_threshold, spend_threshold, is_default, sort_order, is_active
- **member_level_prices**: member_level_id + product_id + sku_id (unique together; sku_id 0 = whole product), price_amount

### Wallet
- **wallet_accounts**: user_id (unique), balance
- **wallet_transactions**
  - user_id, operator_admin_id, order_id, amount, balance_before, balance_after, currency, remark
  - type: `recharge`, `order_pay`, `order_refund`, `admin_adjust`, `admin_refund`, `gift_card_redeem` or `order_underpaid_credit`
  - direction: `in` or `out`
  - reference: unique idempotency key
- **wallet_recharge_orders**: recharge_no (unique), user_id, payment_id (unique), channel snapshot, amount, payable_amount, fee_rate, fee_amount, currency, status (`pending`, `success`, `failed`, `expired`), paid_at

### Affiliate
- **affiliate_profiles**: user_id (unique), affiliate_code (unique), status (`active`/`disabled`)
- **affiliate_clicks**: affiliate_profile_id, visitor_key, landing_path, referrer, client_ip, user_agent
- **affiliate_commissions**
  - affiliate_profile_id + order_id + commission_type (unique together), order_item_id
  - base_amount, rate_percent, commission_amount
  - status: `pending_confirm`, `available`, `rejected` or `withdrawn`
  - confirm_at, available_at, withdraw_request_id, invalid_reason
- **affiliate_withdraw_requests**: profile_id, amount, channel, account, status (`pending_review`, `rejected`, `paid`), reject_reason, processed_by, processed_at

### Reseller
- **reseller_profiles**: user_id (unique), status (`pending_review`, `active`, `rejected`, `disabled`), apply_reason, reject_reason, default_markup_percent, max_markup_percent, settlement_status (`normal`/`frozen`), reviewed_by, reviewed_at
- **reseller_domains**
  - reseller_id, domain (unique among live rows), type (`subdomain`/`custom`)
  - verification_token, verification_status (`pending`, `verified`, `failed`)
  - status (`pending_review`, `active`, `disabled`), is_primary, verified_at
- **reseller_site_configs**: reseller_id (unique), site_name, logo, favicon, and JSON fields announcement, support, seo, footer_links, nav_config, theme
- **reseller_product_settings**
  - reseller_id + product_id + sku_id (unique), is_listed
  - pricing_mode: `inherit`, `markup_percent`, `fixed_markup` or `fixed_price`
  - markup_percent, fixed_markup_amount, fixed_price_amount, sort_order
- **reseller_order_snapshots**: order_id (unique), reseller_id, domain, currency, reseller_user_id, buyer_user_id, base_amount, reseller_amount, profit_amount, profit_eligible, profit_block_reason, pricing_snapshot, risk_snapshot
- **reseller_ledger_entries**
  - reseller_id, order_id, amount, currency, idempotency_key (unique), metadata
  - type: `order_profit`, `refund_deduct`, `manual_adjust`, `withdraw_lock` or `withdraw_paid`
  - status: `pending_confirm`, `available`, `locked`, `withdrawn` or `canceled`
  - available_at, withdraw_request_id, remark
- **reseller_withdraw_requests**: reseller_id, amount, currency, channel, account, status (`pending`, `rejected`, `paid`), reject_reason, processed_by, processed_at
- **reseller_balance_accounts**: reseller_id + currency (unique), status, available / locked / negative amount caches, last_ledger_entry_id, risk_note
- **reseller_related_accounts**: reseller_id + user_id (unique), relation_type, source, status, remark, created_by

### Integration with other shops (site-to-site)
- **api_credentials**: user_id (unique), api_key (unique), api_secret, status (`pending_review`, `approved`, `rejected`, `disabled`), reject_reason, approved_at, is_active, last_used_at
- **site_connections**
  - name, base_url, api_key, api_secret (encrypted), protocol (`dujiao-next`), callback_url, status (`pending`, `active`, `disabled`)
  - last_ping_at, last_ping_ok, retry_max, retry_intervals (JSON, default `[30,60,300]`)
  - Pricing: exchange_rate (decimal 16,6), price_markup_percent, price_rounding_mode (`none`, `ceil_int`, `ceil_tenth`), auto_sync_price
- **procurement_orders**
  - connection_id, local_order_id (foreign key → orders), local_order_no, upstream_order_id, upstream_order_no
  - status: `pending`, `submitted`, `accepted`, `rejected`, `failed`, `partially_refunded`, `fulfilled`, `completed`, `refunded` or `canceled`
  - upstream_amount, upstream_currency, local_sell_amount, currency, error_message, retry_count, next_retry_at, upstream_payload (text), trace_id
- **downstream_order_refs**: order_id (unique), api_credential_id, downstream_order_no, callback_url, trace_id, callback_status, callback_retry_count, last_callback_at
- **reconciliation_jobs**: connection_id, type (`status`, `amount`, `full`), status, time_range_start, time_range_end, counts, result_json, started_at, finished_at
- **reconciliation_items**: job_id, procurement_order_id, local/upstream order_no, local/upstream status, local/upstream amount, mismatch_type (`status`, `amount`, `both`), resolved, resolved_by, resolved_at, remark
- **channel_clients**: name, channel_type, channel_key (unique), channel_secret (encrypted), bot_token (encrypted), callback_url, status (1/0), description, last_used_at
- **telegram_broadcasts**: title, recipient_type (`all`/`specific`), filters, recipient_chat_ids, recipient/success/failed counts, status (`pending`, `running`, `completed`, `failed`), message_html, attachment_url, attachment_name, started_at, completed_at, last_error

### Content
- **posts**: slug (unique), type (`blog`/`notice`), ml-json title / summary / content, thumbnail, category_id, is_published, published_at
- **post_products**: post_id + product_id (unique), sort
- **post_categories**: parent_id, slug, name (ml-json), icon, is_active, sort_order
- **banners**: name, position (`home_hero`), ml-json title / subtitle, image, mobile_image, link_type (`none`, `internal`, `external`), link_value, open_in_new_tab, is_active, start_at, end_at, sort_order
- **media**: name, filename, path (unique, `/uploads/{scene}/{yyyy}/{mm}/{uuid.ext}`), mime_type, size, scene, width, height

### Settings, logs and permissions
- **settings**: key (primary key), value_json — a key/value store; the keys are listed in section 6
- **notification_logs**: event_type, biz_type, biz_id, channel, recipient, locale, title, body, status, error_message, is_test, variables
- **user_login_logs**, **admin_login_logs**, **authz_audit_logs**: login attempts (email or username, status, fail reason, IP, user agent, request_id; admins also have event_type and operator_id) and permission-change audit records
- **casbin_rule**: standard Casbin table (ptype, v0..v5)

---

## 2. HTTP routes

Wiring is in `internal/app/httpserver/router.go`, `routes_storefront.go`, `routes_admin.go`, `routes_channel.go` and `routes_upstream.go`. Each module's routes are in `internal/modules/*/transport/http/routes.go`.

**Global middleware, in order:** RequestID (`X-Request-ID`) → Recovery → Logger → CORS → CallbackRouteMiddleware (admin-configurable custom callback paths; when set, the default callback paths return 404).

**Non-API routes:**
- `/uploads/*` serves static files; SVGs get a forced download and a sandbox CSP.
- `GET /sitemap.xml` and `GET /robots.txt`
- `GET /health` returns `{"status":"ok"}`
- In the "fullstack" build, the admin single-page app is served at `web.admin_path` and the user app at `/`.

**Rate limits** (Redis, with an in-memory fallback):

| Route group | Keyed by | Limit |
|---|---|---|
| User login | IP + email | from config (5 per 300 s, blocked 900 s) |
| Admin login | IP | from config (same) |
| Guest reads | IP | 120 per minute |
| Guest writes | IP | 20 per minute, blocked 300 s |
| Upstream API | API key | 60 per minute |
| Channel API | IP + channel key | 600 per minute |
| Callbacks | IP | 120 per minute |
| Gift-card redeem | user + IP | 10 per minute, blocked 300 s |

### A. Storefront (`/api/v1`, all through the reseller-tenant middleware)

**Public — `/api/v1/public`, no auth**

| Method | Path | Purpose |
|---|---|---|
| GET | `/public/config` | Site config (see section 6) |
| GET | `/public/products` | Product list |
| GET | `/public/products/:slug` | Product detail |
| GET | `/public/categories` | Categories |
| GET | `/public/posts`, `/public/posts/:slug` | Posts |
| GET | `/public/banners` | Banners |
| GET | `/public/post-categories` | Post categories |
| GET | `/public/captcha/image` | Image captcha challenge |
| POST | `/public/affiliate/click` | Track an affiliate click |
| GET | `/public/member-levels` | Member levels |

**Guest — `/api/v1/guest`, uses the `Guest` header except when creating**

| Method | Path | Purpose |
|---|---|---|
| POST | `/guest/orders/preview` | Price preview |
| GET | `/guest/orders` | List guest orders |
| GET | `/guest/orders/:order_no` | Order detail |
| GET | `/guest/orders/:order_no/fulfillment/download` | Download delivered content |
| GET | `/guest/payments/latest` | Latest payment (read limit) |
| POST | `/guest/orders` | Create order (write limit, captcha scene `guest_create_order`) |
| POST | `/guest/orders/create-and-pay` | Create and pay in one step |
| POST | `/guest/payments` | Start payment (`order_no`, `channel_id`) |
| POST | `/guest/payments/:id/capture` | Actively query the gateway |

**Auth — `/api/v1/auth`, no auth**

| Method | Path | Purpose |
|---|---|---|
| POST | `/auth/send-verify-code` | Send email code (`email`, `purpose`, `captcha_payload`) |
| POST | `/auth/register` | Register (`email`, `password`, `code`, `agreement_accepted`) |
| POST | `/auth/login` | Login (`email`, `password`, `remember_me`, `captcha_payload`); rate limited |
| POST | `/auth/login/verify-2fa` | Finish 2FA login |
| POST | `/auth/forgot-password` | Reset (`email`, `code`, `new_password`) |
| POST | `/auth/telegram/login` | Telegram widget login |
| POST | `/auth/telegram/miniapp/login` | Telegram mini-app login |
| GET | `/auth/telegram/oidc/start` | Start Telegram OIDC |
| POST | `/auth/telegram/oidc/callback` | Finish Telegram OIDC |
| POST | `/auth/google/login` | Google ID-token login |
| POST | `/auth/google/redirect/intent` | Google redirect-flow intent |
| POST | `/auth/google/redirect/callback` | Google `form_post` callback |
| POST | `/auth/google/redirect/exchange` | Exchange for a login |

**User — `/api/v1`, needs a user JWT**

*Account*

| Method | Path | Purpose |
|---|---|---|
| GET | `/me` | Current user |
| PUT | `/me/profile` | Update profile |
| POST | `/me/email/send-verify-code` | Code for email change |
| POST | `/me/email/change` | Change email |
| PUT | `/me/password` | Change password |
| GET | `/me/login-logs` | Own login history |
| GET | `/me/2fa/status` | 2FA status |
| POST | `/me/2fa/setup`, `/enable`, `/disable`, `/recovery-codes/regenerate` | Manage 2FA |
| GET | `/me/telegram` | Telegram binding |
| POST | `/me/telegram/bind`, `/me/telegram/miniapp/bind` | Bind Telegram |
| DELETE | `/me/telegram/unbind` | Unbind Telegram |
| GET / POST | `/me/telegram/oidc/start`, `/me/telegram/oidc/callback` | Bind via OIDC |
| GET | `/me/google` | Google binding |
| POST | `/me/google/bind`, `/me/google/redirect/intent`, `/me/google/redirect/exchange` | Bind Google |
| DELETE | `/me/google/unbind` | Unbind Google |

*Cart, orders and payments*

| Method | Path | Purpose |
|---|---|---|
| GET | `/cart` | View cart |
| POST | `/cart/items` | Add or update item |
| DELETE | `/cart/items/:product_id` | Remove item |
| POST | `/orders/preview` | Price preview |
| POST | `/orders` | Create order |
| POST | `/orders/create-and-pay` | Create and pay (`channel_id`, `use_balance`) |
| POST | `/order/payment-channels` | Allowed channels (`amount`, `order_no`, `items`) |
| GET | `/orders`, `/orders/stats`, `/orders/:order_no` | List, stats, detail |
| GET | `/orders/:order_no/fulfillment/download` | Download delivered content |
| POST | `/orders/:order_no/cancel` | Cancel |
| POST | `/payments` | Start payment (`order_no`, `channel_id`, `use_balance`) |
| POST | `/payments/:id/capture` | Actively query the gateway |
| GET | `/payments/latest` | Latest payment |

*Wallet, gift cards and affiliate*

| Method | Path | Purpose |
|---|---|---|
| GET | `/wallet` | Balance |
| GET | `/wallet/transactions` | Transactions |
| POST | `/wallet/payment-channels` | Top-up channels |
| POST | `/wallet/recharge` | Start a top-up |
| GET | `/wallet/recharges`, `/wallet/recharges/stats`, `/wallet/recharges/:recharge_no` | Top-up history |
| POST | `/wallet/recharge/payments/:id/capture` | Query a top-up payment |
| POST | `/gift-cards/redeem` | Redeem (rate limited, captcha) |
| POST | `/affiliate/open` | Become an affiliate |
| GET | `/affiliate/dashboard`, `/affiliate/commissions`, `/affiliate/withdraws` | Affiliate data |
| POST | `/affiliate/withdraws` | Request withdrawal |

*API access (for other shops to connect)*

| Method | Path | Purpose |
|---|---|---|
| GET | `/api-credential` | View own credential |
| POST | `/api-credential/apply` | Apply |
| POST | `/api-credential/regenerate` | Regenerate secret |
| PUT | `/api-credential/status` | Enable / disable |

*Reseller console — `/api/v1/reseller/*`; blocked when accessed from a reseller's own domain*

| Method | Path | Purpose |
|---|---|---|
| GET | `/reseller/profile` | Profile snapshot |
| POST | `/reseller/apply` | Apply to become a reseller |
| GET / POST | `/reseller/domains` | List / submit custom domain |
| GET / PUT | `/reseller/site-config` | Branding of the reseller site |
| POST | `/reseller/upload` | Upload image |
| GET | `/reseller/product-settings`, `/reseller/product-settings/:product_id` | Pricing settings |
| POST | `/reseller/product-settings/:product_id/preview` | Preview pricing |
| PUT / DELETE | `/reseller/product-settings/:product_id` | Save / reset pricing |
| GET | `/reseller/dashboard`, `/reseller/balance-accounts`, `/reseller/ledger-entries` | Finance views |
| GET / POST | `/reseller/withdraws` | List / request withdrawal |
| GET | `/reseller/orders`, `/reseller/orders/stats`, `/reseller/orders/:order_no` | Orders |

### B. Payment callbacks (`/api/v1`, no auth, callback rate limit)

| Method | Path | Purpose |
|---|---|---|
| GET, POST | `/payments/callback` | Shared endpoint for WeChat, OKPay, Alipay, epay, TokenPay, epusdt, BEpusdt |
| POST | `/payments/webhook/dujiaopay?channel_id=` | DujiaoPay webhook |
| POST | `/payments/webhook/paypal?channel_id=` | PayPal webhook |
| POST | `/payments/webhook/stripe?channel_id=` | Stripe webhook |

All four paths can be replaced by admin-set custom paths (settings key `callback_routes_config`).

### C. Upstream API (`/api/v1/upstream`) — this shop acting as supplier to another shop

**Auth:**
- Headers: `Dujiao-Next-Api-Key`, `Dujiao-Next-Timestamp`, `Dujiao-Next-Signature`
- Signature = HMAC-SHA256(secret, `"{METHOD}\n{path}\n{ts}\n{md5hex(body)}"`), hex
- Timestamp must be within ±60 s
- Credential must be `approved` + `is_active`, and its user must be `active`
- Errors come back as `{ok:false, error_code, error_message}` with real HTTP status codes

| Method | Path | Purpose |
|---|---|---|
| POST | `/upstream/ping` | Check connection, returns site name, balance, member level |
| GET | `/upstream/categories` | Categories |
| GET | `/upstream/products`, `/upstream/products/:id` | Products |
| POST | `/upstream/orders` | Create order; paid automatically from the caller's wallet (HTTP 402 if balance too low) |
| GET | `/upstream/orders/:id` | Order status |
| POST | `/upstream/orders/:id/cancel` | Cancel |
| POST | `/upstream/callback` | This shop acting as buyer: receives the supplier's callback, verified with the site connection's key/secret; replies `{ok,message}` |

### D. Channel API (`/api/v1/channel`) — used by the Telegram bot

**Auth:**
- Headers: `Dujiao-Next-Channel-Key`, `-Channel-Timestamp`, `-Channel-Signature` (same HMAC scheme as above, with the client's decrypted secret)
- Response shape: `{status_code, msg, data?, error_code?, request_id?}` with real HTTP status codes

| Method | Path | Purpose |
|---|---|---|
| GET | `/channel/telegram/config` | Bot configuration |
| POST | `/channel/telegram/heartbeat` | Bot heartbeat |
| POST | `/channel/identities/telegram/resolve`, `/provision`, `/bind` | Map Telegram users to shop users |
| GET | `/channel/me` | Current identity |
| GET | `/channel/catalog/categories`, `/channel/catalog/products`, `/channel/catalog/products/:id` | Catalog |
| POST | `/channel/orders/preview`, `/channel/orders` | Preview / create order |
| GET | `/channel/orders`, `/channel/orders/by-order-no/:order_no`, `/channel/orders/:id` | Orders |
| POST | `/channel/orders/:id/cancel` | Cancel |
| GET | `/channel/payment-channels`, `/channel/payment-methods` | Payment channels (same data) |
| GET | `/channel/payments/latest`, `/channel/payments/:id` | Payments |
| POST | `/channel/payments` | Start payment |
| GET | `/channel/wallet`, `/channel/wallet/transactions` | Wallet |
| POST | `/channel/wallet/recharge` | Top up |
| POST | `/channel/wallet/gift-card/redeem` | Redeem gift card |
| POST | `/channel/affiliate/click`, `/channel/affiliate/open` | Affiliate tracking / open |
| GET | `/channel/affiliate/dashboard`, `/channel/affiliate/commissions`, `/channel/affiliate/withdraws` | Affiliate data |
| POST | `/channel/affiliate/withdraws` | Request withdrawal |
| GET | `/channel/member-levels` | Member levels |

### E. Admin (`/api/v1/admin`)

- `POST /admin/login` and `POST /admin/login/verify-2fa` need no token (IP rate limit).
- Everything else needs an admin JWT, then the Casbin permission check.
- Routes marked **[C]** also need the compliance acknowledgement; without it they return 403 with `msg` `compliance_required` (or `compliance_required_by_super_admin` for non-super admins).

**Account, permissions and system**

| Method | Path | Purpose |
|---|---|---|
| PUT | `/password` | Change own password |
| GET | `/2fa/status` | 2FA status |
| POST | `/2fa/setup`, `/2fa/enable`, `/2fa/disable`, `/2fa/recovery-codes/regenerate` | Manage own 2FA |
| POST | `/authz/admins/:id/2fa/reset` | Reset another admin's 2FA (super admin only) |
| DELETE | `/users/:id/2fa` | Reset a user's 2FA |
| GET | `/authz/me` | Own roles and permissions |
| GET / POST | `/authz/roles` | List / create roles |
| DELETE | `/authz/roles/:role` | Delete role |
| GET | `/authz/roles/:role/policies` | Role permissions |
| POST / DELETE | `/authz/policies` | Grant / revoke a permission |
| GET / POST | `/authz/admins` | List / create admins |
| PUT / DELETE | `/authz/admins/:id` | Edit / delete admin |
| GET / PUT | `/authz/admins/:id/roles` | Admin's roles |
| GET | `/authz/permissions/catalog` | All admin routes, built from the route list |
| GET | `/authz/audit-logs` | Permission audit log |
| GET | `/user-login-logs` | User login history |
| GET | `/system/version/check`, `/system/update/capability`, `/system/update/status` | Update info |
| POST | `/system/update/start`, `/system/update/rollback`, `/system/restart` | Self-update |
| GET | `/compliance/status` | Compliance status |
| POST | `/compliance/acknowledge` | Acknowledge compliance |
| GET | `/dashboard/overview`, `/dashboard/trends`, `/dashboard/rankings`, `/dashboard/inventory-alerts` | Dashboard |
| GET | `/ads/render/:slotCode` | Ad slot (proxied from ads-gateway.dujiao-next.com) |
| POST | `/ads/impression` | Ad impression tracking |

**Catalog and content**

| Method | Path | Purpose |
|---|---|---|
| GET / POST | `/products` | List / create |
| GET / PUT / PATCH / DELETE | `/products/:id` | Read / update / quick update / delete |
| PATCH | `/products/:id/wholesale-prices` | Wholesale tiers |
| POST | `/products/batch-status`, `/products/batch-category`, `/products/batch-delete` | Batch actions |
| GET / POST | `/categories` | List / create |
| PUT / DELETE | `/categories/:id` | Update / delete |
| PATCH | `/categories/:id/active` | Toggle active |
| GET / POST | `/posts` | List / create |
| PUT / DELETE | `/posts/:id` | Update / delete |
| GET | `/posts/:id/products` | Linked products |
| GET / POST | `/post-categories` | List / create |
| PUT / DELETE | `/post-categories/:id` | Update / delete |
| PATCH | `/post-categories/:id/status` | Toggle status |
| GET / POST | `/banners` | List / create |
| GET / PUT / DELETE | `/banners/:id` | Read / update / delete |
| GET | `/media` | Media library |
| PUT / DELETE | `/media/:id` | Rename / delete |
| POST | `/media/batch-delete` | Batch delete |
| POST | `/upload` | Upload file (multipart; `scene` field) |

**Settings**

| Method | Path | Purpose |
|---|---|---|
| GET | `/settings?key=` | Read any setting key (default `site_config`) |
| PUT | `/settings` | Write `{key, value}`; `google_auth_config` must use its own route |
| GET / PUT | `/settings/smtp` | SMTP |
| POST | `/settings/smtp/test` | Test SMTP |
| GET / PUT | `/settings/captcha` | Captcha |
| GET / PUT | `/settings/telegram-auth` | Telegram login |
| GET / PUT | `/settings/google-auth` | Google login |
| GET / PUT | `/settings/affiliate` | Affiliate |
| GET / PUT | `/settings/order-email-template` | Order emails |
| POST | `/settings/order-email-template/reset` | Reset templates |
| GET / PUT | `/settings/telegram-bot` | Bot config |
| GET | `/settings/telegram-bot/runtime-status` | Bot status |
| GET / PUT | `/settings/notification-center` (alias `/settings/notifications`) | Notification settings |
| GET | `/settings/notification-center/logs` | Notification log |
| POST | `/settings/notification-center/test` | Send a test notification |

**Orders and fulfillment**

| Method | Path | Purpose |
|---|---|---|
| GET | `/orders` | List |
| GET | `/orders/:id` | Detail |
| PATCH | `/orders/:id` | Change status (`{status}`) |
| POST | `/orders/:id/refund-to-wallet` | Refund to wallet (`amount`, `remark`) |
| POST | `/orders/:id/manual-refund` | Record a manual refund (`amount`, `remark`, `payment_fee_refunded`) |
| GET | `/order-refunds`, `/order-refunds/:id` | Refund records |
| PATCH | `/order-refunds/:id/payment-fee` | Set fee-refunded flag |
| GET | `/orders/:id/fulfillment/download` | Download delivered content |
| POST | `/fulfillments` | Manual delivery |
| POST | `/card-secrets/batch`, `/card-secrets/import` | Add card secrets (manual / CSV) |
| GET | `/card-secrets` | List |
| PUT | `/card-secrets/:id` | Edit |
| PATCH | `/card-secrets/batch-status` | Batch status |
| POST | `/card-secrets/batch-delete`, `/card-secrets/export`, `/card-secrets/export-available` | Delete / export |
| GET | `/card-secrets/stats`, `/card-secrets/batches`, `/card-secrets/template` | Stats, batches, CSV template |
| POST | `/gift-cards/generate` | Generate gift cards |
| GET | `/gift-cards` | List |
| PUT / DELETE | `/gift-cards/:id` | Edit / delete |
| PATCH | `/gift-cards/batch-status` | Batch status |
| POST | `/gift-cards/export` | Export |

**Coupons, promotions and member levels**

| Method | Path | Purpose |
|---|---|---|
| GET / POST | `/coupons` | List / create |
| PUT / DELETE | `/coupons/:id` | Update / delete |
| GET / POST | `/promotions` | List / create |
| PUT / DELETE | `/promotions/:id` | Update / delete |
| GET / POST | `/member-levels` | List / create |
| PUT / DELETE | `/member-levels/:id` | Update / delete |
| POST | `/member-levels/backfill` | Give all users the default level |
| GET | `/member-level-prices` | Level prices |
| POST | `/member-level-prices/batch` | Batch upsert |
| DELETE | `/member-level-prices/:id` | Delete |
| PUT | `/users/:id/member-level` | Set a user's level |

**Payments [C]**

| Method | Path | Purpose |
|---|---|---|
| GET / POST | `/payment-channels` | List / create |
| GET / PUT / DELETE | `/payment-channels/:id` | Read / update / delete |
| POST | `/payment-channels/:id/wechatpay-public-key-test` | Test WeChat Pay key |
| GET | `/payment-channels/:id/trade-bill?file_date=yyyyMMdd` | Query Huifu `TRADE_BILL` generation and files |
| GET | `/payment-channels/:id/trade-bill/download?file_date=yyyyMMdd&file_id=...` | Re-query, verify and download one Huifu trade-bill file |
| GET | `/payments`, `/payments/export`, `/payments/:id` | Payment records |

**Users and wallet**

| Method | Path | Purpose |
|---|---|---|
| GET | `/users` | List |
| PUT | `/users/batch-status` | Batch enable / disable |
| GET / PUT | `/users/:id` | Read / update |
| GET | `/users/:id/coupon-usages` | Coupon usage |
| DELETE | `/users/:id/oauth/telegram`, `/users/:id/oauth/google` | Unbind logins |
| GET | `/users/:id/wallet`, `/users/:id/wallet/transactions` **[C]** | Wallet |
| POST | `/users/:id/wallet/adjust` **[C]** | Adjust balance |
| GET | `/wallet/recharges` **[C]** | Top-up records |

**Affiliate**

| Method | Path | Purpose |
|---|---|---|
| GET | `/affiliates/users` | Affiliates |
| PATCH | `/affiliates/users/:id/status`, `/affiliates/users/batch-status` | Status |
| GET | `/affiliates/commissions`, `/affiliates/withdraws` **[C]** | Finance |
| POST | `/affiliates/withdraws/:id/reject`, `/affiliates/withdraws/:id/pay` **[C]** | Review withdrawals |

**Resellers**

| Method | Path | Purpose |
|---|---|---|
| GET | `/resellers/operations/overview` | Overview |
| GET | `/resellers/profiles`, `/resellers/profiles/:id` | Profiles |
| PUT | `/resellers/profiles/:id` | Update profile |
| PUT | `/resellers/profiles/:id/system-domain` | Assign subdomain |
| POST | `/resellers/profiles/:id/approve`, `/reject`, `/disable`, `/restore` | Review |
| GET | `/resellers/domains` | Domains |
| POST | `/resellers/domains/:id/approve`, `/disable`, `/set-primary` | Domain actions |
| GET | `/resellers/site-configs`, `/resellers/site-configs/:reseller_id` | Site configs |
| PUT | `/resellers/site-configs/:reseller_id` | Update site config |
| POST | `/resellers/site-configs/:reseller_id/reset` | Reset site config |
| GET | `/resellers/product-settings`, `/resellers/product-settings/:reseller_id/:product_id` | Pricing settings |
| POST | `/resellers/product-settings/:reseller_id/:product_id/preview` | Preview |
| PUT / DELETE | `/resellers/product-settings/:reseller_id/:product_id` | Save / reset |
| GET | `/resellers/operations/finance`, `/resellers/ledger-entries`, `/resellers/balance-accounts`, `/resellers/withdraws` **[C]** | Finance |
| POST | `/resellers/withdraws/:id/reject`, `/resellers/withdraws/:id/pay` **[C]** | Review withdrawals |

**Integration**

| Method | Path | Purpose |
|---|---|---|
| GET | `/api-credentials`, `/api-credentials/:id` | Credentials |
| POST | `/api-credentials/:id/approve`, `/api-credentials/:id/reject` | Review |
| PUT | `/api-credentials/:id/status` | Enable / disable |
| DELETE | `/api-credentials/:id` | Delete |
| GET / POST | `/site-connections` | List / create |
| GET / PUT / DELETE | `/site-connections/:id` | Read / update / delete |
| POST | `/site-connections/:id/ping` | Test connection |
| PUT | `/site-connections/:id/status` | Status |
| POST | `/site-connections/:id/reapply-markup` | Reapply pricing markup |
| GET | `/product-mappings`, `/product-mappings/:id` | Mappings |
| POST | `/product-mappings/import`, `/batch-import`, `/batch-import-by-category` | Import supplier products |
| POST | `/product-mappings/:id/sync`, `/batch-sync` | Sync |
| PUT | `/product-mappings/:id/status` | Status |
| POST | `/product-mappings/batch-status`, `/batch-delete` | Batch actions |
| DELETE | `/product-mappings/:id` | Delete |
| GET | `/upstream-products`, `/upstream-categories` | Browse the supplier's catalog |
| GET | `/procurement-orders`, `/procurement-orders/stats`, `/procurement-orders/:id` | Purchase orders |
| GET | `/procurement-orders/:id/upstream-payload/download` | Download supplier delivery |
| POST | `/procurement-orders/:id/retry`, `/procurement-orders/:id/cancel` | Retry / cancel |
| POST | `/reconciliation/run` **[C]** | Start reconciliation |
| GET | `/reconciliation/jobs`, `/reconciliation/jobs/:id` **[C]** | Jobs |
| PUT | `/reconciliation/items/:id/resolve` **[C]** | Resolve a mismatch |
| GET / POST | `/channel-clients` | List / create |
| GET / PUT / DELETE | `/channel-clients/:id` | Read / update / delete |
| PUT | `/channel-clients/:id/status` | Status |
| POST | `/channel-clients/:id/reset-secret` | New secret |
| GET / POST | `/telegram-bot/broadcasts` | List / create broadcast |
| GET | `/telegram-bot/broadcasts/:id` | Broadcast detail |
| GET | `/telegram-bot/users` | Bot users |

---

## 3. Response format, errors, JWT, permissions, 2FA

**Response envelope** (`internal/platform/http/response/response.go`):
- Success: `{"status_code":0,"msg":"success","data":…}` with HTTP 200.
- Paged: the same plus `"pagination":{"page","page_size","total","total_page"}`.
- Error: HTTP **200** with `{"status_code":<code>,"msg":<translated text>,"data":{"request_id":…}}`.
- Only middleware errors (JWT problems, recovery) sometimes use a real HTTP status via `ErrorWithHTTPStatus`.
- Codes (`codes.go`): 0, 400, 401, 403, 404, 429, 500.
- Messages are translation keys like `error.token_invalid`. The table has about 1,000 lines in `internal/i18n/messages.go` for zh-CN, zh-TW and en-US.
- Language is picked from `?lang=`, then the `X-Lang` header, then `Accept-Language`; default is zh-CN.

**Pagination** (`internal/platform/http/ginutil/pagination.go`): query `page` (default 1) and `page_size` (default 20, max 200).

**JWT:**

| | Admin | User |
|---|---|---|
| Algorithm | HS256 only | HS256 only |
| Secret (config) | `jwt.secret` | `user_jwt.secret` |
| Lifetime | `expire_hours` | `expire_hours`, or `remember_me_expire_hours` if "remember me" |
| Claims | `admin_id`, `username`, `token_version`, `typ:"access"`, exp/iat/nbf | `user_id`, `email`, `token_version`, `typ` |
| Code | `modules/identity/adminauth/application/service.go` | `modules/identity/userauth/application/service.go` |

- The middleware (`internal/app/httpserver/middleware/middleware.go`) reads `Authorization: Bearer`.
- It rejects any `typ` other than `access` (empty is also accepted, for old tokens).
- It compares `token_version` and `iat >= token_invalid_before` against the account row. That state is cached in Redis (`cache/auth_state.go`).
- User tokens are also rejected if the user is not active.

**Casbin RBAC** (`internal/authz/service.go`, `bootstrap.go`):
- Model: `r = sub, obj, act`; `p = sub, obj, act`; `g = _, _`; matcher `(g(r.sub,p.sub) || r.sub==p.sub) && keyMatch2(r.obj,p.obj) && (r.act==p.act || p.act=="*")`.
- Subject is `admin:{id}`; roles are named `role:{name}` and anchored to `role:__anchor__`.
- Built-in roles are locked (their rules are reset at startup):
  - `readonly_auditor`: dashboard, own password, own 2FA, ads
  - `operations`: products, categories, posts, banners, coupons, promotions, card secrets, gift cards, media and upload, affiliate users, member levels
  - `support`: read orders and refunds, manual delivery, user management, read wallets and payments
  - `integration`: site connections, product mappings, purchase orders, reconciliation, API credentials, reseller management
  - `finance`: payments, payment channels, order refunds and status, affiliate and reseller withdrawals, gift-card export, wallet adjust
  - `system_admin`: all settings, permission management, system updates, channel clients, broadcasts, compliance acknowledgement, disabling resellers
- All roles except `readonly_auditor` inherit from it.

**TOTP 2FA:**
- Library `pquerna/otp`: 6 digits, 30 s period, ±1 step allowed. Issuer is `app.totp_issuer`.
- Setup returns `{secret, otpauth_url, expires_at}`. The pending secret lives 10 minutes.
- Enabling returns 10 recovery codes (`xxxx-xxxx`, stored bcrypt-hashed).
- Login with 2FA returns `{requires_totp:true, challenge_token, challenge_expires_at}`. The challenge is a JWT with `typ:"2fa_challenge"`, lifetime 5 minutes, at most 5 failed attempts, tracked in Redis (`2fa:challenge:{jti}:fails` / `:revoked`).
- `verify-2fa` takes `{challenge_token, code | recovery_code}`.
- Without 2FA, login returns `{requires_totp:false, token, user:{…}, expires_at}`.
- Command-line recovery: `server admin list-admins | reset-2fa | reset-password` (`internal/admincmd`).

---

## 4. Business flows

**Order creation** (`modules/order/application/order_service.go`, `order_service_validate.go`):
1. Fail with `ErrQueueUnavailable` if the job queue is off.
2. Risk checks from `order_risk_control_config` (`modules/orderrisk`): IP blacklist; limits on pending orders per IP or user and quantity per product; rate limit. Guests can get a shorter payment window.
3. Merge duplicate items. For each item: product must be active; check purchase quantity; members-only products cannot be bought by guests; pick the SKU.
4. Price each item from the SKU price:
   1. apply any active promotion;
   2. compare with the wholesale tier and keep the cheaper price (they never stack);
   3. apply the member-level special price on top;
   4. after all items, apply the coupon to the eligible items.
   - Resellers cannot use coupons, member prices or wholesale prices.
5. Check stock: manual stock, or ask the supplier for stock on `upstream` items. Validate the manual form against the product's form schema.
6. Reseller pricing is applied, and if the site is set to wallet-only payment the balance must cover the total.
7. In one database transaction:
   - create the parent order;
   - create one child order per item (`{order_no}-{n}`);
   - for `auto` items, lock and reserve card secrets (`reserved`);
   - for `manual` items with limited stock, reserve SKU stock;
   - record coupon usage (with row locks and limit checks);
   - save the reseller snapshot.
8. Schedule the timeout-cancel job to run after the payment window; if scheduling fails, roll the order back. The window comes from `order_config.payment_expire_minutes`, default 15.

**Payment** (`modules/payment/application/*`):
- `CreatePayment`: optionally take wallet balance first; if the balance covers everything the order is paid at once. Otherwise:
  - pick the channel;
  - check the product's allowed channels, amount limits, role, member level and payment type;
  - apply the fee policy snapshot;
  - call the gateway to get a pay URL or QR code;
  - older pending payment links are marked superseded.
- Callback (`applyPaymentUpdate`), under row locks:
  - a payment already in success is ignored;
  - the paid amount must cover `total − wallet_paid`, otherwise the money goes to the user's wallet as `order_underpaid_credit` and the payment is flagged underpaid;
  - on success, other pending payments for the order expire; the order and its children become `paid`, or `fulfilling` for children with manual items; manual stock is consumed; reseller profit is posted;
  - on failure or expiry, the wallet part is returned.
- After a successful payment (`payment_service_callback_dispatch.go`):
  - affiliate commission is calculated;
  - status email is queued (skipped when the whole order is auto-delivered);
  - admin notifications and the Telegram-bot notification are queued;
  - member-level upgrade is checked;
  - for each child: the auto-delivery job is queued, or a "manual delivery pending" alert is sent;
  - a purchase order is created for `upstream` items;
  - if the buyer is another shop, its callback is queued.

**Fulfillment** (`modules/fulfillment/application/service.go`):
- **Auto:** use the reserved secrets (or take available ones), mark them `used`, save a fulfillment row with the secrets one per line, set the child to `completed`, recalculate the parent, send the email, notify the bot, and call back the buying shop if any.
- **Manual:** admin `POST /admin/fulfillments` with delivery data, which becomes the payload plus structured logistics.

**Timeout cancel:** cancels the order and its children, releases reserved secrets and stock, restores the coupon, returns wallet money, and records affiliate cancellation.

**Refunds** (`modules/order/application/refund/`): refund to wallet or record a manual refund. The allowed period is `max_refund_days`. Affiliate and reseller amounts are clawed back and a refund email is sent.

**Cart:** simple rows per user, product and SKU; any fulfillment type.

**Coupons:** fixed or percent; minimum spend; maximum discount; total and per-user limits; product scope; can exclude wholesale-priced items; optional per-item discount; role and level filters.

**Promotions:** per-product fixed, percent or special price with a time window.

**Wallet:**
- Top-up: `/wallet/recharge` creates a recharge order and a payment. The callback credits the wallet and triggers member-level and notification updates. A timeout job expires unpaid top-ups.
- Admins can adjust balances. Every movement is a transaction row with a unique reference.
- Settings: `wallet_config` (`wallet_only_payment`, top-up channel list).

**Affiliate:**
- Visitors are tracked by `affiliate_code` and visitor key; the order keeps a snapshot.
- Commission = base amount (only affiliate-enabled products) × `commission_rate`%.
- It stays `pending_confirm` for `confirm_days`, then becomes `available` (job every minute).
- Withdrawals need at least `min_withdraw_amount` and use a configured channel; admins pay or reject.
- Self-referral is ignored.

**Member levels:**
- A default level is given at registration.
- Users are upgraded automatically when `total_recharged` or `total_spent` pass a level's threshold.
- Each level has a global discount rate plus optional special prices per product or SKU.

**Gift cards:** admins generate batches. Redeeming (user or bot, captcha scene `gift_card_redeem`) credits the wallet with a `gift_card_redeem` transaction and marks the card `redeemed`, all in one transaction (`internal/workflows/giftcardredeem`).

**Resellers** (white-label shops):
- A user applies; an admin approves and assigns a subdomain under `reseller.subdomain_base`, or approves a custom domain.
- Requests are matched to a reseller by Host (`ResellerTenantMiddleware`, cached); `main_hosts` means the main shop.
- The reseller controls branding (`reseller_site_configs`, layered over `/public/config`) and which products are listed and how they are priced (inherit, markup %, fixed markup, fixed price, capped by `max_markup_percent`).
- Each order records the reseller profit. It is posted to the ledger as `pending_confirm` and becomes `available` after `settlement_confirm_days` (job every minute). Resellers request withdrawals; admins pay them.

**Supplying other shops, and buying from other shops:**
- *As supplier:* a user applies for API credentials; the other shop calls `/api/v1/upstream/*`. Their orders are paid from the user's wallet here. `downstream_order_refs` stores their callback URL, and the `downstream:callback` job notifies them.
- *As buyer:*
  - an admin sets up a `site_connection` (URL, key, encrypted secret, exchange rate and markup);
  - supplier products are imported into local products with `fulfillment_type=upstream` and `is_mapped`, with images downloaded to `uploads`;
  - stock is synced on a schedule;
  - a paid upstream item creates a purchase order; the submit job calls the supplier, which is paid from our wallet there;
  - the result arrives via the supplier's callback to `/api/v1/upstream/callback`, or by the poll job;
  - the delivery is copied into the local fulfillment.
- Reconciliation jobs compare purchase-order status and amounts with the supplier.
- Adapter: `internal/upstream/dujiao_next.go`.

**Channel API / Telegram bot:** an external bot process authenticates as a `channel_client`. It links Telegram users to shop users (resolve, provision, bind), browses, orders, pays and checks balances, and pulls its config and reports heartbeats. The shop pushes events to Telegram users through the `bot:notify` job. Admin broadcasts go through the `telegram:broadcast` job.

**Notifications** (`modules/notification`):
- Channels: email (SMTP), Telegram (Bot API, using the `telegram_auth` bot token) and Feishu/Lark.
- Events: `wallet_recharge_success`, `order_paid_success`, `manual_fulfillment_pending`, `exception_alert`, and a periodic alert check.
- Templates exist per event and language. Duplicates are suppressed for a TTL; every send is logged to `notification_logs`.
- Customer order emails use `order_email_template_config` (default, paid, delivered, delivered with content, refunded, partially refunded) and go through the `order:status_email` job.

**Captcha** (`modules/captcha`):
- Provider: `none`, `image` (base64Captcha, stored in memory) or `turnstile` (Cloudflare).
- Can be switched on per scene: `login`, `register_send_code`, `reset_send_code`, `guest_create_order`, `gift_card_redeem`.
- Request field: `captcha_payload: {captcha_id, captcha_code, turnstile_token}`.

**Uploads** (`modules/upload`):
- Admin `POST /admin/upload` (multipart, with `scene`) and reseller `/reseller/upload`.
- Scenes: `product`, `post`, `banner`, `editor`, `common`, `category`, `telegram`, `reseller`.
- Type, extension, size and pixel limits come from config.
- Files are stored at `./uploads/{scene}/{yyyy}/{mm}/{uuid}.ext` and recorded in the `media` table.

**Other:** Google login uses ID-token verification against Google's keys (the iOS redirect flow needs Redis GETDEL). The `selfupdate` module does binary self-update and rollback.

---

## 5. Payment providers

- **Registration:** `internal/app/container/bootstrap.go` registers each gateway by provider type and channel type.
- **Adapters:** `modules/payment/infrastructure/gateway/*`.
- **Interface:** `modules/payment/contract/gateway.go`. Every gateway must validate its config and create a payment; some can also query status, parse a webhook, verify a callback, or run a security test.

| provider_type | Channels | Config keys | Callback |
|---|---|---|---|
| `official` + `alipay` | Alipay | app_id, private_key, alipay_public_key, gateway_url, sign_type, cert SNs, notify/return_url | shared endpoint (form); reply `success`/`fail` |
| `official` + `wechat` | WeChat Pay API v3 | appid, mchid, merchant_serial_no, merchant_private_key, api_v3_key, verification_mode, wechatpay public key, H5 settings | shared endpoint, `?channel_id=`, signed headers |
| `official` + `paypal` | PayPal | client_id/secret, base_url, webhook_id, target_currency, exchange_rate… | `/payments/webhook/paypal?channel_id=` |
| `official` + `stripe` | Stripe | secret_key, publishable_key, webhook_secret, payment_method_types… | `/payments/webhook/stripe?channel_id=` |
| `epay` | wechat, alipay, qqpay (易支付 / "epay"-protocol gateways) | gateway_url, epay_version, merchant_id/key or RSA keys, sign_type, api_path | shared endpoint; matched by `pid` + `out_trade_no` + `trade_status`; reply `success`/`fail` |
| `huifu` | alipay, wechat (hosted H5/PC) | sys_id, product_id, huifu_id, merchant RSA private key, Huifu RSA public key, hosted project | shared endpoint; RSA-SHA256 verified `resp_data`; dynamic `RECV_ORD_ID_*` reply |
| `epusdt` | USDT (GMPay) | gateway_url, pid, secret_key, order_mode, token, network, currency | shared endpoint; reply `ok` |
| `bepusdt` | USDT, TRX, USDC | gateway_url, auth_token, trade_type, order_mode, fiat, address | shared endpoint; reply `success` |
| `tokenpay` | crypto | gateway_url, notify_secret, currency, base_currency | shared endpoint; reply `ok` |
| `okpay` | crypto | gateway_url, merchant_id, merchant_token, exchange_rate, coin | shared endpoint; reply `{"status":"success"}` |
| `dujiaopay` | crypto (transaction or cashier mode) | api_base_url, api_key_id, api_secret, webhook_secret, chain, token_id, fiat_currency | `/payments/webhook/dujiaopay?channel_id=` |
| `wallet` | balance | – | internal, no gateway |

- **Shared endpoint** (`modules/payment/transport/http/callback/handler.go`): tries each format in this order — WeChat, OKPay, Alipay, epay, TokenPay, epusdt, BEpusdt. If none matches it returns 404 and queues an alert.
- The payment is found by `gateway_order_no` (a separate gateway order number for some providers) or `provider_ref`.
- Signatures are verified with the channel's config. The paid amount, currency and order number are checked before `applyPaymentUpdate` runs.
- Users can also actively query the gateway via `/payments/:id/capture`.
- Fee behaviour is set in `payment_config` (`customer_fee_enabled`, `reuse_legacy_order_fee_payment`).

---

## 6. Settings

- **Storage:** the `settings` table (key → JSON). Keys are defined in `internal/constants/constants.go`.
- **Service:** `modules/settings/application/*`. Each key has a normalizer; saving some keys clears the cached public config or callback routes (`default_registry.go`).
- **Schemas:** `modules/settings/schema/*`.

| Key | Contents |
|---|---|
| `site_config` | `brand{site_name, site_url, site_icon, site_logo, site_description(ml)}`, `contact{telegram, whatsapp}`, `seo{title, keywords, description}` (ml), `legal{terms, privacy}` (ml), `about{hero, introduction, services{title, items}, contact{title, text}}`, `scripts[{name, enabled, position: head/body_end, code}]` (max 20), `footer_links[{name, url}]` (max 20), `currency` (3 letters, default CNY), `template_mode` (card/list), `storefront_template` (classic/vault), `languages` |
| `order_config` | payment_expire_minutes, max_refund_days |
| `smtp_config` | enabled, host, port, username, password, from, from_name, use_tls, use_ssl, order_notification_enabled, verify_code{expire_minutes, send_interval_seconds, max_attempts, length} |
| `captcha_config` | provider, scenes{…}, image{length, width, height, noise_count, show_line, expire_seconds, max_store}, turnstile{site_key, secret_key, verify_url, timeout_ms} |
| `telegram_auth_config` | enabled, bot_username, bot_token, client_secret, oidc_redirect_uri, mini_app_url, login_expire_seconds, replay_ttl_seconds |
| `google_auth_config` | enabled, client_id |
| `dashboard_config` | alert thresholds, ranking limits, accounting.refund_reverses_cost |
| `notification_center_config` | default_locale, channels{email, telegram, feishu}, scenes, templates, dedupe/interval settings, ignored_product_ids |
| `affiliate_config` | enabled, commission_rate, confirm_days, min_withdraw_amount, withdraw_channels |
| `telegram_bot_config` | enabled, default_locale, config_version, basic, welcome, help, menu |
| `telegram_bot_runtime_status` | bot status reported by heartbeat |
| `order_email_template_config` | email templates (see section 4) |
| `nav_config` | builtin{blog, notice, about}, custom_items (max 10) |
| `wallet_config` | wallet_only_payment, top-up channel ids |
| `payment_config` | customer_fee_enabled, reuse_legacy_order_fee_payment |
| `registration_config` | registration_enabled, email_verification_enabled, email_domain_allowlist_enabled, allowed_email_domains |
| `order_risk_control_config` | version, enabled, common.ip_blacklist, guest{…}, member{…} |
| `upstream_sync_config` | interval_minutes, pre_order_stock_check_enabled, sync_page_size, sync_max_pages, sync_conn_concurrency |
| `callback_routes_config` | payment_callback, dujiaopay_webhook, paypal_webhook, stripe_webhook, upstream_callback |
| `home_announcement` | home-page announcement |

**`GET /api/v1/public/config`** (`modules/settings/transport/http/public/handler.go`) is cached in Redis for 60 s per reseller. It returns:
- the whole `site_config` object, over defaults for languages, currency, template, contact and scripts;
- `payment_channels` (public list for orders), `wallet_recharge_channel_ids`, `wallet_only_payment`;
- `captcha` (public part only), `telegram_auth{enabled, bot_username, mini_app_url}`, `google_auth{enabled, client_id}`, `affiliate`;
- `smtp_enabled`, `registration_enabled`, `email_verification_enabled`, `email_domain_allowlist_enabled`, `allowed_email_domains`;
- `nav_config`, `announcement`, `tenant{mode, host}` or the reseller's branding;
- `server_time` (ms) and `app_version`.

---

## 7. `config.yml.example`

- **app**: `secret_key` (AES key source), `totp_issuer`. The three secrets (`app.secret_key`, `jwt.secret`, `user_jwt.secret`) must be different strong values, or the server refuses to start.
- **server**: host `0.0.0.0`, port 8080, mode `debug`/`release`, `trusted_proxies` (defaults 127.0.0.1/32 and ::1/128; any rule that trusts every address is rejected).
- **log**: dir, filename, rotation size, backups, age, compress.
- **database**: `driver: sqlite | postgres`, `dsn: ./db/dujiao.db`, pool (max_open 1 for SQLite).
  - SQLite uses the pure-Go driver with WAL, busy_timeout 5000 and synchronous NORMAL.
- **jwt** (admin): secret, 24 h. **user_jwt**: secret, 24 h, remember-me 168 h.
- **bootstrap**: default_admin_username / password for the first admin.
- **telegram_auth**: enabled, bot_username, bot_token, client_secret, oidc_redirect_uri, mini_app_url, login and replay TTLs.
- **google_auth**: enabled, client_id.
- **redis**: enabled, host, port, password, db 0, prefix `dj`. Used for cache, rate limits and 2FA challenges; rate limits fall back to memory without it.
- **queue**: enabled, Redis host and port, db 1, concurrency 10, queue weights `default: 10, critical: 5`, `upstream_sync_interval: 5m`.
- **upload**: max_size 10 MB, allowed types and extensions (jpeg, png, gif, webp; SVG off), max 4096×4096.
- **cors**: allowed origins, methods and headers, allow_credentials, max_age.
- **security**: login_rate_limit (300 s window, 5 attempts, 900 s block); password_policy (min length 8, upper, lower and digit required).
- **email**: SMTP defaults plus verify_code settings (10 min, 60 s resend interval, 5 attempts, 6 digits).
- **order**: payment_expire_minutes 15, max_refund_days 30.
- **reseller**: enabled, main_hosts, trusted_forwarded_host, subdomain_base, self_apply_enabled, settlement_confirm_days 7.
- **web**: admin_path `/admin` (fullstack build only).

The Go config loader is in `internal/config`, using viper. The server takes `-mode all|api|worker`, plus the `admin …` and `rollback` subcommands (`cmd/server/main.go`).

---

## 8. Background jobs (asynq)

Files: `internal/queue/tasks.go`, `internal/queue/client.go`, `internal/app/jobs/service.go`, `internal/app/jobs/consumer/*`. All jobs use the `default` queue.

| Task | Trigger | What it does |
|---|---|---|
| `order:status_email` | queued on status change | Sends the customer's order email (`{order_id, refund_record_id?, status}`) |
| `order:auto_fulfill` | after payment | Runs auto delivery for a child order |
| `order:timeout_cancel` | delayed by the payment window | Cancels an unpaid order and releases stock, coupon and wallet money |
| `wallet_recharge:timeout_expire` | delayed | Expires an unpaid top-up |
| `notification:dispatch` | queued | Sends admin notifications through the notification center |
| `affiliate:confirm_commissions` | every 1 min | Moves due commissions to `available` |
| `reseller:confirm_ledger` | every 1 min | Confirms due reseller ledger entries |
| `upstream:sync_stock` | every interval (setting, else config, default 5 m) | Syncs supplier stock and prices |
| `procurement:submit` | queued (up to 3 retries) | Places the order with the supplier |
| `procurement:poll_status` | delayed | Checks the supplier order's status |
| `procurement:sync_accepted` | every 30 min | Re-checks accepted purchase orders |
| `downstream:callback` | queued (up to 5 retries) | Notifies the buying shop about its order |
| `reconciliation:run` | admin | Runs a reconciliation job |
| `bot:notify` | queued (up to 3 retries) | Pushes order or top-up events to the Telegram bot |
| `telegram:broadcast` | admin | Sends a broadcast |
| inventory / payment alert check (a `notification:dispatch` task with event `exception_alert_check`) | every 1 min | Checks stock and payment-alert thresholds |

A constant `upstream:sync_products` exists but no handler or schedule is registered for it.

---

## Key source files

- Router: `internal/app/httpserver/router.go`, `routes_storefront.go`, `routes_admin.go`, `routes_channel.go`, `routes_upstream.go`
- Middleware: `internal/app/httpserver/middleware/{middleware,channel_auth,upstream_auth,reseller_middleware,compliance_middleware,callback_route_middleware,rate_limit}.go`
- Migrations: `internal/bootstrap/database/migrations/{registry,migrations}.go`; DB setup `internal/platform/database/gormdb/db.go`
- Constants: `internal/constants/constants.go`; permissions `internal/authz/{service,bootstrap}.go`
- Order: `internal/modules/order/application/{order_service,order_service_validate,order_service_child,order_status}.go`
- Payment: `internal/modules/payment/application/payment_service_{create,callback,callback_dispatch,provider}.go`; gateways under `infrastructure/gateway/*`
- Fulfillment: `internal/modules/fulfillment/application/service.go`
- Signing: `internal/upstream/signer.go`; encryption `internal/crypto/aes.go`; money `internal/shared/money/amount.go`; order numbers `internal/shared/serial`
- Jobs: `internal/app/jobs/service.go`, `internal/queue/{tasks,client}.go`
- Public config: `internal/modules/settings/transport/http/public/handler.go`; site settings `internal/modules/settings/application/site_normalize.go`
