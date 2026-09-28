# Live QA: 管理后台 + 运维 (store.dot2.com / zs2.dot2.com)

> 2026-09-27 文档更新：手册已加入 89 张真实应用截图；需要独立宝塔环境的 6 张截图不再由正文引用，因此线上文档已没有损坏的截图链接。下文保留原始 QA 记录作为历史依据。

- **Date:** 2026-09-26 (UTC+8), around 01:45–02:50
- **Build:** `zebra-store 0.1.0` (lab image `zebra-lab/zebra:amd64`)
- **Tester:** Claude QA agent, using Playwright against the real admin UI
- **Scope:** every flow in TEST_FLOWS §2 (B-001…B-132) and §5 (O-001…O-012)

**How it was tested.** Scripts are in `e2e/live/admin/`:
- `crawl-admin.mjs` crawls all 56 admin routes and records console errors, 4xx/5xx, non-zero envelopes, raw i18n keys, CJK leaking into en-US, horizontal overflow and load time. It ran on SA in zh-CN, zh-TW and en-US, on SA mobile (390x844), and on Z2 in zh-CN.
- `run.mjs` runs the per-flow scripts: `a/` auth and RBAC, `b/` settings and notifications, `c/` catalog and marketing, `d/` orders, users and payments, `s/` dashboard.

Screenshots are in `e2e/live/shots/admin/`. File names start with `crawl-`, `dash-`, `a-`, `b-`, `c-`, `d-` or `o012-`.

**Safety.**
- The main `admin` password and 2FA were not touched.
- Every setting that was changed was backed up first (`e2e/live/admin/out/*backup*.json`), restored, and the restore was verified.
- Test data was prefixed `qa-`. It was deleted, except for the items listed under "Left on the lab" below.
- Payment channels were created disabled and then deleted.
- The live DB was read only. The single exception is an online `.backup` read, taken through a temporary `zebra-lab-qa-backup` container that was then removed.

## Crawl summary

- **Errors:** all 56 routes × 3 locales + mobile on SA, and the Z2 crawl, gave **0 console errors, 0 page errors, 0 HTTP ≥ 400 and 0 non-zero envelopes**.
- **Speed:** typical load (networkidle) is 1.4–4.3 s.
- **i18n:** no raw i18n keys and no untranslated UI labels in en-US or zh-TW. The only CJK in en-US is user data such as product names and tags.
- **Layout:** no page overflows horizontally at 390 px.
- **Match with the original:** layout and information structure match `docs/reference/screenshots/admin/*` (for example, the settings tabs are identical).
- **Anomalies:**
  - One slow episode around 18:02 UTC: `/admin/` hit a 30 s timeout and API calls took 6–14 s while 4 agents were testing at once.
  - One Z2 login timed out on the first attempt; the retry worked.

## Flow results

`PASS` = works as specified; `PARTIAL` = core works, but a sub-step failed or could not be verified live; `FAIL` = the expected result is missing; `N/A` = not executable in the lab.

### 2.1 Login, compliance, security

| ID | Result | Evidence |
|---|---|---|
| B-001 | PASS | Unknown user and wrong password both show `用户名或密码错误`. Login lands on the dashboard. `a-login-error.png` |
| B-002 | PARTIAL | SA's compliance was already acknowledged, so the 4-phrase dialog could not be exercised live. The router guard (`meta.compliance`) and the `ComplianceRequired` view were verified in code; api test covers it. |
| B-003 | PASS | Setup QR → recovery codes → TOTP challenge on login (qa-admin). "Challenge dies after 5 wrong codes" can't be reached because the IP login limiter trips first (see I-3). `a-2fa-*.png` |
| B-004 | PASS | After qa-admin changes the password, the old token gets `401`. `a-pw-confirm.png` |
| B-005 | PARTIAL | `reset-password` works and revokes sessions (tested on qa-admin). The documented command `reset-2fa` does not exist; the binary accepts only `reset2fa` (I-6). |

### 2.2 Dashboard

| ID | Result | Evidence |
|---|---|---|
| B-010 | PASS | Today / 7d / 30d / custom each send `range` + `tz=Asia/Singapore`. "强制刷新" adds `force_refresh=true`. The KPIs match the order list (21 orders, 249.90 GMV at crawl time). Trend GMV per day shows 0.00 for wallet-paid days because the trend is payment-based, the same as the original. The custom range default is off by one day (I-21). `dash-range-0..3.png` |
| B-011 | PASS | Product ranking adds up to the KPIs (15+3+3 orders; 172.50+45.00+32.40 = 249.90). Channel ranking is empty because every order was paid by wallet. The funnel is consistent. |
| B-012 | PASS | 2 low-stock SKUs (Claude Max 5x/20x, stock 3 < threshold 5) are listed, and the link opens `/products?product_id=4` with the edit dialog. `dash-alert-link.png` |
| B-013 | PARTIAL | The `dashboard_config.accounting.refund_reverses_cost` switch saves and was restored. The effect can't be observed: the only refunded orders (#56, #58) have item cost 0.00, so cost and profit don't change (118.50 / 731.53 either way). `b013-setting.png` |
| B-014 | FAIL | The "检测更新" dialog calls `GET /admin/system/version`, which returns 404 plus a console error. It never shows the version, the capability, or "当前部署方式不支持一键升级" (I-5). `a-system-update-dialog.png` |

