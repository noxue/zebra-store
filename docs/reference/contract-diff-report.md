# API contract diff: dujiao-next (Go) vs Zebra Store (Rust)

Tool: `backend/scripts/contract-diff/` (Python 3 stdlib only). Last run: 2026-09-25 (after the bind-validation follow-up), **323 endpoints/flows compared, 0 unclassified differences** (47 labelled DEV-01…DEV-07).

## How it works

`run.sh` starts a fresh original Go server (`dujiao-server`, empty SQLite, its own Redis dbs 4/5, port 8092) and a fresh Rust server (empty SQLite, port 8091). `contract_diff.py` then does the following against both:

1. **Seeds equivalent data through the public APIs.** This covers:
   - the admin login, the compliance acknowledgement, and `registration_config` (email verification off)
   - the affiliate setting, a parent category and a child category
   - an epay payment channel (redirect mode, so no network calls)
   - an auto-delivery product with 2 SKUs and a wholesale tier, a manual-delivery product for members only, and card secrets
   - a banner, a post category, a blog post and a notice
   - a member level and a member price, a coupon, a promotion and gift cards
   - an admin role, a policy and a second admin, and a channel client
   - a registered user with a wallet adjustment, a gift-card redemption, a cart item, an affiliate profile and an API-credential application
   - orders: preview, create, create with a coupon, pay through epay, create-and-pay from the wallet, and a manual order paid from the wallet
   - a guest preview and a guest create-and-pay, and a wallet recharge
   - a manual fulfillment and a refund to the wallet
2. **Calls each endpoint and compares the responses.** It calls every GET in the route table (§2 of `backend-spec.md`):
   - public: 17, including `/health`, `robots.txt` and `sitemap.xml`
   - user: 27
   - guest: 4
   - admin: 110, including every list with default parameters, the dashboard, and not-found (`999999`) and invalid (`abc`) ids
   - `GET /admin/settings?key=`: all 20 keys plus one unknown key

   It also runs the write flows above and 35 error and validation cases:
   - a wrong password and missing fields, for both admin and user login
   - missing or bad tokens, a user token on an admin route, and the guest header
   - register: duplicate email, weak password, no agreement
   - `binding:"required"` failures on 13 admin create/update endpoints, and a body that is not an object
   - 60 more `binding:"..."` failures (`required`, `required,min=1`, `oneof=0 1`, `*bool` required) across settings, authz, users, orders/refunds/fulfillment, catalog, content, marketing, wallet, affiliate, integration, notify, cart/checkout, payments, guest and auth endpoints — every one matches the original's status and message
   - a bad coupon, a missing product, a members-only product for a guest, insufficient card secrets, a missing order, and cancelling twice
3. **Compares structurally.** It checks:
   - the HTTP status, the envelope keys, and the exact values of `status_code` and `msg`
   - `pagination.page` and `pagination.page_size`
   - field names, recursively (list elements are merged)
   - value types: `string`, `number`, `bool`, `null`, `object` and `array`, plus the string kinds `money` (`"12.30"`) and `time` (RFC 3339)

   Ids, timestamps, tokens, request ids and order numbers are compared by type only. Differences that match `deviations.json` are labelled with their DEV id.

Output: `backend/scripts/contract-diff/out/diff.md` (tables) and `out/responses.json` (every raw response pair).

### Re-running

```
cd backend
scripts/contract-diff/run.sh                  # build, start both servers on empty DBs, diff, stop them
SKIP_BUILD=1 scripts/contract-diff/run.sh     # reuse target-contract/debug/zebra-store
KEEP=1 scripts/contract-diff/run.sh           # leave both servers running afterwards
GO_SRC=/path/to/dujiao-next GO_PORT=8092 RS_PORT=8091 scripts/contract-diff/run.sh
python3 scripts/contract-diff/contract_diff.py --orig URL --ours URL [--only REGEX]   # against running empty instances
```

The script exits with 1 when any difference is not listed in `deviations.json`. It needs Redis on `127.0.0.1:6379`, because the Go server cannot create orders without it. It refuses to start when either port is already in use.

