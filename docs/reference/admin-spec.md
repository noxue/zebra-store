I covered all 62 views, the 17 dialog/tab components, the router, layout, `api/admin.ts`, `api/types.ts`, the stores and the composables. Paths are relative to the `.../dujiao-next/frontend/admin/src` folder unless marked `(root)`. Column, field and filter names are the i18n key suffixes or `v-model` names from the source. The i18n key names are English and self-describing; the Chinese text for each key is in `i18n/index.ts`.

---

## 1. Build, dev port, base path

**package.json (root).** Vue 3.5, vue-router 4.6, pinia 3, vue-i18n 9 (`legacy:false`), reka-ui 2.9 (the base for shadcn-vue), @vueuse/core 14, lucide-vue-next, @tanstack/vue-table (listed as a dependency), class-variance-authority, clsx, tailwind-merge.
- **Rich text:** the full @tiptap 2.27 suite (starter-kit, image, link, placeholder, text-align, color, text-style, table/row/cell/header).
- **Other runtime deps:** `qrcode` (for the 2FA QR image), `dompurify` and `marked` (for release notes).
- **Dev deps:** vite 7, tailwindcss 4 with @tailwindcss/postcss, tw-animate-css, vitest with jsdom, vue-tsc, TypeScript 5.9.
- **Scripts:**
  - `dev` = `vite --port 5174`
  - `build` = `vue-tsc -b && vite build`
  - `build:fullstack` = the same with `VITE_FULLSTACK=1`
  - `test` = `vitest run`

**vite.config.ts (root)**
- Dev server: host `0.0.0.0`, port **5174**, `strictPort`. It proxies `/api` and `/uploads` to `http://localhost:8080`.
- Alias `@` points to `src`. `manualChunks` splits vendor-vue, vendor-ui and vendor-tiptap.
- A plugin adds `data-cfasync="false"` to module scripts (for Cloudflare Rocket Loader).
- In fullstack mode, `base: './'` and the `adminBaseInjector` plugin inserts `<base href="__DJ_ADMIN_BASE__/">` right after `<head>`. The Go backend (`internal/web/handler.go`) replaces that placeholder with the configured `web.admin_path` at startup.

**How the admin base path is resolved at runtime (`utils/adminBase.ts`)**
- `ADMIN_BASE` is read from `document.querySelector('base').href`, falling back to `import.meta.env.BASE_URL`, then `/`.
- If the value still contains `__DJ_ADMIN_BASE__`, or equals `.`, it becomes `''`. Trailing slashes are stripped.
- `adminUrl(path)` must be used for raw `<a href>` and `window.location` (logout, 401 redirect, user-detail links).
- The router uses `createWebHistory(ADMIN_BASE || '/')`. Do not add the prefix to `router.push` or `<RouterLink>`, or you get a double prefix.

**API client (`api/client.ts`)**
- Uses `fetch`, base `${VITE_API_BASE_URL||''}/api/v1`, 10 s timeout via AbortController.
- Headers: `X-Lang: <locale>` and `Authorization: Bearer <admin_token>`. JSON body unless the body is FormData.
- Response envelope: `{status_code, msg, data, pagination?{page,page_size,total,total_page}}`. Any `status_code !== 0` is a business error and triggers a toast.
- A 401 (HTTP status or `status_code`) on anything except `/admin/login` clears the token and redirects to `adminUrl('/login')`.
- The messages `compliance_required` and `compliance_required_by_super_admin` are rejected silently (no toast).
- The `blob:true` option returns `{data: Blob, headers}` and parses JSON error bodies.
- `api.get/post/put/patch/delete`. DELETE can carry a body through `options.data`.

---

## 2. Routes and sidebar

**Router (`router/index.ts`).** `/login` sits outside the layout. `/` is `AdminLayout` with `meta.requiresAuth`. Every child route has `meta.permission`, which is checked by `authStore.hasPermission`. A failed check redirects to `/forbidden?from=…`.

**Guard order**
1. No token: go to `/login`.
2. Token present and the target is `/login`: go to `/`.
3. Permissions not loaded yet: call `loadAuthz()`. On failure, log out and go to `/login`.
4. Check the route permission.
5. Compliance gate. For these routes: `payments, payment-channels, wallet-config, wallet-recharges, reconciliation, affiliates-withdraws, affiliates-commissions, resellers-operations, resellers-ledger-entries, resellers-balance-accounts, resellers-withdraws`:
   - fetch compliance status;
   - a non-super admin who has not acknowledged is sent to `compliance-required`;
   - a super admin is let through, and the page's `ComplianceGuardWrapper` shows a blocking dialog instead.

**Sidebar (`layouts/AdminLayout.vue`).** Menu items are filtered by the same permission. Group labels below are the zh-CN i18n values.

| Group (id) | Item → path → view | Permission |
|---|---|---|
| (top) 仪表盘 | `/` → Dashboard.vue | none |
| 商品管理 (products) | 商品分类 `/categories` → Categories | GET:/admin/categories |
| | 商品列表 `/products` → Products | GET:/admin/products |
| | 卡密库存 `/card-secrets` → CardSecrets | GET:/admin/card-secrets |
| | 卡密导入 `/card-secret-imports` → CardSecretImports | GET:/admin/card-secrets |
| | 卡密导出 `/card-secret-exports` → CardSecretExports | GET:/admin/card-secrets |
| 订单管理 | 订单列表 `/orders` → Orders | GET:/admin/orders |
| | 订单风控 `/order-risk-control` → OrderRiskControl | GET:/admin/settings |
| | 退款记录 `/order-refunds` → OrderRefunds | GET:/admin/order-refunds |
| 支付管理 | 支付渠道 `/payment-channels` → PaymentChannels | GET:/admin/payment-channels |
| | 支付记录 `/payments` → Payments | GET:/admin/payments |
| | 回调路由 `/callback-routes` → CallbackRoutes | GET:/admin/settings |
| 用户管理 | 用户列表 `/users` → Users | GET:/admin/users |
| | 钱包管理 `/wallet-recharges` → WalletRecharges | GET:/admin/wallet/recharges |
| | 钱包配置 `/wallet-config` → Wallet | GET:/admin/settings |
| | 登录日志 `/user-login-logs` → UserLoginLogs | GET:/admin/user-login-logs |
| | 会员等级 `/member-levels` → MemberLevels | GET:/admin/member-levels |
| 文章管理 | 文章列表 `/posts/blog` → Posts | GET:/admin/posts |
| | 文章分类 `/posts/categories` → PostCategories | GET:/admin/post-categories |
| 内容管理 | Banner管理 `/banners` → Banners | GET:/admin/banners |
| | 素材管理 `/media` → Media | GET:/admin/media |
| | 公告列表 `/posts/notice` → Posts (type=notice) | GET:/admin/posts |
| 营销管理 | 优惠券 `/coupons`; 活动价 `/promotions`; 批发价 `/wholesale-prices` (perm GET:/admin/products); 礼品卡 `/gift-cards` | coupons / promotions / products / gift-cards |
| 推广返利 | 返利设置 `/affiliates/settings` (GET:/admin/settings/affiliate); 返利用户 `/affiliates/users`; 佣金记录 `/affiliates/commissions`; 提现审核 `/affiliates/withdraws` | GET:/admin/affiliates/{users,commissions,withdraws} |
| 分销商管理 | 分销概览 `/resellers/operations` (…/operations/overview); 分销商审核 `/resellers/profiles`; 分销域名 `/resellers/domains`; 分销站点配置 `/resellers/site-configs`; 分销商品配置 `/resellers/product-settings`; 分销流水 `/resellers/ledger-entries`; 分销余额 `/resellers/balance-accounts`; 分销提现 `/resellers/withdraws` | GET:/admin/resellers/… (same path) |
| 对接管理 | 连接管理 `/site-connections`; 商品映射 `/product-mappings`; 采购单管理 `/procurement-orders`; 对账中心 `/reconciliation` (GET:/admin/reconciliation/jobs); API 凭证 `/api-credentials` | GET:/admin/… (same path) |
| Telegram Bot | 概览 `/telegram-bot`; 基础设置 `/telegram-bot/settings`; 帮助中心 `/telegram-bot/help-center`; 菜单配置 `/telegram-bot/menu`; 连接状态 `/telegram-bot/status` (all GET:/admin/settings/telegram-bot); Bot 客户端 `/telegram-bot/channel-clients` (GET:/admin/channel-clients); 消息群发 `/telegram-bot/broadcasts` (GET:/admin/telegram-bot/broadcasts) | |
| 系统设置 | 站点设置 `/settings` (GET:/admin/settings); 通知中心 `/settings/notifications` (…/settings/notification-center); 权限管理 `/authz` (GET:/admin/authz/roles); 权限审计 `/authz-audit-logs`; 安全设置 `/security` (none) | |