### 2.3 System settings

| ID | Result | Evidence |
|---|---|---|
| B-020 | PASS | Site name, SEO title, logo and favicon reached the storefront nav, title, favicon and footer and the admin login immediately. Restored afterwards. `b-020-storefront2.png`, `b-020-admin-login2.png` |
| B-021 | PARTIAL | Currency, meta tags, contact and footer links work. **A custom script entered as plain JS never executes** (I-8). `b-021-storefront.png` |
| B-022 | PASS | An invalid colour is rejected. Colours and dark mode reach the storefront and admin login. "恢复默认配色" works. `b-022-*.png` |
| B-023 | PASS | Card/list template mode applies on the storefront. |
| B-024 | PARTIAL | Hiding the blog and adding an external menu item work. An *internal* item with an absolute URL is silently dropped while the UI says 保存成功 (I-20). `b-024-*.png` |
| B-025 | PASS | About, legal and announcement save per locale. An announcement end date before its start date is accepted without error (I-20). |
| B-026 | PASS* | The SMTP password is masked in the UI and kept on re-save; test-send errors are clear. **However, `GET /admin/settings?key=smtp_config` returns the password in plaintext** (I-1). Port 0 silently becomes 587. `b-026-*.png` |
| B-027 | PASS | Edit and reset work. An empty subject gives a raw English backend message (I-19). |
| B-028 | PASS | Turnstile without a key is refused. The image captcha appeared on storefront login and was switched back to none about 30 s later. `b-028-*.png` |
| B-029 | PASS | With registration off, the storefront shows it is closed and the API returns 403. The domain allowlist can be enabled with an empty list (I-20). |
| B-030 | PARTIAL | Values save, but −5 is silently clamped to 1. Expiry was not checked on a new order. `b-030-neg-timeout.png` |
| B-031 | PARTIAL | Values save, but 0 is silently reset to 5. Sync timing can't be observed. `b-031-zero.png` |
| B-032 | PARTIAL | Prefix and reserved-path checks work. **Conflicting paths are accepted** (I-9), and `/api/v1/../admin/x` is stored verbatim. TEST_FLOWS says the old path stays usable, but the code (and the original) hide the default path once a custom one is set, so the doc needs fixing. `b-032-*.png` |

### 2.4 RBAC and audit

| ID | Result | Evidence |
|---|---|---|
| B-040 | PASS | 6 built-in roles, immutable in both the UI and the API. |
| B-041 | PASS | `qa-auditor` (readonly_auditor) sees only 仪表盘 and 安全设置. Other routes go to `/forbidden` and the API returns 403. `a-auditor-*.png` |
| B-042 | PARTIAL | A role granted only `GET:/admin/orders` **cannot log in**: `/admin/authz/me` returns 403 and the user sees a generic 无权限访问 toast (I-14). After more policies were added, the role can view orders but not change them. `a-custom-role-policies.png`, `a-qaadmin-orders.png` |
| B-043 | PARTIAL | Deleting `admin` is refused, but only as "cannot delete the current admin". The bootstrap-protection path from *another* super admin was not exercised, to avoid risking the only super admin. `a-delete-bootstrap-confirm.png` |
| B-044 | PASS | Reset 2FA of qa-admin from the admin list; the next login needs no TOTP. `a-reset2fa-confirm.png`, `a-after-reset2fa-login.png` |
| B-045 | PASS | Role create, grant and assign events are logged with operator, target and request_id. Filters and paging work (the role filter needs the `role:` prefix, I-22). `a-audit-logs*.png` |

### 2.5 Products, SKUs, card secrets

| ID | Result | Evidence |
|---|---|---|
| B-050 | PASS | Required-field and duplicate-slug errors, 2-level lock, deactivated child hidden on the storefront. `c-cat-*.png` |
| B-051 | PASS | 3-language title, 2 SKUs, image and tags appear on the storefront. The list price shows the last SKU, not the lowest (I-23, same as the original). `c-prod-form.png` |
| B-052 | PASS | Manual stock 20 and the order form (text required + select) appear on the storefront. `c-manual-form.png` |
| B-053 | PASS | Inline sort, category and status edits; batch on/off/move report "2/2"; filters and paging work. `c-prod-inline.png`, `c-prod-batchbar.png` |
| B-054 | PASS | Deleting a product with stock is refused; disabling an auto SKU that has card secrets is refused; a clean delete cascades. `c-sku-disable-guard.png` |
| B-055 | PASS | An invalid tier is refused; tiers show on the storefront; "清空批发价" works. `c-wholesale-*.png`, `c-sf-product-wholesale.png` |
| B-056 | PARTIAL | Disabled or unknown channel ids are filtered out on save. Storefront checkout couldn't be checked: the lab has no active channel and the test channel stays disabled. |
| B-057 | PASS | 10 pasted lines with 1 duplicate → 9 imported, batch number shown. **Re-importing the same secrets creates duplicates** (I-10). `c-import-paste.png` |
| B-058 | PARTIAL | CSV import works in the UI (the UI only offers .csv, same as the original); TXT works via the API. A file named `evil.php` was imported as secrets (I-23). `c-import-csv.png` |
| B-059 | PARTIAL | Filters, bulk status, delete and edit work. **A used (sold) secret can be set back to available** (I-11). `c-secrets-*.png` |
| B-060 | PASS | Export 5 as txt with delete-after-export: the file is correct and stock went 11 → 6. `c-export-result.png` |