## Differences found and their classification

### Bugs, fixed in the Rust backend

| # | Endpoint | Original | Ours (before) | Fix | Test |
|---|---|---|---|---|---|
| B1 | `GET /admin/api-credentials` → `data[].user` | Full `User` model: `locale`, `total_recharged`, `total_spent` (money), `email_verified_at`, `last_login_at`, `created_at`, `updated_at`; `admin_note` and `totp_enabled_at` are omitempty | Only `id`, `email`, `display_name`, `status`, `member_level_id` | `CredentialUser` now carries every `users` JSON field (`domain/src/integration/credential.rs`, `infra/.../integration/credential.rs`) | `api/tests/contract_shapes.rs::admin_api_credential_user_is_full_user_model` |
| B2 | Validation errors on bodies bound with `ShouldBindJSON` + `RespondBindError`: admin login, category, product, post, banner, coupon, promotion, member level, gift-card generate, card-secret batch, payment-channel create, compliance acknowledge | `msg` names the failed Go fields in struct order, e.g. `"CategoryID: 不能为空; TitleJSON: 不能为空; PriceAmount: 不能为空"`; `required` also fails on Go zero values (`""`, `0`, `false`, `null`) | Always `"请求参数错误"` | New `extract::Bind<T>` / `bind_json` plus a `BindRules` trait (the Go field names from each `binding:"required"`), and `i18n::bind_message`, which uses `validation.<Field>.<tag>`, falling back to `validation.rule.<tag>`. Type errors keep the `error.bad_request` message. | `contract_shapes.rs::bind_errors_name_go_fields`; updated expectations in `marketing_coupon.rs`, `marketing_gift_card.rs`, `payment_admin.rs`, `identity_compliance.rs` |
| B3 | `order:auto_fulfill` after a wallet payment. Seen as: `GET /orders/stats` has `paid` where the original has `completed`; `children[].fulfillment` missing; `/orders/:no/fulfillment/download` returns JSON, not `text/plain` | Delivered at once | The job failed with SQLite `database is locked` (517 / `SQLITE_BUSY_SNAPSHOT`) from the multi-connection pool | Fixed in parallel by another agent (`infra/src/db/mod.rs`: SQLite pool pinned to 1 connection). This tool found the problem, and later runs confirm the fix. | That agent's tests |

### Documented deviations (kept on purpose, listed in `deviations.json`)

| ID | Class | Endpoint(s) | Original | Ours | Why |
|---|---|---|---|---|---|
| DEV-01 | compatible | JSON map/list fields that were never set: product `seo_meta`/`description`/`content`/`instructions`/`images`/`tags`, order item `tags`, admin payment `provider_payload`, `fulfillment.delivery_data` | `null` | `{}` / `[]` | The original also returns `{}` / `[]` once such a field is saved empty, so its clients already handle both. Our frontends do too (`formatPayload` treats a falsy value as `-`). Switching to `null` would mean changing many domain types without helping any client. |
| DEV-02 | intentional | `GET /public/config` | no `theme` | `data.theme` | A documented extension (`docs/PLAN.md` "主题可配置", `docs/DESIGN.md`) |
| DEV-03 | Go bug | `GET /payments/latest`, `GET /guest/payments/latest` | `channel_name` always omitted | `channel_name` present | The original declares the field and JOINs `payment_channels.name`, but `Payment.ChannelName` is `gorm:"-"`, so the value is dropped. |
| DEV-04 | Go bug | `GET /admin/products/abc` | 500 `获取商品失败` (the raw id reaches SQL: `no such column: abc`) | 400 `请求参数错误` | This is an injection-shaped Go bug, so we do not reproduce it. |
| DEV-05 | Go bug | `GET /admin/reconciliation/jobs/:missing` | 500 with the untranslated `error.reconciliation_fetch_failed` | 404 `error.reconciliation_job_not_found` | The original maps not-found to a 500. |
| DEV-06 | Go bug | `POST /orders`, `POST /orders/create-and-pay` with insufficient card secrets | 500 `创建订单失败` | 400 `卡密库存不足` | The handler maps `cardsecretapp.ErrInsufficient`, but the service returns `order/application.ErrCardSecretInsufficient`. The intended code is 400, which the channel API does return. |
| DEV-07 | security | `GET /admin/orders/:id` → `payments[]` | Full `provider_payload`, `pay_url`, `qr_code` | Redacted like `/admin/payments` | Upstream commit `4e4d5bbc` ("加固支付安全边界") added `redactAdminPayment` for the payment views but missed the order-detail copy. |