**Routes not in the menu:**
- `users/:id` → UserDetail
- `resellers/profiles/:id` → ResellerProfileDetail
- `telegram-bot/broadcasts/create` → TelegramBotBroadcastCreate
- `telegram-bot/broadcasts/:id` → TelegramBotBroadcastDetail
- `forbidden` → Forbidden
- `compliance-required` → ComplianceRequired
- `posts` redirects to `/posts/blog`

**Layout behaviour**
- Collapsible sidebar: `w-64` expanded, `w-16` icons-only. It auto-collapses below 1280 px unless the user toggled it; the choice is stored in LS `admin_sidebar_collapsed`.
- Group expand state is stored in LS `admin_nav_group_expanded`.
- A nav search box filters both group names and item labels.
- On mobile the sidebar becomes a Sheet.
- Header controls:
  - locale Select (zh-CN / zh-TW / en-US, LS `admin_locale`);
  - system-update button (opens SystemUpdateDialog);
  - dark/light toggle (LS `admin_theme`, `.dark` class on `<html>`, defaults to prefers-color-scheme);
  - storefront Home link (`public/config` → `brand.site_url`);
  - Logout.
- The footer shows © Dujiao-Next with `app_version` and a GitHub link. The favicon is set from `brand.site_icon`.

---

## 3. Views (spec)

Conventions shared by list pages:
- Pagination via `ListPagination` with page sizes 10/20/50/100 and jump-to-page.
- Filter selects use `__all__` to mean "no filter".
- Datetime filters are sent as RFC3339 (`created_from`/`created_to`).
- A refresh button uses `useListRefresh`, which shows a toast.
- Deletes go through the `confirmAction` store dialog.
- Localized fields have a language tab switcher (zh-CN/zh-TW/en-US) and are stored as `{lang: text}`.

### Dashboard (`views/Dashboard.vue`)
- **Filters:** range `today|7d|30d|custom` (custom adds from/to date inputs), plus a "refresh now" button that sends `force_refresh=true`. Query params: `range, tz, from, to`.
- **KPI cards:**
  - orders_total (sub: paid_orders)
  - gmv_paid (sub: payment_success_rate)
  - total_cost (sub: payment_fee)
  - total_profit
  - profit_margin
  - pending orders (sub: processing_orders)
  - new_users (sub: active_products)
  - total_user_balance
  - low_stock_products (sub: out_of_stock products, SKUs, low_stock_skus)
  - auto_available_secrets (sub: manual_available_units)
  - payments_success (sub: payments_failed)
  - period card
- **Charts** are hand-drawn CSS bar charts, no chart library:
  1. Order trend: orders_total vs orders_paid per day.
  2. Payment trend: payments_success vs payments_failed.
- **Funnel:** orders_created → payments_created → payments_success → orders_paid → orders_completed, plus payment_conversion_rate and completion_rate.
- **Rankings:**
  - Top products: title, SKU label, paid_orders, quantity, paid_amount, profit.
  - Top channels: success/failed count, success_amount, success_rate.
- **Alerts:** `overview.alerts` (type/level/value) and SKU-level inventory alerts from `dashboard/inventory-alerts`. Each alert links to `/products?product_id=`.
- **Quick actions:** links to orders, payments, products, card-secrets, users.
- **DashboardAd** (`components/admin/DashboardAd.vue`), slot `dashboard_sponsored`, uses `ads/render` and `ads/impression`.
- **APIs:** `getDashboardOverview`, `getDashboardTrends`, `getDashboardRankings`, `getDashboardInventoryAlerts`. Response shapes are defined inline at the top of Dashboard.vue: `DashboardOverview.kpi`, `funnel`, `alerts`, trend `points[]` (date, orders_total, orders_paid, payments_success, payments_failed, gmv_paid, profit), `rankings.top_products` / `top_channels`.

### Products (`views/admin/Products.vue` and `components/ProductEditModal.vue`)
- **Filters:** search; stock_status (`all|low|normal|unlimited`); category; is_active (`all|active|inactive`); wholesale (`all|enabled|disabled`, synced to `?wholesale=`); reset button.
- **Query:** `page, page_size, search, stock_status, is_active, wholesale, category_id`. The page also reads `getSettings(site_config)` for the currency.
- **Columns:** checkbox, id, name, price, category (inline select edit), sort (inline number edit), status (click to toggle), actions (edit, delete).
- **Batch actions:** activate, deactivate, move to category, delete. Endpoints: `batch-status`, `batch-category`, `batch-delete`; each returns `success_count`.
- **Inline edits:** `patchProduct {is_active | sort_order | category_id}`.
- **Deep links:** `?action=create` and `?product_id=X` open the modal.
- **Edit modal fields:**
  - Basics: title[lang], slug, seo_meta.keywords[lang], seo_meta.description[lang], category.
  - purchase_type `member|guest`; min/max_purchase_quantity.
  - stock_display_mode `exact|status|range|hidden`.
  - fulfillment_type `manual|auto`. This field and several others are locked when the product `is_mapped` (upstream).
  - manual_stock_total.
  - **manual_form_schema** builder (list of fields): key, type (`text|textarea|phone|email|number|select|radio|checkbox`), required, label[lang], placeholder[lang], regex, max_len, min, max, options_text.
  - **SKUs** (list): sku_code, spec_values[lang], price_amount, cost_price_amount, manual_stock_total (−1 means unlimited), sort_order, is_active.
  - price_amount and cost_price_amount (hidden or hinted when SKUs exist), sort_order.
  - images (MediaPicker, multiple, scene `product`), description[lang] (textarea), content[lang] (RichEditor), instructions[lang] (RichEditor), tags (chips).
  - payment_channel_ids (chip multi-select from `getPaymentChannels`), is_affiliate_enabled, is_active.