### 2.6 Orders

| ID | Result | Evidence |
|---|---|---|
| B-070 | PASS | Filters by status, user, order number, guest email, product keyword and time; amount sort; paging and jump. `d-orders-jump99.png` |
| B-071 | PASS | Amount breakdown, items, child orders, delivery, procurement and payments are shown. `d-order-detail-A*.png` |
| B-072 | PARTIAL | Completed and refunded orders are locked. **But a paid order can be set to 已退款 from the status dropdown with no money moving** (I-2). `d-order-status-B-已退款.png` |
| B-073 | PASS | Manual delivery. `d-deliver-modal.png`, `d-deliver-after.png` |
| B-074 | PASS | Partial and full wallet refunds, the manual refund record, and the fee-refunded toggle. `d-refund-*.png`, `d-order-refunds.png` |
| B-075 | PASS | Payment records filter, detail and export. `d-payments.png`, `d-payment-detail.png` |
| B-076 | PASS | Risk-control switches, IP blacklist, limits and the recommended guest policy save and reload; restored afterwards. The storefront effect is covered by F-054/F-055. `d-risk-*.png` |

### 2.7 Users, wallet, member levels

| ID | Result | Evidence |
|---|---|---|
| B-080 | PASS | ID, keyword, status and time filters, balance sort, batch disable/enable. `d-users-*.png` |
| B-081 | PASS | Edit nickname, email, locale, verified flag and note; password reset revokes the user's session. `d-user-edit-*.png` |
| B-082 | PARTIAL | +100 and −50 work and the ledger records both. Over-deducting is refused, **but with the wrong message 支付金额不匹配** (I-12). `d-wallet-overdeduct.png` |
| B-083 | PASS | Orders, payments, coupon usage and wallet tabs. `d-user-tab-*.png` |
| B-084 | PARTIAL | Resetting user 2FA works. OAuth unbinding is untestable because no lab user has Telegram or Google bound. `d-user-2fa-*.png` |
| B-085 | PASS | Setting the level manually updates storefront pricing. `d-user-level-set.png` |
| B-086 | PASS | Login-log filters. `d-login-logs-filtered.png` |
| B-087 | PASS | The page and filters work; the lab has no recharges yet. `d-wallet-recharges.png` |
| B-088 | PASS | Wallet config saves and was restored. `d-wallet-config-saved.png` |
| B-089 | PARTIAL | CRUD works. Backfill without a default shows the **raw key `error.member_level_no_default`** (I-7). Discount 120% and negative values are accepted (I-20). **A level still assigned to a user can be deleted** (I-13). `d-levels-*.png` |

### 2.8 Marketing and content

| ID | Result | Evidence |
|---|---|---|
| B-090 | PASS | Full coupon CRUD with every limit. Errors are generic (I-19), and codes are unique only case-sensitively (I-23). `c-coupon-*.png` |
| B-091 | PASS | Promotion price 20% applied on the storefront; it drops off once the window expires. `c-promo-*.png` |
| B-092 | PASS | Generate 10, batch disable, export txt/csv; a disabled card can't be redeemed. `c-gift-*.png` |
| B-093 | PASS | Desktop and mobile images show on the storefront. A `javascript:` link is accepted (I-18). `c-banner-list.png`, `c-sf-home-mob.png` |
| B-094 | PASS | Drafts are hidden (404); the related product is linked in both directions. `c-posts-list.png`, `c-sf-blog.png` |
| B-095 | PASS | Tree, deactivation, and deleting a parent that has children is refused. `c-postcat-list.png` |
| B-096 | PASS | Upload, rename and batch delete work. .php, .svg, .html and disguised PHP are refused and path traversal is neutralised. **The refusal messages are hard-coded Chinese** (I-15). `c-media-*.png` |

### 2.9 Affiliate

| ID | Result | Evidence |
|---|---|---|
| B-100 | FAIL | Saving affiliate settings **does not invalidate the public-config cache**, so the storefront shows stale affiliate config for about 30–60 s (I-4). `d-aff-settings-*.png` |
| B-101 | PASS | Filter and disable/enable, single and batch. `d-aff-user-disabled.png` |
| B-102 | PARTIAL | The commission and withdraw pages render. No commission could be generated because `lab-e2e-card` has affiliate disabled. `d-aff-commissions.png`, `d-aff-withdraws.png` |

### 2.10 Notifications, Telegram

| ID | Result | Evidence |
|---|---|---|
| B-110 | PASS | Channels, recipients, scenes and templates save; an invalid recipient is refused. `b-110-invalid-recipient.png` |
| B-111 | PASS | Test send is logged with `is_test` and a failure reason (the lab has no SMTP). `b-111-test-email.png` |
| B-112 | N/A | No working delivery channel in the lab; covered by api tests. |
| B-113 | N/A | Same reason as B-112. |
| B-114 | PASS | Settings, help center and menu save; the status page shows 未连接. `b-114-*.png` |
| B-115 | PASS | Create → reset secret (old one 401, new one 200) → disable (403) → delete. `b-115-*.png` |
| B-116 | PARTIAL | Empty-form validation works. The lab has 0 users bound to Telegram, so submitting returns a generic 请求参数错误 and the lifecycle can't be tested. `b-116-*.png` |

