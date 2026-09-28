# Contract diff (generated)

Generated 2026-09-25 05:38:04. Endpoints/flows compared: **323**. Differences: **47** (0 unlabelled, 47 documented).

## Timestamp formats observed

- orig: frac/Z, frac/offset, nofrac/Z, nofrac/offset
- ours: frac/Z, nofrac/Z

## Unlabelled differences

| check | path | kind | orig | ours |
|---|---|---|---|---|

## Documented deviations

| check | path | kind | orig | ours | class | reason |
|---|---|---|---|---|---|---|
| POST /admin/products | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/:id (seed) | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.content` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.description` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.images` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.instructions` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/products (manual) | `data.tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /orders (manual) | `data.children[].items[].tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /orders (manual) | `data.items[].tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| POST /admin/orders/{paid_order_id}/refund-to-wallet | `data.order.children[].fulfillment.delivery_data` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products | `data[].seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{product_id} | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.content` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.description` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.images` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.instructions` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/products/{manual_product_id} | `data.tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/orders/{user_order_id} | `data.payments[].provider_payload.endpoint` | missing | string | <absent> | security | DEV-07: upstream commit 4e4d5bbc (payment security hardening) redacts provider_payload, pay_url and qr_code in the admin payment list and detail (redactAdminPayment) but missed the payments embedded in admin order detail. Ours applies the same redaction there. |
| GET /admin/orders/{user_order_id} | `data.payments[].provider_payload.mode` | missing | string | <absent> | security | DEV-07: upstream commit 4e4d5bbc (payment security hardening) redacts provider_payload, pay_url and qr_code in the admin payment list and detail (redactAdminPayment) but missed the payments embedded in admin order detail. Ours applies the same redaction there. |
| GET /admin/orders/{user_order_id} | `data.payments[].provider_payload.params` | missing | object | <absent> | security | DEV-07: upstream commit 4e4d5bbc (payment security hardening) redacts provider_payload, pay_url and qr_code in the admin payment list and detail (redactAdminPayment) but missed the payments embedded in admin order detail. Ours applies the same redaction there. |
| GET /admin/orders/{paid_order_id} | `data.children[].fulfillment.delivery_data` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/orders/{paid_order_id} | `data.payments[].provider_payload` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/payments | `data[].provider_payload` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/payments/{user_payment_id} | `data.provider_payload` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /admin/reconciliation/jobs/999999 | `status_code` | value | 500 | 404 | go-bug | DEV-05: the original maps record-not-found to 500 with an untranslated key ("error.reconciliation_fetch_failed"). Ours returns 404 error.reconciliation_job_not_found. |
| GET /admin/reconciliation/jobs/999999 | `msg` | value | "error.reconciliation_fetch_failed" | "error.reconciliation_job_not_found" | go-bug | DEV-05: the original maps record-not-found to 500 with an untranslated key ("error.reconciliation_fetch_failed"). Ours returns 404 error.reconciliation_job_not_found. |
| GET /admin/products/abc | `status_code` | value | 500 | 400 | go-bug | DEV-04: the original passes the raw :id into GORM, so a non-numeric id becomes SQL ("no such column: abc") and a 500 error.product_fetch_failed. Ours rejects it as 400 error.bad_request. |
| GET /admin/products/abc | `msg` | value | "获取商品失败" | "请求参数错误" | go-bug | DEV-04: the original passes the raw :id into GORM, so a non-numeric id becomes SQL ("no such column: abc") and a 500 error.product_fetch_failed. Ours rejects it as 400 error.bad_request. |
| GET /public/config | `data.theme` | extra | <absent> | object | intentional | DEV-02: site_config.theme (colours, background, mascot, effects) is a documented superset of the original config (docs/PLAN.md, docs/DESIGN.md). |
| GET /public/products | `data[].seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-auto | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-manual | `data.content` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-manual | `data.description` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-manual | `data.images` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-manual | `data.seo_meta` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /public/products/cd-manual | `data.tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /orders/{paid_order_no} | `data.children[].fulfillment.delivery_data` | nullability | null | object | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /orders/{manual_order_no} | `data.children[].items[].tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /orders/{manual_order_no} | `data.items[].tags` | nullability | null | array | compatible | DEV-01: JSON map/list columns that were never set (product texts/images/tags, order item tags, payment provider_payload, auto-fulfillment delivery_data): the original emits null (nil Go map/slice), ours an empty {} / []. The original emits {} / [] for the same fields once they are saved empty, so clients already handle both (ours included). |
| GET /payments/latest?order_no={user_order_no} | `data.channel_name` | extra | <absent> | string | go-bug | DEV-03: the original's LatestPaymentResp declares channel_name (omitempty) and its store JOINs payment_channels.name, but Payment.ChannelName is gorm:"-" so GORM drops the column and the field is always omitted. Ours fills the declared field. |
| GET /guest/payments/latest?order_no={guest_order_no} | `data.channel_name` | extra | <absent> | string | go-bug | DEV-03: the original's LatestPaymentResp declares channel_name (omitempty) and its store JOINs payment_channels.name, but Payment.ChannelName is gorm:"-" so GORM drops the column and the field is always omitted. Ours fills the declared field. |
| create-and-pay insufficient card secrets | `status_code` | value | 500 | 400 | go-bug | DEV-06: the original's order handlers map cardsecretapp.ErrInsufficient to 400 error.card_secret_insufficient, but the order service returns its own sentinel, order/application ErrCardSecretInsufficient, so errors.Is misses it and POST /orders and create-and-pay fall back to 500 error.order_create_failed. Ours returns the intended 400, which the channel API also returns for the same case. |
| create-and-pay insufficient card secrets | `msg` | value | "创建订单失败" | "卡密库存不足" | go-bug | DEV-06: the original's order handlers map cardsecretapp.ErrInsufficient to 400 error.card_secret_insufficient, but the order service returns its own sentinel, order/application ErrCardSecretInsufficient, so errors.Is misses it and POST /orders and create-and-pay fall back to 500 error.order_create_failed. Ours returns the intended 400, which the channel API also returns for the same case. |
| create order insufficient card secrets | `status_code` | value | 500 | 400 | go-bug | DEV-06: the original's order handlers map cardsecretapp.ErrInsufficient to 400 error.card_secret_insufficient, but the order service returns its own sentinel, order/application ErrCardSecretInsufficient, so errors.Is misses it and POST /orders and create-and-pay fall back to 500 error.order_create_failed. Ours returns the intended 400, which the channel API also returns for the same case. |
| create order insufficient card secrets | `msg` | value | "创建订单失败" | "卡密库存不足" | go-bug | DEV-06: the original's order handlers map cardsecretapp.ErrInsufficient to 400 error.card_secret_insufficient, but the order service returns its own sentinel, order/application ErrCardSecretInsufficient, so errors.Is misses it and POST /orders and create-and-pay fall back to 500 error.order_create_failed. Ours returns the intended 400, which the channel API also returns for the same case. |