- **APIs:** `getProduct`, `createProduct`, `updateProduct`.

### Categories (`Categories.vue`)
- A tree list (parent/child), fetched with `getCategories()`.
- **Columns:** id, icon, name, slug, sort, status (toggle via `patchCategoryActive`), actions (edit, delete).
- **Dialog fields:** name[lang], slug, parent_id (0 = root; locked when the category has children), icon (MediaPicker, scene `category`), sort_order. Supports `?category_id=` deep link.

### WholesalePrices (`WholesalePrices.vue`)
- Product list with a search box and a wholesale filter (all/enabled/disabled).
- **Columns:** id, product, price, tiers, status, action.
- **Configure modal:** a list of tiers {sku_id (select when the product has SKUs), min_quantity, unit_price}, with add/remove. Buttons: "clear wholesale" and save.
- **API:** `PATCH products/:id/wholesale-prices {wholesale_prices:[{sku_id?,min_quantity,unit_price}]}`.

### CardSecrets (`CardSecrets.vue`, `components/CardSecretEditModal.vue`)
- **Product picker:** searchable, auto-fulfilled products only (`getProducts{fulfillment_type:'auto', page_size:100, search}`), then a SKU picker (`getProduct` for SKUs).
- **Stats:** `card-secrets/stats{product_id, sku_id}` returns available/reserved/used/total.
- **Batches panel:** `card-secrets/batches{product_id, sku_id, page}`. Each row shows batch_no, source, note and counts; clicking a row filters the list by that batch.
- **List filters:** status (available/reserved/used), secret, batchNo, batch_id.
- **Columns:** id, secret, product, sku, status, orderId, batchId, createdAt, action (edit).
- **Operation scope:** `selected` (ids) or `filtered` (the current filter). Actions: batch set-status (available/reserved/used), batch delete, export (txt or csv, returns a blob).
- **Edit modal:** secret, status → `updateCardSecret`.
- A "guide" card explains usage.

### CardSecretImports (`CardSecretImports.vue`, `components/CardSecretBatchCreateModal.vue`)
- Pick product and SKU (same picker as above); the panel then shows ready or empty states.
- **Two forms:**
  1. **Batch paste:** a secrets textarea (one per line), batch_no, note, deduplicate → `POST card-secrets/batch {product_id, sku_id?, secrets[], batch_no, note, deduplicate}`.
  2. **File import (CSV/TXT):** FormData with `product_id, sku_id, batch_no, note, deduplicate, file` → `POST card-secrets/import`.

### CardSecretExports (`CardSecretExports.vue`)
- Pick product, SKU and optionally a batch. Loads the available count and batch list.
- **Form:** exportCount (limit), format (txt/csv), delete_after_export (in the payload).
- Flow: confirm dialog → `POST card-secrets/export-available` (blob) → result dialog with copy-content and download buttons.

### GiftCards (`GiftCards.vue`)
- **Filters:** code, status (`active|expired|redeemed|disabled`), createdFrom/To. `redeemedUserID` exists in the form.
- **Columns:** checkbox, id, name, code, amount, status, batchNo, redeemedUser, redeemedAt, expiresAt, createdAt, action.
- **Generate modal:** name, quantity, amount, expiresAt → `gift-cards/generate`.
- **Edit modal:** name, status (active/disabled), expiresAt → `PUT gift-cards/:id`.
- Delete. Batch set status active/disabled → `PATCH gift-cards/batch-status {ids, status}`. Export selected as txt/csv → `POST gift-cards/export {ids, format}` (blob).

### Orders (`Orders.vue`, `components/OrderDetailDialog.vue`, `components/OrderFulfillmentModal.vue`, `components/OrderRefundsDialog.vue`)
- **Filters:** status; userId; userKeyword; orderNo; guestEmail; productKeyword; createdFrom/To; sortBy (`created_at_desc/asc, updated_at_desc/asc, total_amount_desc/asc`, parsed into sort params).
- **Order statuses:** `pending_payment, paid, fulfilling, partially_delivered, partially_refunded, delivered, completed, canceled, refunded`.
- The page reads `getSettings(order_config)`.
- **Columns:** id, orderNo (copy button), items, user, ip, amount, status, createdAt, updatedAt, action.
- **Row actions:**
  - Status select plus update → `PATCH orders/:id {status}`. Not allowed when the order is completed, canceled, partially_refunded or refunded.
  - "mark completed" (only when delivered).
  - View (detail dialog).
  - Fulfill: only when there is no fulfillment yet, all items are manual, and status is paid or fulfilling.
- **Detail dialog** (`getOrder`):
  - Sections: amounts, discounts (coupon/promotion/member/wholesale), items, child orders with their items and fulfillment, fulfillment (download via `orders/:id/fulfillment/download` blob), procurement (`getProcurementOrders{local order}`), payments table (id, channel, status, feeRate, amount, createdAt).
  - Refund card with two tabs:
    - **wallet:** amount, remark → `orders/:id/refund-to-wallet`;
    - **manual:** amount, reason, paymentFeeRefunded → `orders/:id/manual-refund {amount, remark, payment_fee_refunded}`.
- **Fulfillment modal:** a list of delivery key/value entries plus a note → `POST fulfillments` (payload: order_id, payload/delivery_data).

### OrderRefunds (`OrderRefunds.vue`)
- **Filters:** userId, userKeyword, orderNo, guestEmail, productKeyword, created range.
- **Columns:** id, orderId, productInfo, user, refundType, amount, paymentFeeRefund, createdAt, action (detail).
- **Detail dialog** (`getOrderRefund`) has a toggle for payment_fee_refunded → `PATCH order-refunds/:id/payment-fee`.

### OrderRiskControl (`OrderRiskControl.vue` wraps `components/SettingsOrderRiskControlTab.vue`)
Settings key `order_risk_control_config`:
- `enabled`
- `common.ip_blacklist` (textarea, one per line)
- `guest{enabled, max_pending_orders_per_ip, max_pending_quantity_per_ip_product, max_quantity_per_product_per_order, payment_expire_minutes, rate_limit{enabled, window_seconds, max_requests, block_seconds}}`
- `member{enabled, max_pending_orders_per_user, max_pending_orders_per_ip, max_quantity_per_product_per_order, rate_limit{…}}`
- An "apply recommended guest policy" button.

### Payments (`Payments.vue`, compliance-guarded)
- **Filters:** status (`initiated|pending|success|failed|expired`), userId, orderId, channelId (select from channels), providerType, channelType, created range.
- **Columns:** paymentId, orderId, channel, status, amount, feeRate, feeAmount, createdAt, action (detail).
- **Detail** (`getPayment`) shows the AdminPayment fields: provider_trade_no, pay_url, qr_code, provider_payload, fee_policy, superseded_*.
- **Export:** `GET payments/export` with the current filters (blob).