### 2.11 Payment channels

| ID | Result | Evidence |
|---|---|---|
| B-120 | PASS | A disabled QA epay channel was created, edited and deleted. `d-channel-*.png` |
| B-121 | PARTIAL | The secret is masked and kept on save (verified read-only in the DB). Only 名称 gets a field-level error; the other missing fields share one generic toast (I-19). `d-channel-validation-*.png` |
| B-122 | N/A | No WeChat merchant in the lab. |
| B-123 | PARTIAL | Sort order and scope (member only, level, order/recharge) save. The storefront effect wasn't tested because the channel is disabled. `d-channel-roles-open.png` |
| B-124 | PASS | "买家承担手续费" persists and was restored; storefront F-065 not re-checked. `d-channel-fee150.png` |

### 2.12 Audit and logs

| ID | Result | Evidence |
|---|---|---|
| B-130 | PASS | Same as B-045. |
| B-131 | PASS | The adjustment ledger stores the operator and note (API). The UI doesn't show the operator, same as the original. `d-user-wallet-tab.png` |
| B-132 | PASS | Filters by channel, status and event, plus paging. `b-132-logs.png` |

### 5. Ops

| ID | Result | Evidence |
|---|---|---|
| O-001 | N/A | Not re-run, because it would redeploy the shared lab. `deploy.sh` was reviewed statically: read-only preflight, `zebra-lab` prefix only, ports bound to 127.0.0.1. The lab containers are up and healthy. |
| O-002 | PASS | `lab.sh ps` shows all 6 `zebra-lab-*` containers up and store/zs2 healthy. `TAIL=15 lab.sh logs store` gives structured `tracing` key=value logs, but with ANSI colour escapes (I-24). The logs also show `WARN unknown upstream status status="paid"` from the DJ procurement flow (I-25). |
| O-003 | PARTIAL | The documented `sqlite3 data/zebra.db ".backup …"` **cannot run as documented on this deployment**: neither the host nor the `zebra` image has `sqlite3`, and the DB lives in the docker volume `zebra-lab_store_data` (I-16). Done instead via a temporary `alpine` container mounting the volume. The `.backup` succeeded and `PRAGMA integrity_check` returned `ok` (42 orders, 3 admins). The temp container, image and file were removed. |
| O-004 | PASS (local) | The live backup was restored on a local build (`target-qa`). Admin login with the lab password worked, and orders, users and settings are intact. Upstream stock sync logs `decryption failed` because a different `app.secret_key` was used, which is expected and documented ("secret_key 必须一致"). Note: a restored copy starts the upstream sync jobs immediately (I-26). |
| O-005 | PASS (local) | Empty Postgres DB `zs_qa_live` on zebra-pg:15432: ready in 3 s, 6 built-in roles, super admin login, category CRUD works. The DB was dropped afterwards. |
| O-006 | PASS (local) | Same on MySQL 8.4 (zebra-mysql:13306): ready in 13 s. The DB was dropped afterwards. |
| O-007 | PARTIAL | Starting the current source on the live data snapshot synced the schema with no errors, which is effectively an upgrade. A real image swap on the live lab was not performed. |
| O-008 | PASS (local) | Refuses to start with a clear key name for: an empty `app.secret_key`, a short `jwt.secret` ("must be set to a strong random value"), equal secrets ("jwt.secret and user_jwt.secret must differ"), and `trusted_proxies: ["0.0.0.0/0"]` ("must be an IP or CIDR narrower than /0"). |
| O-009 | PASS (local) | `ZS__SERVER__PORT=19000` beats `port: 8555` in the yml (19000 → 200, 8555 refused). `ZS__LOG__LEVEL=debug` produced 77 DEBUG lines. List-valued keys can't be set from env (I-27). |
| O-010 | N/A | Destructive on the shared lab; not run. |
| O-011 | N/A | Destructive; reviewed statically. It removes only the `zebra-lab` compose project (`down -v`), `zebra-lab/*` images, the `zebra-lab-builder` buildx instance, the Caddy import line (with a backup) and `$DIR`. |
| O-012 | PARTIAL | docs.dot2.com home, sidebar and local search work ("备份" and "数据库" return relevant hits). **Three screenshots on `/deploy/bt-panel` are broken**: `bt-supervisor.png`, `bt-nginx-conf.png` and `bt-ssl.png` return the 404 HTML page (I-28). `/deploy/backup-upgrade` also says the top bar has a "系统更新" button that shows the "不支持一键升级" message, which B-014 contradicts. `o012-docs-*.png` |

**Totals:**

| Group | PASS | PARTIAL | FAIL | N/A |
|---|---|---|---|---|
| Admin (B-*) | 56 | 21 | 2 | 3 |
| Ops (O-*) | 6 | 3 | 0 | 3 |

(PASS includes "PASS*" and "PASS (local)".)

## Issues

**Fix round 2026-09-26 (local, not deployed):** every issue below carries a **Status** line — FIXED with the regression
test name (numbered `QA-Axx` in `docs/reference/bugfix-lessons.md` §24), PARTIAL, or WONTFIX with the reason.