## Coverage notes (one side returned an empty list, elements not compared)

- GET /admin/affiliates/commissions: `data` empty on both sides
- GET /admin/affiliates/withdraws: `data` empty on both sides
- GET /admin/authz/admins/1/roles: `data` empty on both sides
- GET /admin/authz/me: `data.policies` empty on both sides
- GET /admin/authz/me: `data.roles` empty on both sides
- GET /admin/coupons: `data[].member_levels` empty on both sides
- GET /admin/coupons: `data[].payment_roles` empty on both sides
- GET /admin/media: `data.items` empty on both sides
- GET /admin/payment-channels/{channel_id}: `data.member_levels` empty on both sides
- GET /admin/payment-channels/{channel_id}: `data.payment_roles` empty on both sides
- GET /admin/payment-channels: `data[].member_levels` empty on both sides
- GET /admin/payment-channels: `data[].payment_roles` empty on both sides
- GET /admin/procurement-orders: `data` empty on both sides
- GET /admin/product-mappings: `data` empty on both sides
- GET /admin/products/{manual_product_id}: `data.wholesale_prices` empty on both sides
- GET /admin/reconciliation/jobs: `data` empty on both sides
- GET /admin/resellers/balance-accounts: `data` empty on both sides
- GET /admin/resellers/domains: `data` empty on both sides
- GET /admin/resellers/ledger-entries: `data` empty on both sides
- GET /admin/resellers/operations/finance: `data.current_currency_rows` empty on both sides
- GET /admin/resellers/operations/finance: `data.period_currency_rows` empty on both sides
- GET /admin/resellers/operations/overview: `data.alerts` empty on both sides
- GET /admin/resellers/operations/overview: `data.top_resellers` empty on both sides
- GET /admin/resellers/product-settings: `data` empty on both sides
- GET /admin/resellers/profiles: `data` empty on both sides
- GET /admin/resellers/site-configs: `data` empty on both sides
- GET /admin/resellers/withdraws: `data` empty on both sides
- GET /admin/settings/notification-center/logs: `data` empty on both sides
- GET /admin/settings/notification-center: `data.channels.email.recipients` empty on both sides
- GET /admin/settings/notification-center: `data.channels.feishu.recipients` empty on both sides
- GET /admin/settings/notification-center: `data.channels.telegram.recipients` empty on both sides
- GET /admin/settings/notification-center: `data.ignored_product_ids` empty on both sides
- GET /admin/settings/notifications: `data.channels.email.recipients` empty on both sides
- GET /admin/settings/notifications: `data.channels.feishu.recipients` empty on both sides
- GET /admin/settings/notifications: `data.channels.telegram.recipients` empty on both sides
- GET /admin/settings/notifications: `data.ignored_product_ids` empty on both sides
- GET /admin/settings/telegram-bot/runtime-status: `data.warnings` empty on both sides
- GET /admin/settings?key=registration_config: `data.allowed_email_domains` empty on both sides
- GET /admin/site-connections: `data` empty on both sides
- GET /admin/telegram-bot/broadcasts: `data` empty on both sides
- GET /admin/telegram-bot/users: `data` empty on both sides
- GET /admin/users/{user_id}: `data.oauth_identities` empty on both sides
- GET /affiliate/commissions: `data` empty on both sides
- GET /affiliate/withdraws: `data` empty on both sides
- GET /public/config: `data.allowed_email_domains` empty on both sides
- GET /public/config: `data.nav_config.custom_items` empty on both sides
- GET /public/config: `data.scripts` empty on both sides
- GET /reseller/profile: `data.domains` empty on both sides
- POST /admin/coupons: `data.member_levels` empty on both sides
- POST /admin/coupons: `data.payment_roles` empty on both sides
- POST /admin/payment-channels: `data.member_levels` empty on both sides
- POST /admin/payment-channels: `data.payment_roles` empty on both sides
- POST /admin/products (manual): `data.wholesale_prices` empty on both sides
- PUT /admin/settings: `data.allowed_email_domains` empty on both sides