### PaymentChannels (`PaymentChannels.vue`, `components/PaymentChannelModal.vue`, compliance-guarded)
- **Fee config card** (settings key `payment_config`): customer_fee_enabled, reuse_legacy_order_fee_payment.
- **Filters:** providerType (`official, dujiaopay, epay, bepusdt, epusdt, okpay, tokenpay`); channelType (`wechat, alipay, qqpay, paypal, stripe, usdt, usdt-trc20, usdc-trc20, trx, tron-usdt, tron-trx, ethereum-usdt, ethereum-usdc, base-usdc, solana-usdt, aptos-usdt`).
- **Columns:** id, name, type, interaction, feeRate, status, sort, action (edit, delete).
- **Modal, common fields:** name, icon (MediaPicker), provider_type, channel_type, interaction_mode, fee_rate, fixed_fee, min_amount, max_amount, hide_amount_out_range, payment_types (`order|wallet`), payment_roles (`guest|member`), member_levels (MultiSelect), is_active, sort_order, and advanced raw `config_json`.
- **Channel types by provider:**
  - epay: wechat, alipay, qqpay
  - official: paypal, stripe, alipay, wechat
  - bepusdt: usdt-trc20, usdc-trc20, trx
  - okpay: usdt, trx
  - dujiaopay: token_id typed in by hand
- **Interaction modes** (qr, redirect, wap, page) depend on the provider and its order_mode (cashier means redirect only). Official alipay supports qr, wap and page.
- **Per-provider config forms:**
  - **epay:** epay_version v1/v2, gateway_url, merchant_id, merchant_key, private_key, platform_public_key, notify_url, return_url, exchange_rate, target_currency.
  - **bepusdt:** gateway_url, auth_token, trade_type, fiat, currencies, order_mode (transaction/cashier), notify_url, return_url.
  - **epusdt:** gateway_url, token, pid, secret_key, network, currency, order_mode, notify_url, return_url.
  - **okpay:** gateway_url, merchant_id, merchant_token, display_name, exchange_rate, callback_url, return_url.
  - **tokenpay:** gateway_url, notify_secret, currency, base_currency, notify_url, redirect_url.
  - **dujiaopay:** api_base_url, api_key_id, api_secret, webhook_secret, order_mode, allowed_methods, fiat_currency, success_url, cancel_url.
  - **paypal:** base_url, client_id, client_secret, webhook_id, brand_name, locale, return_url, cancel_url, exchange_rate, target_currency.
  - **stripe:** api_base_url, secret_key, publishable_key, webhook_secret, payment_method_types, success_url, cancel_url, exchange_rate, target_currency.
  - **alipay:** app_id, gateway_url, private_key, alipay_public_key, app_cert_sn, alipay_root_cert_sn, sign_type, notify_url, return_url, exchange_rate, target_currency.
  - **wechat:** appid, mchid, merchant_serial_no, merchant_private_key, api_v3_key, verification_mode (`platform_certificate|wechatpay_public_key|combined`), wechatpay_public_key, wechatpay_public_key_id, notify_url, h5_type, h5_wap_name, h5_wap_url, h5_redirect_url, exchange_rate, target_currency. Has a "test public key" button → `POST payment-channels/:id/wechatpay-public-key-test`, which returns `AdminGatewaySecurityTestResult`.

### CallbackRoutes (`CallbackRoutes.vue`)
- Settings key `callback_routes_config`: payment_callback, dujiaopay_webhook, paypal_webhook, stripe_webhook, upstream_callback. Helpers are in `utils/callbackRoutes.ts`.
- **Validation:** each path must start with `/api/`, must not collide with the reserved prefixes (`/api/v1/public|admin|auth|guest|channel|upstream/api|user/`), and must be unique.

### Users (`Users.vue`)
- **Filters:** userId, keyword, status (active/disabled), created range, last-login range, reset button.
- **Sortable:** created_at, last_login_at, wallet_balance (`sort_by`, `sort_order`).
- **Columns:** checkbox, id, email, nickname, status, locale, memberLevel, adminNote, action (edit, and a link to the detail page).
- **Batch:** set active or disabled → `PUT users/batch-status {user_ids, status}`.
- **Edit modal:** email, nickname, password, status, locale (zh-CN/zh-TW/en-US), email_verified (verified/unverified), admin_note → `PUT users/:id`.

### UserDetail (`UserDetail.vue`)
- **Profile card:** OAuth identities, each with unbind (Telegram → `DELETE users/:id/oauth/telegram`, Google → `…/oauth/google`); reset user 2FA → `DELETE users/:id/2fa`; member level select → `PUT users/:id/member-level {member_level_id}`.
- **Wallet:** `users/:id/wallet`. Adjust form {amount, operation add|subtract, remark} → `POST users/:id/wallet/adjust`.
- **Tabs:**
  - orders (`getOrders{user_id}`): id, orderNo, status, amount, createdAt
  - payments (`getPayments{user_id}`): id, orderId, status, amount, createdAt
  - coupons (`users/:id/coupon-usages`): id, coupon, type, products, orderId, discount, createdAt
  - wallet transactions (`users/:id/wallet/transactions`): id, type, direction, amount, balanceAfter, remark, createdAt

### UserLoginLogs (`UserLoginLogs.vue`)
- **Filters:** userId, email, clientIp, status (success/failed), failReason (`bad_request, captcha_required, captcha_invalid, captcha_config_invalid, captcha_verify_failed, invalid_email, invalid_credentials, email_not_verified, user_disabled, internal_error`), created range.
- **Columns:** id, user, status, failReason, clientIp, loginSource, createdAt.

### WalletRecharges (`WalletRecharges.vue`, compliance-guarded)
- **Filters:** rechargeNo, userId, userKeyword, paymentId, channelId, providerType (`official, epay, epusdt, tokenpay, wallet`), status (`pending, success, failed, expired`), created range, paid range.
- **Columns:** id, rechargeNo, user, payment, channel, status, amount, paidAt, createdAt.

### Wallet config (`Wallet.vue`, compliance-guarded)
- Settings key `wallet_config`: `recharge_channel_ids[]` (checkbox list of active channels) and `wallet_only_payment` (switch).

### MemberLevels (`MemberLevels.vue`)
- **Columns:** id, icon, name, slug, discountRate, rechargeThreshold, spendThreshold, sortOrder, isActive, createdAt, action.
- **Modal:** name[lang], slug, icon (emoji or image mode), discount_rate, recharge_threshold, spend_threshold, is_default, sort_order, is_active.
- **Backfill button** → `POST member-levels/backfill`.
- The `member-level-prices` APIs exist in `api/admin.ts` but no view calls them.

### Coupons (`Coupons.vue`)
- **Filters:** code, scopeRefId (product search picker), isActive. `id` is sent in the query too.
- **Columns:** id, code, type, value, scope, limits, period, status, action.
- **Modal:** code, type (percent/fixed), value, scope_ref_ids (multi-select products with search, select-all, clear), min_amount, max_discount, usage_limit, per_user_limit, disabled_wholesale_price, per_item_discount (fixed type only), payment_roles, member_levels, starts_at, ends_at, is_active.

### Promotions (`Promotions.vue`)
- **Filters:** name, scope product, isActive.
- **Columns:** id, name, type, value, scope, minAmount, period, status, action.
- **Modal:** name, scope_ref_id (single product), type (percent/fixed), value, min_amount, starts_at, ends_at, is_active.