Sorted by severity: P0 money/security/data loss, P1 broken, P2 wrong behaviour or UX, P3 cosmetic. "Baseline" records compatibility behavior observed during testing.

### P0

None found.

### P1

**I-1. The generic settings endpoint leaks secrets in plaintext (security).**
- **Status:** FIXED — generic GET masks secrets, PUT/GET whitelist known keys, secret keys go through the validated patch (empty secret kept); `zebra-store admin prune-settings --apply` removes `foo_bar_unknown`. Tests: `content_settings.rs::qa_a01_generic_settings_mask_secrets`, `schema::tests::qa_a01_key_whitelist`, `infra tests/backup.rs::qa_a01_prune_unknown_settings_keys` (QA-A01).
- **Repro:** `GET /api/v1/admin/settings?key=smtp_config` with any admin role that has `GET:/admin/settings` (for example `operations`). Confirmed live while an SMTP password was set.
- **Actual:** the SMTP password is returned in clear text. The same endpoint reads any stored key, so `captcha_config`, `telegram_auth_config` and `notification_center_config` secrets are exposed the same way once filled.
- **Write side:** `PUT /admin/settings` stores raw values for arbitrary keys, bypassing the per-tab validation. Only `google_auth_config` is blocked. The stray key `foo_bar_unknown` remains on SA (set to `{}`).
- **Expected:** the generic GET masks secret fields (as the SMTP tab does), and PUT only accepts known keys and validates them.
- **Orig:** yes (`admin_handler.go` Get). Still worth hardening.
- **Where:** zs-api generic settings handler (`backend/crates/api/src/routes/content/…settings`), `backend/crates/app/src/content/settings.rs`.

**I-2. Setting "已退款" from the order-list dropdown fakes a refund.**
- **Status:** FIXED — `refunded`/`partially_refunded` refused by `PATCH /admin/orders/:id` (`error.order_status_refund_required`) and removed from the dropdown; refunds only via the refund flow. Tests: `order_admin.rs::qa_a02_status_change_cannot_fake_refund`, admin `orderUtils.test.ts` "QA-A02" (QA-A02).
- **Repro:** order #58 (paid, 160.55) → 订单列表 → 状态 → 已退款. Screenshot `d-order-status-B-已退款.png`.
- **Actual:** status becomes `refunded` with `refunded_amount` 0.00, no wallet credit, no refund record and no stock returned. The full 160.55 could still be refunded afterwards.
- **Expected:** "refunded" is reachable only through the refund flow; the manual status list excludes it, or it triggers a real refund.
- **Orig:** the transition is allowed, but it's a finance trap.
- **Where:** `admin/src/views/orders/Orders.tsx` status options; the domain order transition table in `backend/crates/domain/src/order/`.

**I-3. The admin login rate limiter locks out the shared IP.**
- **Status:** FIXED — admin login/2FA count failures only, keyed by (username, IP) / (IP, challenge); success resets. Tests: `admin_auth.rs::qa_a03_login_limiter_counts_failures_per_account`, `rate_limit::tests::qa_a03_failure_only_limit` (QA-A03).
- **What happens:** every attempt per IP counts, including successful logins and 2FA code checks (5 per 5 min, then a 15 min block). The lab IP was locked out once during testing.
- **Impact:** a whole team behind one NAT/VPN locks each other out after a few logins. B-003's "5 wrong TOTP" rule can't be reached because the IP limit trips first.
- **Expected:** count failures only, keyed by IP + username.
- **Orig:** yes.
- **Where:** `backend/crates/api/src/middleware/rate_limit.rs`.

### P2

**I-4. Affiliate settings don't invalidate the public-config cache.**
- **Status:** FIXED — every settings write applies the key's effects; all keys feeding `/public/config` invalidate it (affiliate, payment, smtp, captcha, telegram/google auth added). Tests: `content_settings.rs::qa_a04_affiliate_save_invalidates_public_config`, `schema::tests::qa_a04_public_config_keys_invalidate_cache` (QA-A04).
- **Actual:** after saving 返利设置, `/public/config.affiliate` stays stale for about 30–60 s, in both directions (B-100).
- **Where:** `update_affiliate` in `backend/crates/app/src/content/settings.rs:337` never calls `invalidate_public_config()`.

**I-5. The "检测更新 / 系统更新" dialog is broken (B-014).**
- **Status:** FIXED — dialog uses `version/check` + `update/capability` and shows "当前部署方式不支持一键升级". Test: admin `src/composables/useSystemUpdateInfo.test.ts` (QA-A05).
- **Actual:** it calls `GET /admin/system/version`, which returns 404 and a console error. It never shows the version, the update capability, or the `source_build` block reason.
- **Expected:** the version is shown, plus "当前部署方式不支持一键升级".
- **Where:** `admin/src/components/SystemUpdateDialog.tsx:24`. The original uses `version/check` and `update/capability`.