### Unavoidable / formatting notes (not counted as differences)

- **Timestamps:** both sides send RFC 3339. The original formats `time.Time` in the server's local zone (`+08:00`, with or without fractional seconds), and some fields come out in UTC `Z`. Ours is always UTC `Z`, as the spec requires ("时间统一 UTC"). Clients parse either form.
- **Captcha, versions, `server_time`:** values are random or depend on the build, so only their types are compared.
- **Not covered:** some lists were empty on both sides, so their element shapes were not compared (see "Coverage notes" in `out/diff.md`). These are:
  - affiliate commissions and withdrawals
  - all reseller views (`reseller.enabled` is off in the original's config)
  - site connections, product mappings, procurement orders and reconciliation jobs (they need a second shop)
  - Telegram broadcasts and users, notification logs, and media (uploads are multipart)

  The channel API and the upstream API (both HMAC-signed) are also not covered.

## Bind validation follow-up (all endpoints)

Every JSON (and the one query) binding whose original handler answers with `ginutil.RespondBindError`, `respondChannelBindError` or `channelresponse.BindError` now uses the same rules: **99 handlers** bind through `BindRules` (79 switched in this follow-up, on top of the 20 of B2). The implementation (`crates/api/src/extract.rs`):

- `BindField { json, go, rules }` lists the Go struct's validated fields in declaration order; `Rule` covers every tag the original uses: `required` (value types: absent/`null`/Go zero value fails; empty slices and maps pass), `required` on pointers (`*bool`, `*[]T`: only absent/`null` fails), `min=N` (slice length / number / rune count) and `oneof` (an absent field is the Go zero value, so `{}` passes `oneof=0 1`). Only the first failing tag of each field is reported, as in go-playground/validator. Nested structs are not validated (the original has no `dive`).
- Messages follow `formatFieldError` exactly: `validation.<Field>.<tag>`, else `Field: <validation.rule.tag with %s = param>` (`"IDs: 最小值为 1"`, `"Status: 必须是以下值之一: 0 1"`), else `Field: tag=param`; joined with `"; "`, in the request locale.
- A JSON type error still wins over missing fields (`error.bad_request`), a JSON `null` body is the zero struct (Go decodes it without error), and `null` in a required field is treated like an absent one.
- Channel API: the same message in the channel envelope, HTTP 400, `error_code: validation_error`; `GET /channel/payments/latest` applies `form:"order_id" binding:"required"` (absent/zero → `OrderID: 不能为空`, unparsable → `请求参数错误`).
- Upstream API `POST /upstream/orders`: the original echoes the raw validator text, so we return `invalid request body: Key: 'createOrderRequest.SKUID' Error:Field validation for 'SKUID' failed on the 'required' tag` (one line per field).
- Endpoints whose original handler answers with the generic message keep it: user login, Telegram widget/OIDC **login** (the bind variants are detailed), `PUT /admin/media/:id` (`error.invalid_params`), reconciliation run/resolve, and the `latest` payment queries.
- Fixed while doing this: `POST /guest/payments` checked the guest credentials before binding the body; the original binds first.

Tests: `api/tests/bind_messages.rs` (admin, storefront/guest/auth, upstream), `api/tests/regression_notify.rs::channel_bind_errors_name_go_fields`, `api/tests/contract_shapes.rs::bind_errors_name_go_fields`, and the updated expectations in the module tests that asserted the generic message.