### Posts (`Posts.vue`; the route param `:type` is `blog` or `notice`)
- **Columns:** id, category, title, slug, status, createdAt, action.
- **Modal:** type, title[lang], slug, summary[lang], content[lang] (RichEditor), thumbnail (MediaPicker), is_published.
- Blog posts also have category_id (from `getPostCategories`) and related products (search `getProducts`; add/remove; `product_ids[]`). Existing relations load from `posts/:id/products`.

### PostCategories (`PostCategories.vue`)
- Tree list. **Columns:** id, icon, name, slug, sort, status (toggle → `PATCH post-categories/:id/status`), action.
- **Modal:** name[lang], slug, parent, icon, sort_order.

### Banners (`Banners.vue`)
- **Filters:** search, position, isActive.
- **Columns:** id, image, name, position, linkType, sort, status, action.
- **Modal:** name, position (`home_hero`), title[lang], subtitle[lang], image, mobile_image, link_type (`none|internal|external`), link_value, open_in_new_tab, start_at, end_at, sort_order, is_active.

### Media (`Media.vue`)
- A grid of assets, not a table.
- **Filters:** search, scene (`product, banner, category, post, editor, common, telegram, upstream`).
- **Actions:** inline rename → `PUT media/:id {name}`; delete; batch mode with select-all and batch delete → `media/batch-delete`; upload → `POST /admin/upload` FormData(file, scene).
- The response is `data.{items,total}` (not the standard pagination envelope).
- `components/admin/MediaPicker.vue` is reused everywhere. Props: v-model (string or string[]), `multiple`, `scene`, `dialogOnly`. It has a library tab and upload with a size check.

### Affiliate
- **AffiliateSettings:** enabled, commission_rate, confirm_days, min_withdraw_amount, withdraw_channels (textarea, one per line) → `PUT settings/affiliate`.
- **AffiliateUsers:**
  - Filters: keyword, code, status (active/disabled).
  - Columns: checkbox, id, user, code, clicks, validOrders, conversionRate, pending, available, withdrawn, status, createdAt, action.
  - Actions: toggle one → `PATCH affiliates/users/:id/status`; batch → `PATCH affiliates/users/batch-status {profile_ids, status}`.
- **AffiliateCommissions** (compliance-guarded):
  - Filters: affiliateProfileId, keyword, orderNo, status (`pending_confirm, available, rejected, withdrawn`).
  - Columns: id, user, orderNo, baseAmount, rate, commission, status, confirmAt, availableAt, createdAt.
- **AffiliateWithdraws** (compliance-guarded):
  - Filters: affiliateProfileId, keyword, status (`pending_review, rejected, paid`).
  - Columns: id, user, amount, channel, account, status, rejectReason, processedBy, createdAt, action.
  - Actions: pay → `POST …/:id/pay`; reject (reason prompt) → `…/:id/reject {reason}`.

### Reseller (distribution)
- **ResellerOperationsDashboard** (compliance-guarded):
  - Range `today|7d|30d|custom`.
  - Lifecycle stat cards and order stat cards (fields in `AdminResellerOperationsOverview`), plus alerts.
  - Top resellers table: reseller, ordersTotal, paidOrders, activeDomains, siteConfigured, lastOrderAt.
  - Finance tables from `operations/finance`:
    - period rows: currency, ordersTotal, paidOrders, gmvPaid, profitEarned, refundDeducted, withdrawPaid;
    - current rows: currency, available, locked, negative, pendingWithdraw, abnormalAccounts.
- **ResellerProfiles:**
  - Filters: keyword, userId, status (`pending_review, active, rejected, disabled`), settlementStatus (`normal, frozen`), created range.
  - Columns: id, user, status, settlement, defaultMarkup, maxMarkup, applyReason, rejectReason, reviewedAt, createdAt, action.
  - Actions:
    - approve dialog {default_markup_percent, max_markup_percent};
    - reject or disable dialog {reason};
    - restore;
    - edit dialog {default/max markup, settlement_status, reason};
    - link to detail.
- **ResellerProfileDetail:**
  - Profile summary and edit dialog.
  - System subdomain assignment → `PUT profiles/:id/system-domain {subdomain}`.
  - Domains table: domain, type, verification, status, primary, verifiedAt, action (set primary → `domains/:id/set-primary`).
  - Product summary.
  - Finance tabs:
    - orders: orderNo, status, domain, amount, profit, createdAt;
    - ledger: type, amount, status, time;
    - withdraws: channel, amount, status, time;
    - balances.
- **ResellerDomains:**
  - Filters: keyword, domain, resellerId, userId, type (`subdomain|custom`), status (`pending_review|active|disabled`), verificationStatus (`pending|verified|failed`), created range.
  - Columns: id, reseller, domain, type, verificationStatus, status, primary, verifiedAt, createdAt, action (approve, disable).
- **ResellerSiteConfigs:**
  - Filters: keyword, resellerId.
  - Columns: id, reseller, siteName, logo, favicon, updatedAt, action (edit, reset → `site-configs/:rid/reset`).
  - Editor (tabbed, localized) fields:
    - site_name, logo, favicon;
    - announcement {enabled, type info|success|warning, title[lang], content[lang]};
    - support {telegram, whatsapp, email, support_url};
    - seo {title, keywords, description [lang], default_og_image};
    - footer_links [{name[lang], url}];
    - nav_config.builtin[key] (booleans).
- **ResellerProductSettings:**
  - Filters: keyword, resellerId, userId, productId, listed (listed/hidden), pricingMode (`inherit|markup_percent|fixed_markup|fixed_price`).
  - Columns: id, owner, product, sku, listed, pricingMode, pricingValue, updatedAt, actions (edit, reset → `DELETE product-settings/:rid/:pid?sku_id=`).
  - Editor: loads `GET product-settings/:rid/:pid` (the AdminResellerProductSettingDetail type). Fields: is_listed, pricing_mode, markup_percent, fixed_markup_amount, fixed_price_amount, all per product and per SKU. Preview → `POST …/preview` (returns items with effective_price_amount, valid, error_code). Save → `PUT {settings:[…]}`.
- **ResellerLedgerEntries** (compliance-guarded):
  - Filters: keyword, resellerId, userId, orderId, orderNo, type (`order_profit, refund_deduct, manual_adjust, withdraw_lock, withdraw_paid`), status (`pending_confirm, available, locked, withdrawn, canceled`), created range.
  - Columns: id, reseller, orderNo, type, amount, currency, status, availableAt, withdrawRequest, createdAt.
- **ResellerBalanceAccounts** (compliance-guarded):
  - Filters: keyword, resellerId, userId, status (`normal, negative_balance, frozen_review, disabled`).
  - Columns: id, reseller, currency, available, locked, negative, status, lastLedger, updatedAt.
- **ResellerWithdraws** (compliance-guarded):
  - Filters: keyword, resellerId, userId, status (`pending, rejected, paid`), created range.
  - Columns: id, reseller, amount, currency, channel, account, status, rejectReason, processedBy, createdAt, action (pay; reject dialog with reason).

### Integration: upstream, procurement, reconciliation
- **SiteConnections:**
  - Columns: id, name, baseUrl, protocol, markup, status, lastPing, actions (edit, delete, ping, toggle status → `PUT …/status {is_active}`, reapply markup).
  - Modal: name, base_url, api_key, api_secret, protocol (`dujiao-next`), callback_url, retry_max, retry_intervals (comma-separated, sent as a JSON string), exchange_rate, price_markup_percent, price_rounding_mode (`none|ceil_int|ceil_tenth`), auto_sync_price (true/false).