**I-6. The documented CLI command doesn't exist.**
- **Status:** FIXED — subcommand is `reset-2fa` with alias `reset2fa`; CLI errors print the translated message + key. Tests: `zs-server cli::tests::qa_a06_reset_2fa_names`, `qa_a07_cli_errors_are_translated` (QA-A06).
- **Docs:** the handbook (`admin/security.md:34`, `faq/index.md:18`, `faq/troubleshooting.md:61`) and TEST_FLOWS B-005 say `zebra-store admin reset-2fa`.
- **Actual:** the binary only accepts `reset2fa`. A locked-out admin who follows the docs gets an error.
- **Fix:** add a `reset-2fa` alias. Where: `backend/crates/server/src/cli.rs:41`.
- **Also:** CLI errors print raw keys (`error.user_not_found`, `error.password_min_length`).

**I-7. Raw `error.*` keys reach the admin UI.**
- **Status:** FIXED — all 41 keys (plus the new ones) translated in zh-CN/zh-TW/en-US. Test: `zs-api i18n::tests::qa_a07_every_error_key_is_translated` scans all crate sources (QA-A07).
- **Example:** 会员等级 → 批量分配默认等级 with no default level shows `error.member_level_no_default`. Media rename to an empty name returns `error.invalid_params`.
- **Scope:** a static check found **41 `error.*` keys used in the backend but missing from `backend/crates/api/src/i18n/messages.json`**, including `member_level_*`, `connection_*`, `procurement_*`, `reconciliation_*`, `not_found` and `internal`.

**I-8. Plain-JS storefront custom scripts never execute.**
- **Status:** FIXED — plain JS wrapped in a live `<script>`, HTML scripts re-created, managed nodes cleared/re-applied. Test: storefront `tests/customScripts.test.ts` (QA-A08).
- **Actual:** the admin hint says plain JS is supported, but `storefront/src/utils/customScripts.ts` appends the code as a text node.
- **Expected (original):** code without `<` is wrapped in a new `<script>`, and managed scripts are cleared and re-applied.

**I-9. Callback-route validation misses conflicts.**
- **Status:** FIXED — saving refuses reserved/`..`/non-canonical paths, built-in callback paths and duplicates (`error.callback_route_invalid`) instead of dropping them. Tests: `integration::tests::qa_a09_callback_routes_validation`, `content_settings.rs::set_01_callback_routes_normalized_via_api` (QA-A09). TEST_FLOWS B-032 corrected (default path is hidden, like the original).
- **Repro:** set 支付回调路由 to `/api/v1/payments/webhook/paypal`, which is another route's default. It is accepted.
- **Impact:** the custom path can never be reached, and the default `/api/v1/payments/callback` is hidden at the same time, so shared-gateway callbacks break silently.
- **Also:** `/api/v1/../admin/x` is stored without normalisation.
- **Where:** `backend/crates/domain/src/payment/routes.rs::normalize_route_path` and the conflict check.

**I-10. Card-secret dedupe only works within a single import.**
- **Status:** FIXED (deviates from original) — with `deduplicate` (default) imports skip secrets already stored for the SKU; all-duplicate → `error.card_secret_all_duplicate`. Test: `catalog_card_secret.rs::qa_a10_a11_reimport_and_sold_secret_guards` (QA-A10).
- **Actual:** re-importing a secret that is already in stock creates a second available row, so the same card can be sold twice.
- **Orig:** yes.
- **Where:** `backend/crates/domain/src/catalog/card_secret.rs:223`, `backend/crates/app/src/catalog/card_secret.rs:117`.

**I-11. A sold (used) card secret can be set back to available and resold.**
- **Status:** FIXED (deviates from original) — `used` is terminal: single edit refused (`error.card_secret_used_locked`), bulk status skips used rows via conditional UPDATE. Test: same as I-10, plus `card_secret::tests::qa_a11_used_secret_is_terminal` (QA-A11).
- **Paths:** bulk status in the UI, and `PUT /admin/card-secrets/:id`.
- **Expected:** TEST_FLOWS B-059 says this is blocked.
- **Orig:** same gap.
- **Where:** `backend/crates/app/src/catalog/card_secret.rs:176` and the bulk status path.

**I-12. The wallet over-deduct message is wrong.**
- **Status:** FIXED — admin adjust maps to `error.wallet_insufficient_balance` ("钱包余额不足"). Tests: `wallet_account.rs::wal_02_admin_adjust_rules_and_audit`, `wallet::account::tests::wal_02_admin_adjust` (QA-A12).
- **Actual:** deducting more than the balance says 支付金额不匹配. It should say the balance is insufficient.
- **Orig:** yes.
- **Where:** the wallet adjust error mapping in `backend/crates/app/src/wallet/`.

**I-13. A member level still assigned to users can be deleted.**
- **Status:** FIXED (deviates from original) — delete refused while users hold the level (`error.member_level_in_use`). Test: `marketing_member_level.rs::qa_a13_level_in_use_cannot_be_deleted` (QA-A13).
- **Actual:** the user keeps a `member_level_id` pointing at the deleted level.
- **Expected:** refuse the delete, or reassign those users to the default level.
- **Where:** `backend/crates/app/src/marketing/member_level*`.