- **ProductMappings:**
  - Filters: connection, product_status (active/inactive), upstream_status (inactive/deleted), search.
  - List: rows expand to show SKU comparison (skuCode, spec, localPrice, upstreamPrice, costPrice, priceDiff, upstreamStock, upstreamActive). Other columns: connection, lastSynced.
  - Row actions: sync, enable/disable, delete. Batch actions: sync, enable/disable, delete.
  - **Import modal:** choose a connection, then either a flat list view or a by-category view.
    - Data: `upstream-products` (paged, load more) and `upstream-categories`.
    - Selection: select products (SKUs expandable) and a local category → `product-mappings/batch-import {connection_id, upstream_product_ids, category_id}`. It falls back to a single `import` call per product.
    - Whole-category import: `batch-import-by-category {connection_id, upstream_category_id, auto_create_category, local_category_id}`.
- **ProcurementOrders:**
  - Stat cards: total, pending, fulfilled, failed, rejected, other. Clicking one sets the status filter.
  - Filters: connection, status (`pending, accepted, rejected, failed, partially_refunded, fulfilled, refunded, canceled`), order_no, upstream_order_no, date range.
  - Columns: localOrderNo, parentOrderNo, upstreamOrderNo, connection, status, upstreamAmount, localCost, localSellAmount, retryCount, errorMessage, createdAt.
  - Row actions: retry, cancel.
  - Detail drawer: order info, financials (profit, local and upstream refunded), trace id, next retry, timeline, upstream refund records table, and upstream payload download (blob).
- **Reconciliation** (compliance-guarded):
  - Filters: connection, status (`pending, running, completed, failed`), type (`status, amount, full`).
  - Columns: id, connection, type, status, timeRange, total, matched, mismatched, createdAt, actions.
  - New-job dialog {connection_id, type, time_range_start, time_range_end} → `POST reconciliation/run`.
  - Detail: `GET jobs/:id?page=` returns items (localOrderNo, upstreamOrderNo, localStatus, upstreamStatus, mismatchType, status, action). Resolve dialog {remark} → `PUT reconciliation/items/:id/resolve`.
- **ApiCredentials** (API keys of downstream sites):
  - Filters: search, status (`pending_review, approved, rejected`).
  - Columns: id, user, apiKey, status, isActive, lastUsedAt, createdAt, actions (approve, reject with reason, toggle active, delete, detail).

### Telegram Bot
- Settings come from `composables/useTelegramBotSettings.ts` (`GET/PUT settings/telegram-bot`; upload for the cover image). Form shape:
  - `enabled`, `default_locale`
  - `basic {display_name, description[lang], support_url, cover_url}`
  - `welcome {enabled, message[lang]}`
  - `help {enabled, title, intro, center_hint, support_hint [lang], items[≤12]: {key, enabled, order, summary, title, content [lang], show_support_link}}`
  - `menu.items[≤20]: {key, enabled, order, label[lang], action{type builtin|url|web_app|command, value}}`
- **Pages:**
  - **TelegramBot (overview):** runtime status card (`settings/telegram-bot/runtime-status`), feature cards, quick actions.
  - **Settings:** basic and welcome sections.
  - **HelpCenter:** help items with move up/down.
  - **MenuSettings:** menu items with move up/down.
  - **Status:** connected, last_seen_at, bot_version, webhook_status, machine_code, license_status and license_expires_at, warnings, config_version, last_config_sync_at.
  - **ChannelClients:** columns name, channelKey, channelSecret, botToken, callbackUrl, status, actions (copy, edit, toggle status → `{status:number}`, reset secret, delete). Create and edit fields: name, description, bot_token, callback_url (create also sends channel_type).
  - **Broadcasts:** filters keyword, recipientType (`all|specific`), status (`pending|running|completed|failed`), created range. Columns: title, recipientType, status, recipientCount, success, failed, createdAt, completedAt, actions (view, delete).
  - **BroadcastCreate:** title, recipient_type all or specific. For specific, a user table from `telegram-bot/users` with filters keyword, display_name, telegram_username, telegram_user_id, created range, and columns displayName, tgUsername, tgId, email, boundAt. Also message_html and an attachment (MediaPicker, attachment_url and name).
  - **BroadcastDetail:** read-only view of the broadcast.

### Settings (`Settings.vue`)
13 tabs. A single Save button dispatches to whichever tab is active. The page loads these in parallel: site_config, order_config, smtp, captcha, telegram-auth, google-auth, dashboard_config, registration_config, order-email-template.

1. **basic:**
   - `registration_config` {registration_enabled, email_verification_enabled, email_domain_allowlist_enabled, allowed_email_domains[]}.
   - `order_config` {payment_expire_minutes 1–10080, max_refund_days 0–3650}.
   - `site_config.brand` {site_name, site_url, site_icon, site_logo (MediaPicker dialog), site_description[lang]}; currency (30 ISO codes); seo {title, keywords, description [lang]}; contact {telegram, whatsapp}; footer_links [{name, url}] (max count enforced); scripts [{name, position head|body_end, enabled, code}].
2. **template:** storefront_template `classic|vault` and template_mode `card|list` (visual radio cards).
3. **navigation** (`SettingsNavigationTab`, key `nav_config`): builtin {about, blog, notice} and custom items [{title[lang], link_type internal|external, url, target _self|_blank, icon preset, sort_order, enabled}].
4. **about:** hero {title, subtitle}, introduction, services {title, items[≤12]}, contact {title, text}. All localized.
5. **legal:** terms[lang] and privacy[lang] (RichEditor).
6. **home_announcement** (key `home_announcement`): enabled, type normal|info|warning, title[lang], content[lang], start_at, end_at.
7. **smtp** (`settings/smtp`): enabled, host, port, username, password, from, from_name, use_ssl, use_tls, order_notification_enabled, verify_code {expire_minutes, length, max_attempts, send_interval_seconds}, plus a test email button → `settings/smtp/test`.
8. **order_email_template:** a scene selector; per scene and language {subject, body}; guest_tip[lang]; a variables list (order_no, status, amount, refund_amount, refund_reason, currency, fulfillment_info, site_name, site_url); reset → `POST …/reset`.
9. **captcha** (`settings/captcha`):
   - provider none|image|turnstile;
   - scenes {login, register_send_code, reset_send_code, guest_create_order, gift_card_redeem};
   - image {length, width, height, noise_count, show_line, expire_seconds, max_store};
   - turnstile {site_key, secret_key, verify_url, timeout_ms}.
10. **telegram** (`settings/telegram-auth`): enabled, bot_username, bot_token and client_secret (write-only; the response has has_bot_token and has_client_secret flags plus mode), oidc_redirect_uri, mini_app_url, login_expire_seconds, replay_ttl_seconds.
11. **google** (`settings/google-auth`): enabled, client_id.
12. **dashboard** (key `dashboard_config`): accounting.refund_reverses_cost; alert {low_stock_threshold, out_of_stock_products_threshold, pending_payment_orders_threshold, payments_failed_threshold}; ranking {top_products_limit, top_channels_limit}.
13. **upstream_sync** (key `upstream_sync_config`): interval_minutes, pre_order_stock_check_enabled, sync_conn_concurrency, sync_max_pages, sync_page_size.