**I-14. Custom roles without `GET:/admin/authz/me` can't log in.**
- **Status:** FIXED — self-service routes (`authz/me`, `compliance/status`, own 2FA, own password) are allowed for every authenticated admin. Test: `identity_rbac.rs::qa_a14_self_service_routes_need_no_grant` (QA-A14).
- **Repro:** a role granted only `GET:/admin/orders` (B-042).
- **Actual:** login returns a token, then `/admin/authz/me` returns 403 and the user stays on /login with a generic 无权限访问 toast.
- **Expected:** `/authz/me` is implicitly allowed for every authenticated admin, or the permission catalog grants it with any role.
- **Orig:** yes.

**I-15. Media upload errors are hard-coded Chinese in every locale.**
- **Status:** FIXED — upload errors are `error.upload_*` keys with arguments (zh-CN keeps the original text); the admin toast lists each failed file with its reason. Tests: `content_media.rs::upl_05_validation_errors`, `media::tests::upl_05_validation_messages`, admin `useMedia.test.ts` (QA-A15).
- **Example:** "文件扩展名不被允许: .php" in the en-US UI.
- **Where:** `backend/crates/app/src/content/media.rs:133-166`.
- **Related:** the upload failure toast reads "Upload failed: 1", with the count passed as the message (`admin/src/views/content/useMedia.ts:91`).

**I-16. The backup procedure doesn't fit the Docker deployment (O-003).**
- **Status:** FIXED — built-in `zebra-store backup [--output FILE]` (SQLite `VACUUM INTO`, online, never overwrites); handbook `deploy/backup-upgrade`, `deploy/docker`, `parts/backup-short` and TEST_FLOWS O-003 document it plus `mysqldump`/`pg_dump`. Test: `infra tests/backup.rs::qa_a16_sqlite_online_backup` (QA-A16).
- **Actual:** the handbook `/deploy/backup-upgrade` and TEST_FLOWS O-003 assume `sqlite3` on the host and a `/opt/zebra/data` path. The lab/Docker deployment has neither `sqlite3` (host or image) nor a host path; the data is in the `zebra-lab_store_data` volume.
- **Fix:** document a Docker variant, e.g. `docker run --rm -v zebra-lab_store_data:/d alpine sh -c 'apk add sqlite && sqlite3 /d/zebra.db ".backup /d/bk.db"'`, or ship a `zebra-store admin backup` command (`VACUUM INTO`).

### P3

**I-17. Admin UI isn't filtered by permission for restricted roles.**
- **Status:** FIXED (partly) — order-list status/complete/fulfil actions and dashboard quick links gated by permission; `order_config` not fetched without `GET:/admin/settings`. Dashboard API calls are not gated (built-in roles all have them). Tests: admin `orderUtils.test.ts`, `dashboardUtils.test.ts` "QA-A17".
- The dashboard shows quick links the role can't open, and 403 banners.
- The order list shows 更新状态 / 标记完成 to a read-only role.
- `settings?key=order_config` fires a 403 toast.
- Orig: yes.

**I-18. Banner links accept `javascript:` URLs.** Not executed through the storefront button in Chromium, but they should be refused.
- **Status:** FIXED — external links must be `http(s)://`, internal links site-relative (no scheme, no `//`). Test: `banner::tests::rejects_invalid_links_and_windows` (QA-A18).

**I-19. Generic or raw validation messages.**
- **Status:** PARTIAL — `error.email_service_disabled` is now translated (QA-A07). WONTFIX for now: the "… config invalid: detail" texts are the original validation messages, and the generic coupon / channel / product-dialog messages are UX work beyond this fix round.
- **Raw English backend text in toasts:**
  - "order email template config invalid"
  - "captcha config invalid: …"
  - `error.email_service_disabled`
  - "notification config invalid" (without naming the missing bot token)
- **Generic "Invalid coupon":** shown for both percent > 100 and end before start.
- **Payment channels:** only 名称 gets a field-level error; other fields share one toast.
- **Product dialog:** an empty submit gives only the toast "Please select a category".

**I-20. Invalid input is silently normalised while the UI says 保存成功.**
- **Status:** PARTIAL — member level discount must be 0–100 % and thresholds ≥ 0; announcement end before start is refused. Tests: `marketing_member_level.rs::qa_a20_level_values_are_range_checked`, `schema::tests::qa_a20_announcement_window` (QA-A20). WONTFIX: clamping of timeout/interval/port, blacklist/nav item dropping, empty allowlist and 2-decimal rounding are the original normalisation rules (rejecting them would break saving legacy values through the Basic tab).
- Payment timeout −5 → 1.
- Upstream interval 0 → 5.
- SMTP port 0 → 587.
- IP blacklist entry `not-an-ip` is dropped.
- An internal nav item with an absolute URL is dropped.
- Announcement end before start, member discount 120% and negative values, and an empty domain allowlist are all accepted.
- A wallet adjustment of 1.234 is rounded to 1.23 without warning.

**I-21. The dashboard "自定义" default range is off by one day in UTC+ timezones.**
- **Status:** FIXED — default range uses the local calendar day. Test: admin `dashboardUtils.test.ts` "QA-A21".
- **Cause:** `defaultCustomRange` uses `toISOString()`, so between 00:00 and 08:00 SGT it ends *yesterday*.
- **Observed:** it showed 09/19–09/25 with 0 orders, while "最近7天" is 09/20–09/26 with 21. `dash-range-3.png`
- **Where:** `admin/src/views/dashboard/dashboardUtils.ts:45-48`.
- **Orig:** same bug in `Dashboard.vue:308`.