### Notifications (`Notifications.vue` and `components/SettingsNotificationTab.vue`)
- **Settings** (`settings/notification-center`):
  - channels.email {enabled, recipients}; channels.telegram {enabled, recipients}; channels.feishu {enabled, app_id, app_secret, receive_id_type chat_id|open_id|user_id|union_id|email, recipients};
  - scenes {wallet_recharge_success, order_paid_success, manual_fulfillment_pending, exception_alert};
  - templates[scene][lang] {title, body};
  - default_locale; dedupe_ttl_seconds; inventory_alert_interval_seconds; payment_order_alert_check_interval_seconds; payment_order_alert_interval_seconds;
  - ignored products (product search picker).
- **Test send:** {channel, scene, target} → `…/test`.
- **Logs** (`…/logs`):
  - Filters: channel, status, eventType, isTest, created range.
  - Columns: createdAt, scene, channel, recipient, type, status, content, error.

### Authz (RBAC) (`Authz.vue`)
UI strings live in an inline `textMap` inside the file, not in i18n. Four sections:
1. **Admins:** search box. Columns: id, username, super, roles, totp, lastLoginAt, createdAt, operation (edit, delete, reset 2FA → `authz/admins/:id/2fa/reset`). Create/edit form: username, password, isSuper.
2. **Roles:** list, create, delete. Immutable roles cannot be modified. Role names have a `role:` prefix, which is stripped for display.
3. **Policies of the selected role:** table of object and action with revoke. Add form: object (default `/admin/`) and action (`GET|POST|PUT|PATCH|DELETE|*`). A permission catalog grouped by module is searchable, collapsible, and each entry has one-click grant.
4. **Admin roles:** pick an admin, multi-select roles, save → `PUT authz/admins/:id/roles {roles}`.

### AuthzAuditLogs (`AuthzAuditLogs.vue`)
- **Filters:** operator_admin_id, target_admin_id, action (`role_create, role_delete, policy_grant, policy_revoke, admin_roles_update`), role, object, method, created range.
- **Row fields:** operator_username, target_username, action, role, object, method, request_id, detail JSON, created_at.

### Security (`Security.vue`, `components/Setup2FAModal.vue`, `components/RecoveryCodesModal.vue`)
- **Change password:** old, new, confirm → `PUT admin/password`.
- **2FA status:** enabled, enabled_at, recovery codes remaining/total.
- **Setup modal:** `2fa/setup` returns secret and otpauth_url; the modal renders the QR with `qrcode`, offers copy secret, takes a 6-digit code → `2fa/enable`, which returns recovery_codes. RecoveryCodesModal offers copy all and download txt.
- **Regenerate codes:** {code}.
- **Disable:** {code} or {recovery_code}.

### Misc views
- **Forbidden:** back button, or go home.
- **ComplianceRequired:** shown to non-super admins while compliance is not acknowledged.
- **ComplianceAckDialog:** a super admin must type 4 exact Chinese phrases (paste is blocked). The UI splits them into 4 boxes; segments 3 and 4 are merged before sending `POST compliance/acknowledge {segment1,2,3}`.
- **SystemUpdateDialog:** version check (a 429 is rate-limited), capability, start update, poll status (running/succeeded/failed), rollback {force}, restart. Release notes are rendered with marked and DOMPurify.

---

## 4. Auth flow (`stores/auth.ts`, `views/Login.vue`)
1. On mount, Login calls `GET /public/config`, which provides `captcha.provider` and `captcha.scenes.login`, `turnstile.site_key`, `brand.site_icon`. If login captcha is on, it shows ImageCaptcha (`GET /public/captcha/image`, producing captcha_id and captcha_code) or a Turnstile widget.
2. `POST /admin/login {username, password, captcha_payload?:{captcha_id, captcha_code}|{turnstile_token}}` returns one of:
   - `{requires_totp:false, token, user{id,username}, expires_at}`
   - `{requires_totp:true, challenge_token, challenge_expires_at}`
3. **TOTP step:** a countdown until challenge expiry, after which it automatically returns to the password step. Input is a 6-digit code, or a recovery code via a toggle. Calls `POST /admin/login/verify-2fa {challenge_token, code|recovery_code}`, which returns the token.
4. After the token arrives: `GET /admin/authz/me` returns `{admin_id, is_super, roles, policies[{subject,object,action}]}`. Policies become permission keys `METHOD:/path`. Matching supports `*` and `:param` wildcards; a super admin passes every check.
5. **Storage (localStorage):** `admin_token`, `admin_is_super`, `admin_roles`, `admin_permissions`.
6. **No refresh token.** Any 401 clears the token and hard-redirects to login. Logout clears storage, resets the compliance store and goes to `adminUrl('/login')`.

## 5. Stores, composables, i18n
- **Stores:**
  - `auth` (above);
  - `compliance` (fetchStatus, acknowledge, reset);
  - `notice` (toasts error/success/info with dedupe; used through `utils/notify.ts`; rendered by `NoticeHost`);
  - `confirm` (promise-based confirm dialog with default/destructive variants and rich description segments; `utils/confirm.ts`; rendered by `ConfirmDialogHost`).
- **Composables:**
  - `useListPage` (loading, items, pagination, jumpPage, debounced search);
  - `useCrudModal`;
  - `useListRefresh`;
  - `useFormValidation` (rule factories required, minLength, maxLength, min, max, …);
  - `useTelegramBotSettings`.
- **Utils:** format (formatMoney, getLocalizedText), sku labels, status maps, category and post-category trees, wholesalePricing, reseller helpers, callbackRoutes, clipboard, upload/image size checks, favicon, fulfillment, releaseNotes.
- **Shared components:** ListPagination, TableSkeleton, IdCell, FileInput, RichEditor (tiptap with an image button that opens MediaPicker, plus tables, color, alignment, links), MediaPicker, captcha components.
- **shadcn/ui primitives in `components/ui`:** alert, badge, button, card, checkbox, dialog, input, label, multi-select, popover, radio-group, select, separator, sheet, switch, table, tabs, textarea, tooltip.
- **i18n (`i18n/index.ts`, 13.4k lines):** languages `zh-CN` (default and fallback), `zh-TW`, `en-US`. Top-level namespaces: `common, order, procurement, reconciliation, apiCredentials, siteConnections, productMappings, payment, orderDetail, admin.*` (most pages), `telegramBot, compliance`. Every string passes through `sanitizeLinkedMessage`, which escapes `@`.
- **Theme (`style.css`):** default shadcn zinc HSL tokens plus `--chart-1..5`, with a `.dark` variant. This is the layer to replace for the anime look.

## 6. Endpoints (all under `/api/v1`, defined in `api/admin.ts`)

**Auth, 2FA, compliance**
- `POST admin/login`, `admin/login/verify-2fa`
- `GET admin/compliance/status`, `POST admin/compliance/acknowledge`
- `GET admin/2fa/status`; `POST admin/2fa/{setup, enable, disable, recovery-codes/regenerate}`
- `PUT admin/password`