**I-22. The audit-log role filter needs the `role:` prefix,** although the UI hides the prefix everywhere else. Orig: yes.
- **Status:** FIXED — a bare role name gets the `role:` prefix. Test: admin `authzUtils.test.ts` "QA-A22".

**I-23. Catalog and marketing small items.**
- **Status:** PARTIAL — en-US colons (`format.test.ts` "QA-A23") and the draft button label (`contentUtils.test.ts` "QA-A23") fixed. WONTFIX: list price of last SKU (original), coupon case-sensitivity, deleted product ids, CSV file name, inactive categories in selects, wording, batch counts and mobile tab cut-off (cosmetic / original behaviour).
- The storefront list price shows the last SKU's price because `sort_order` = index; for example 25.00 on the storefront vs 10.00 in admin. Orig: yes.
- Coupon codes are unique case-sensitively only (`QAC0926` and `qac0926` both saved).
- Coupons keep a deleted product id, shown as "#13".
- The en-US coupon list uses full-width colons ("Min amount：").
- CSV card import accepts any file name and content (`evil.php`).
- Inactive categories are still offered in product selects.
- Active/Inactive vs Listed/Unlisted wording is inconsistent.
- The post button says "立即发布" when saving a draft.
- "Batch created" shows no counts.
- On mobile, the product edit dialog cuts off the en-US tab.

**I-24. Container logs contain ANSI colour escapes.**
- **Status:** FIXED — ANSI only when stdout is a TTY and `NO_COLOR` is unset. Test: `zs-server tests::qa_a24_no_ansi_without_terminal`.
- `log.json: false` plus `tracing_subscriber::fmt()` without `.with_ansi(false)` or a TTY check writes `\x1b[2m…` into `docker logs`, which breaks log collectors.
- Where: `backend/crates/server/src/main.rs:35`.

**I-25. Procurement logs `WARN unknown upstream status status="paid"`** for dujiao-next upstream orders (procurement 4 and 6). They are still fulfilled a second later, so the DJ status mapping is simply missing `paid`.
- **Status:** WONTFIX here — procurement/integration scope, left to the integration fixer.
- Where: `backend/crates/app/src/integration/procurement*`.
- Out of admin scope; noted from the O-002 logs.

**I-26. A restored DB copy immediately runs upstream sync jobs against the real upstreams.** Worth a note in the restore docs: pause the queue, or change `site_connections`, when restoring into staging.
- **Status:** FIXED (docs) — restore warning added to handbook `deploy/backup-upgrade`.

**I-27. List-valued config can't be overridden from env.** `ZS__SERVER__TRUSTED_PROXIES=10.0.0.0/8` fails with "invalid type: string, expected a sequence". Document it, or accept comma-separated values.
- **Status:** FIXED — all list-valued keys accept comma-separated env values; handbook `deploy/env` updated. Test: `zs-server settings::tests::qa_a27_list_values_from_env`.

**I-28. Handbook problems.**
- **Status:** FIXED (docs) — the three missing `bt-*.png` references are commented out until captured (SCREENSHOTS.md lists them); the "检测更新" text is accurate again after I-5.
- `/deploy/bt-panel` references 3 missing screenshots (`/screenshots/deploy/bt-*.png`, served as the 404 HTML).
- `/deploy/backup-upgrade` describes a working "系统更新" message that doesn't exist (see I-5).

**I-29. Layout and visual nits.**
- **Status:** PARTIAL — completed vs refunded badges now differ (admin `orderUtils.test.ts` "QA-A29"). WONTFIX: sticky-bar translucency, mobile sticky column, child profit after refund (cosmetic).
- The translucent sticky top bar lets content show through over the breadcrumb (`AdminLayout.tsx`).
- On mobile, the sticky action column of the order list hides amount and status.
- The completed and refunded order badges share the same pink warning colour.
- Child-order profit isn't recalculated after a refund.

**I-30. Test-flow and contract doc mismatches.**
- **Status:** PARTIAL — TEST_FLOWS B-032 corrected. WONTFIX: media list shape, 403 for disabled channel clients and the Basic-tab multi-key save match the original contract.
- TEST_FLOWS B-032 ("旧路径仍可用") contradicts both the code and the original.
- `GET /admin/media` returns `data:{items,total}` instead of `data[]` + `pagination`.
- A disabled channel client returns 403 while other auth failures return a uniform 401.
- Basic-tab save also rewrites `registration_config` and `order_config`.

## Left on the lab

- **Admins** (non-super, reusable for RBAC tests):
  - `qa-auditor` / `QaAudit#2026pass` (readonly_auditor)
  - `qa-admin` / `QaAdmin#2026cli` (role `qa-orders-viewer`, 2FA off)
- **Storefront users:** `qa-user-d1790358804@lab.test` (#8) and one `qa-user-d2-*` affiliate buyer, with their orders #56 and #58 and 3 refund records.
- **Stock:** 2 units of `chatgpt-team` were used; `lab.sh provision` refills them.
- **Settings:** the settings key `foo_bar_unknown` (value `{}`). `nav_config`, `order_config`, `upstream_sync_config` and `dashboard_config` were restored as their explicit default values instead of `{}`, which behaves the same. `telegram-bot.config_version` is now 2.