**RBAC**
- `GET admin/authz/me`
- `GET/POST admin/authz/roles?include_metadata=true` (returns `{role, immutable}`)
- `DELETE admin/authz/roles/:role`; `GET admin/authz/roles/:role/policies`
- `POST/DELETE admin/authz/policies {role, object, action}`
- `GET/POST admin/authz/admins`; `PUT/DELETE admin/authz/admins/:id`
- `GET/PUT admin/authz/admins/:id/roles`; `POST admin/authz/admins/:id/2fa/reset`
- `GET admin/authz/audit-logs`; `GET admin/authz/permissions/catalog` (items `{module, method, object, permission}`)

**Media**
- `POST admin/upload` (multipart: file, scene)
- `GET admin/media`; `PUT/DELETE admin/media/:id`; `POST admin/media/batch-delete`

**Catalog**
- `admin/products`: GET, POST, GET/PUT/PATCH/DELETE `:id`, PATCH `:id/wholesale-prices`, POST `batch-status`, `batch-category`, `batch-delete`
- `admin/categories`: GET, POST, PUT/DELETE `:id`, PATCH `:id/active`
- `admin/posts`: GET, POST, GET/PUT/DELETE `:id`, GET `:id/products`
- `admin/post-categories`: GET, POST, PUT/DELETE `:id`, PATCH `:id/status`
- `admin/banners`: GET, POST, GET/PUT/DELETE `:id`

**Settings**
- `GET admin/settings?key=` and `PUT admin/settings {key, value}`. Keys used: `site_config, order_config, registration_config, dashboard_config, nav_config, home_announcement, payment_config, wallet_config, callback_routes_config, order_risk_control_config, upstream_sync_config`.
- `GET/PUT admin/settings/{smtp, captcha, telegram-auth, google-auth, order-email-template, notification-center, affiliate, telegram-bot}`
- `POST admin/settings/smtp/test`, `…/order-email-template/reset`, `…/notification-center/test`
- `GET …/notification-center/logs`, `GET …/telegram-bot/runtime-status`

**Public and system**
- `GET public/config`, `GET public/captcha/image`
- `GET admin/system/version`, `…/version/check`, `admin/system/update/{capability, status}`
- `POST admin/system/update/{start, rollback}`, `POST admin/system/restart`

**Dashboard**
- `GET admin/dashboard/{overview, trends, rankings, inventory-alerts}`

**Orders and payments**
- `GET admin/orders`, `GET/PATCH admin/orders/:id`
- `POST admin/fulfillments`; `GET admin/orders/:id/fulfillment/download` (blob)
- `POST admin/orders/:id/{refund-to-wallet, manual-refund}`
- `GET admin/order-refunds`, `GET admin/order-refunds/:id`, `PATCH admin/order-refunds/:id/payment-fee`
- `GET admin/payments`, `GET admin/payments/:id`, `GET admin/payments/export` (blob)
- `admin/payment-channels`: GET, POST, GET/PUT/DELETE `:id`, POST `:id/wechatpay-public-key-test`

**Users and wallet**
- `GET admin/users`, `GET/PUT admin/users/:id`, `PUT admin/users/batch-status`
- `GET admin/users/:id/wallet`, `GET …/wallet/transactions`, `POST …/wallet/adjust`
- `DELETE admin/users/:id/oauth/{telegram, google}`, `DELETE admin/users/:id/2fa`
- `GET admin/users/:id/coupon-usages`, `PUT admin/users/:id/member-level`
- `GET admin/user-login-logs`, `GET admin/wallet/recharges`

**Affiliate**
- `GET admin/affiliates/users`, `PATCH admin/affiliates/users/:id/status`, `PATCH admin/affiliates/users/batch-status`
- `GET admin/affiliates/commissions`
- `GET admin/affiliates/withdraws`, `POST admin/affiliates/withdraws/:id/{reject, pay}`

**Resellers**
- `GET admin/resellers/operations/{overview, finance}`
- `GET admin/resellers/{ledger-entries, balance-accounts, withdraws}`; `POST admin/resellers/withdraws/:id/{reject, pay}`
- `GET admin/resellers/profiles`, `GET/PUT admin/resellers/profiles/:id`, `PUT …/:id/system-domain`, `POST …/:id/{approve, reject, disable, restore}`
- `GET admin/resellers/domains`, `POST …/domains/:id/{approve, disable, set-primary}`
- `GET admin/resellers/site-configs`, `GET/PUT …/site-configs/:rid`, `POST …/site-configs/:rid/reset`
- `GET admin/resellers/product-settings`, `GET/PUT/DELETE …/product-settings/:rid/:pid` (DELETE takes `?sku_id`), `POST …/product-settings/:rid/:pid/preview`

**Marketing**
- `admin/coupons`, `admin/promotions`: GET, POST, PUT/DELETE `:id`
- Gift cards: `POST admin/gift-cards/generate`, `GET admin/gift-cards`, `PUT/DELETE admin/gift-cards/:id`, `PATCH admin/gift-cards/batch-status`, `POST admin/gift-cards/export` (blob)
- Member levels: `admin/member-levels` GET, POST, PUT/DELETE `:id`, POST `backfill`
- Member level prices (defined but unused): `GET admin/member-level-prices?product_id`, `POST …/batch`, `DELETE …/:id`

**Card secrets**
- `POST admin/card-secrets/batch`, `POST admin/card-secrets/import` (multipart)
- `GET admin/card-secrets`, `PUT admin/card-secrets/:id`, `PATCH admin/card-secrets/batch-status`, `POST admin/card-secrets/batch-delete`
- `POST admin/card-secrets/export` (blob), `POST admin/card-secrets/export-available` (blob)
- `GET admin/card-secrets/{stats, batches, template}` (`template` is unused)

**Integration**
- `admin/site-connections`: GET, POST, GET/PUT/DELETE `:id`, POST `:id/ping`, PUT `:id/status`, POST `:id/reapply-markup`
- `admin/product-mappings`: GET, GET/DELETE `:id`, POST `import`, `batch-import`, `batch-import-by-category`, POST `:id/sync`, PUT `:id/status`, POST `batch-sync`, `batch-status`, `batch-delete`
- `GET admin/upstream-products`, `GET admin/upstream-categories?connection_id`
- `GET admin/procurement-orders`, `GET …/stats`, `GET …/:id`, `GET …/:id/upstream-payload/download` (blob), `POST …/:id/{retry, cancel}`
- `POST admin/reconciliation/run`, `GET admin/reconciliation/jobs`, `GET …/jobs/:id`, `PUT admin/reconciliation/items/:id/resolve`
- `GET admin/api-credentials`, `GET/DELETE …/:id`, `POST …/:id/{approve, reject}`, `PUT …/:id/status`

**Telegram and ads**
- `admin/channel-clients`: GET, POST, GET/PUT/DELETE `:id`, PUT `:id/status`, POST `:id/reset-secret`
- `GET/POST admin/telegram-bot/broadcasts`, `GET/DELETE …/broadcasts/:id`, `GET admin/telegram-bot/users`
- `GET admin/ads/render/:slot`, `POST admin/ads/impression`

Full TypeScript request and response shapes for every entity are in `api/types.ts` (1,219 lines) and in the interfaces at the top of `api/admin.ts`. Port them directly.