# Live QA: 对接 + 分站 (dot2.com lab)

> 2026-09-27 状态更新：本文保留 2026-09-26 线上复测时的原始结果。P1-1～P1-4 及相关 P2 问题已在后续实现中修复，并由 `regression_live_reseller.rs`、`regression_live_integration.rs` 等自动化用例覆盖；当前完成状态以 `docs/TODO.md` 和 `docs/reference/regression-coverage.md` 为准。

- **Date**: 2026-09-26 (lab clock 2026-09-25 17:50–19:16 UTC). Lab was freshly reset about 10 min before the run. Other testers were active at the same time: order ids jumped and other people's ledger rows appeared.
- **Scope**: TEST_FLOWS.md §3 (I-001…I-081) and §4 (R-001…R-022).
- **How**: real UIs through Playwright (store/zs2 admin + storefront, reseller console), plus the public APIs where no UI exists (zebra-store protocol, `/shared/*`, `/plugin/open-api/*`, acg-faka admin API).
- **Scripts**: `e2e/live/integration/*.mjs` (helpers in `lib.mjs`). The evidence journal is `e2e/live/integration/.state/journal.log`; it is gitignored because it holds tokens.
- **Screenshots**: `e2e/live/shots/integration/`, `e2e/live/shots/reseller/`.
- **Own fixtures**, so shared lab data was not touched:
  - store: `qa-int-a` (5.00, prefix `QA-INT-A-`) and `qa-int-b` (3.00 → 3.40).
  - users: `qa-z2@`, `qa-sbuyer@`, `qa-acgdown@`, `qa-cred2@`, `qa-rsl@`, `qa-rsl2@`, all `@lab.test`.
  - zs2: connection #2 "QA 主站 (zebra-store)", buyer `qa-buyer@lab.test`.
  - acg: member `qaintsup` with 3 commodities, 萌次元 shop #2, member `qaacgbuyer`.
  - dujiao: user `qa-dj@lab.test`, product `qa-dj-card`, connection "QA Zebra 主站".
  - store connections #3 "QA 异次元" and #4 "QA 独角".
  - reseller `qa-rsl` (R#4, subdomain `qaint`).
- **Lab limits**:
  - The edge only routes the 7 known hosts. The new reseller host `qaint.dot2.com` was therefore checked with read-only `GET`s from inside `zebra-lab-edge-1` to `store:8081` using a `Host:` header.
  - `settlement_confirm_days=0`, so the 7-day window cannot be observed.
  - "Network failure" was simulated by pointing a connection at `https://docs.dot2.com`, a public static site that answers HTML. No container was stopped.
- **State left behind**:
  - Every setting I changed was restored: sakura `settlement_status`, `qaint` primary domain, the qa-rsl site config (reset), ACG card locks, `qa-int-b` stock and the connection base URLs.
  - `upstream_sync_config` was written explicitly with its existing values (`pre_order_stock_check_enabled` was already `true`).
  - sakura's available profit changed: 0.30 was withdrawn and paid, and 0.22 went back to available.

## Flow results

PASS = matches TEST_FLOWS expectation · PARTIAL = works with deviations · FAIL = expectation not met · N/A = not executable on the live lab.

### 3.1 S ← ACG (acg-faka)

| ID | Result | Evidence |
|---|---|---|
| I-001 | PASS | SA UI new connection (acg-faka): handshake returns "异次元 ACG Lab", balance 100.00 CNY, capabilities `[categories]`, status active. `I001-0*.png` |
| I-002 | PASS | UI import of 3 commodities: 1 SKU each, cost = ACG member price (7 / 6 / 4), ×1.2 markup, imported inactive. Widget regex copied into `manual_form_schema`. `I-002-import-*.png` |
| I-003 | PASS | Markup 20→25 % then "重新应用加价" → `updated_products:3`; 7.50→9.38 and 6.00→7.50 |
| I-004 | PASS | UI buy: paid 8.40, card `QA-ACG-0001` in about 3 s. ACG `qaintsup` balance 100→93 (exactly 7.00). `I-004-*.png` |
| I-005 | PASS | ACG price 8→9 / member 7→7.5 and +5 cards → local cost 7.50, price 9.00, stock 24 (20−1+5). `I-005-mappings-list.png`. The list shows prices as `9` / `4.8` (P3-7) |
| I-006 | PASS | ACG balance 0 → procurement `rejected` "余额不足", not retried. After recharging, the UI "重试" gave `fulfilled`: ACG debited exactly 7.50 and no double charge. `I-006-proc-*.png` |
| I-007 | PARTIAL | After sync: stock 0, storefront sold out, order blocked. `I007-storefront-soldout.png`. **Before** the next sync (up to 5 min) the pre-order check trusts cached stock: buyer charged 9.00 twice → `rejected` 库存不足 → buyer order stays "已支付" with no refund (P1-3). The block message is the raw key `error.upstream_stock_insufficient` (P2-1) |
| I-008 | PARTIAL | With the URL pointed at an HTML site: 3 retries (30/60/120 s) then **`rejected`, not `manual_review`**. The error text still says "request may have been executed" (P2-4). After restoring the URL, UI "重试" → `fulfilled`. The ambiguous mid-trade case could not be produced live |
| I-009 | PASS | ACG manual item → procurement `accepted`; the hint text is not delivered as a card. After manual delivery in ACG admin, store picked it up in about 15 s (`fulfilled`). The admin cannot see ACG's hint text (P3-5) |
| I-010 | **FAIL** | Server-side regex check works (API with `abc` → 人工交付表单字段值不合法, no charge; `123456` → fulfilled). But the storefront **never shows the widget field** for this item, so every UI purchase fails with "请填写完整的人工交付信息" (P1-2). `I-010ui-b-checkout.png`, `I-010ui-c-after-submit.png` |
| I-011 | PARTIAL | Delisting in ACG + UI 同步 → badge "上游已下架", local product delisted. `I-011-mapping-after-sync.png`. **Deleting** in ACG still shows 上游已下架 / `inactive`, never 上游已删除 (P3-4) |

### 3.2 / 3.3 ACG ← S (`/shared`, 萌次元)

| ID | Result | Evidence |
|---|---|---|
| I-020 | PASS | UI "生成对接密钥" → app_id 15, 32-char key, site_url and protocols shown. `I020-01-compat-key.png` |
| I-021 | PASS* | The provisioned 异次元 shop #1 was used (balance 5000). A second 异次元 shop with my key was impossible because ACG normalises the URL (see I-030) |
| I-022 | PASS | `items` lists only auto-delivery own products (the manual `chatgpt-team` and upstream-sourced products are excluded). Multi-SKU import keeps race names `月卡` / `季卡` (zh-CN) |
| I-023 | PASS | ACG member bought imported `qa-int-b` → card `QA-INT-B-0005` synchronously. S wallet of acg-down 4970.00 → 4966.60 (exactly 3.40) |
| I-024 | PASS | Replaying the same `request_no` returns the same `tradeNo` and card; `stock` is a string `"24"`. Wallet 56.00→52.60→52.60 |
| I-025 | PASS | Wrong sign, `0e…` magic sign and tampered `num` → 密钥错误. `app_id[]=1` and unknown app_id → 商户ID不存在. Never 500 |
| I-026 | PARTIAL | Wallet 0 → 余额不足, no order. The zero-price case cannot be staged: admin refuses price 0 (fine) |
| I-027 | N/A | 340 `connect` calls took 82 s (about 4 req/s), so 300/min can't be reached from outside. This exposed P2-6: `/shared` requests are serialised |
| I-028 | PASS | Importing manual product code 2 → "该商品未开放对接" |
| I-030 | PASS | 萌次元 shop connected (name, balance 50). **The documented workaround (trailing `/` or upper case) does not work**: ACG 3.7 normalises both to "该店铺地址已经存在". An explicit port `https://store.dot2.com:443` works (P3-9) |
| I-031 | PASS | Via 萌次元: `ChatGPT Plus` race 月卡 → card returned. S wallet 200.00→61.00 (139.00 = store price) and `qa-int-a` 61.00→56.00. Single-SKU items import with race `DEFAULT`, which the buyer must pick (P3-8) |
| I-032 | N/A | No mcy-shop in the lab |

### 3.4 S ← DJ (dujiao-next) and DJ ← S

| ID | Result | Evidence |
|---|---|---|
| I-040 | PASS | DJ UI "申请开通" → pending; approved in DJA and secret generated. `I040-*.png` |
| I-041 | PASS | SA UI: test connection → site "Dujiao-Next Lab", balance 100, callback prefilled, then Ping OK. `I041-*.png` |
| I-042 | PASS | UI import → price 8.40 (7×1.2), imported inactive |
| I-043 | PASS | UI buy: card `QA-DJ-0001` delivered in 3 s (callback); DJ wallet 100.00→93.00 |
| I-044 | PASS | Forged callbacks (no headers / bad signature / key of another connection) are rejected; procurement #8 unchanged |
| I-045 | PARTIAL | UI job (full, today) → total 4, matched 3, mismatched 1, resolved via "标记处理". The mismatch is a false positive: an ACG manual order already delivered. `upstream_status` is **truncated to `awaiting_manual_chec`** and `errors:1` is not explained (P2-7). `I045-*.png` |
| I-046 | PASS | DJ admin created a dujiao-next connection to S (ping: balance 20) and imported `qa-int-b`. A DJ buyer got `QA-INT-B-0007` in 3 s; S wallet 20.00→16.60. S also exposes its own upstream-sourced products (DJ/ACG) to DJ, which allows loops (P3-10) |

### 3.5 Z2 ← S (zebra-store)

| ID | Result | Evidence |
|---|---|---|
| I-060 | PASS | UI "生成连接码" → `zsc1_…` shown once plus the rotation banner "旧 Secret 将于 … 失效". `I060-02-code-shown.png` |
| I-061 | PARTIAL | Pasting the code in Z2A autofills URL, key, secret and protocol. The handshake shows site, CNY, 7 capabilities and rate 1 (auto-applied). **The callback URL is only suggested ("应用" button) and is not auto-applied**, so the saved connection has `callback_url=""` and `webhook_status=none` (P2-2). `auto_sync_price` defaults to 否. `I061-02-parsed-prefilled.png`, `I061-04-created.png` |
| I-062 | PASS | Unreachable code URL → handshake error shown, connection saved `pending`, webhook `failed`. The list shows "注册失败" (webhook) rather than a handshake error (P3). `I062-*.png` |
| I-063 | PASS (after edit) | After setting the callback in the edit dialog → `registered`; `GET /zs/webhooks` shows the zs2 events URL |
| I-064 | PARTIAL | Imports OK (5.50 / 3.30). `last_change_seq` stays **0** right after the import and is only set later (32 after the first handshake-triggering edit) |
| I-065 | PASS / note | **With webhook**: price change → Z2 cost after **13.4 s**, stock +5 after **6.8 s**, delisting after **9.8 s**. Re-listing updates the mapping to active in about 10 s but the local product stays delisted (manual re-list). **Without webhook** (default after pasting a code): first sync after about 7 min (one 5-min run was lost to 429). Stock updated but cost stayed 3.00 while upstream was 3.20 because `auto_sync_price=false` (P2-3) |
| I-066 | N/A | Needs a cursor older than 7 days |
| I-067 | PASS | Quote 5.00 → S raised the price to 6.00 → order charged 5.00. Quote 6.00 → S lowered to 4.50 → charged 4.50. Quantity differing from the quote → 422 `quote_mismatch`. Quote older than 10 min → 409 `quote_expired` |
| I-068 | **FAIL** | Direct API multi-item works: one order, 2 lines, total 11.40. **Z2's own 2-product order produced two separate S orders** (procurements #4 and #5, upstream `…846673683` and `…846296032`), although `multi_item` was negotiated (P2-5). Money correct: S wallet 75.90→67.50 (5.00+3.40). `I068-*.png` |
| I-069 | PASS | Same key → same `order_no`, wallet unchanged. Different body → 422 `idempotency_conflict`. Same `downstream_order_no` with a new key → original order. Missing key → 400. Cancelling a paid order → 409 `order_not_cancelable` |
| I-070 | PASS | `delivery` = `{encrypted:true, alg:A256GCM}`; decrypted with SHA256("zs-delivery:"+secret) to `QA-INT-A-0001`. The create response returns `status:paid, delivery:null`; delivery arrives on GET or webhook. Z2 buyer sees `QA-INT-A-0004` / `QA-INT-B-0004` |
| I-071 | PASS | New code: old and new secrets both 200. After the first new-secret request the old one gets 401 and Z2 ping fails with 401 (surfaced as `status_code 500`, P3-2). After updating the secret in Z2A the ping is OK and pushes still arrive (price → Z2 in 6.9 s) |
| I-072 | PASS | Replayed nonce, −400 s timestamp, bad signature and short nonce → uniform 401 `unauthorized`. 130 burst requests → 98×200 then 429 `rate_limited` with `Retry-After: 29` |
| I-073 | PASS | S wallet 1.00: quote `sufficient_balance:false`, direct order 402. Z2 order → procurement `rejected` `insufficient_balance` (raw code shown in the UI, P3-1). Top-up + UI 重试 → `fulfilled`, wallet 50.00→45.00 |
| I-074 | PASS | S cards emptied → Z2 stock 0 in 10.2 s, product page 售罄 with buttons disabled. Quote gives `reason: out_of_stock`; order → 422 `item_unavailable`. Z2 checkout refused with the raw `error.upstream_stock_insufficient` (P2-1). `I074-01-z2-soldout.png` |
| I-075 | PARTIAL | S "unreachable": the quote fails 3× then **`rejected`** (not `manual_review`) with the text "request may have been executed". After restoring, UI 重试 → `fulfilled` exactly once (wallet 45→40) |
| I-076 | PARTIAL | Retry works in the UI. **Rejected rows have no 取消 button** (`canCancelProcurement` excludes `rejected`). A cancel via the API leaves the buyer order "已支付" with no refund and no marker in the SA order list (P1-3). `I076-sa-order-list-stuck.png` |
| I-077 | PARTIAL | Handshake refuses 127.0.0.1, ::1, localhost and 172.30.77.1. Redirect (308) is not followed. But **saving** a connection to `http://127.0.0.1:8081` succeeds (P3-3) |
| I-078 | PASS | Editing a mapped product: 交付方式 and form are locked ("对接商品交付类型由上游管理"), 7 disabled controls. `I078-01-edit-mapped-product.png` |

### 3.6 API credentials

| ID | Result | Evidence |
|---|---|---|
| I-080 | PASS | UI reject with reason → user sees `rejected` and the reason. Reapply → pending → approve → handshake 200. Admin disable → 403; user self-disable → 403. `I080-*.png` |
| I-081 | PASS* | A pending key gives 401 instead of 403, because a pending credential has no secret to sign with |

### 4 分站

| ID | Result | Evidence |
|---|---|---|
| R-001 | PASS | UI apply → `pending_review`. `/reseller` on sakura redirects silently to `/me/orders` (console main-site only). `R001-*.png` |
| R-002 | PASS | SA UI approve with 10 % / 50 % → active. Every row shows 通过/拒绝/禁用/恢复 regardless of status (P3-6). `R002-*.png` |
| R-003 | PASS | Reject with reason → the user sees the reason → reapply → pending. Disable/restore covered in R-020 |
| R-004 | PASS | Detail → 域名 → 系统二级域名 `qaint` → verified and primary. The backend resolves `qaint.dot2.com` (tenant `reseller`); the lab edge cannot route new hosts. `R004-*.png` |
| R-005 | PASS | Custom domain `qa-shop.example.com` submitted → admin approve (sets `verified` without any DNS proof) → set primary → restored `qaint` |
| R-006 | PASS | Unknown host → 404. Forged `X-Forwarded-Host` / `Forwarded` from the internet → main shop |
| R-007 | PASS | Console site form: name "QA 测试分店 · 蓝莓", logo and announcement → public config for qaint shows them; main site unchanged. `R007-*.png` |
| R-008 | PASS | sakura, neon and matcha have distinct `<title>`, favicon and announcement. No coupon box and no 分销中心 entry on subsites. `R008-*.png` |
| R-009 | PASS | Preview 15 % → 5.98 (5.20×1.15). 60 % → `markup_exceeded`, save refused. −10 % and fixed 4.00 (below base) → `price_invalid`. Fixed 9.00 (>50 %) → refused. Fixed markup +1 → 6.20 OK |
| R-010 | **FAIL** | `is_listed=false` saved for qa-int-b, but `GET /public/products` and `/public/products/qa-int-b` on the qaint host still return it (P1-1) |
| R-011 | **FAIL** | Order on sakura (UI) charged **5.72** (5.20×1.10, correct) but **the sakura product page and list show 5.20** (`R011-a-product.png` vs `R011-b-checkout.png`), P1-1. The order is **not visible on the main site** (`GET /orders/{no}` on the store host → 订单不存在; missing from the main list) although the spec says both hosts show it (P2-8) |
| R-012 | PASS | Reseller detail: profit 0.52, base 5.20, buyer `q***@lab.test`, domain sakura. The neon reseller gets 404 for it. `R012-order-detail.png` |
| R-013 | PASS (days=0) | Ledger `order_profit 0.52` `available` immediately (confirm days 0). The reseller order shows `profit_status:"pending"` at the same moment (inconsistent label) |
| R-014 | **FAIL** | Refunded 2.86 of 5.72 (child #114) to the wallet: buyer 129.48→132.34. **No `refund_deduct`**: sakura keeps 0.52 available, the finance overview shows `refund_deducted 0.00`, and the parent order `refunded_amount 0.00` (P1-4) |
| R-015 | PASS | reseller-sakura's own order on sakura → no ledger entry. Its console still shows profit 0.52 (`unavailable`) for it (P3) |
| R-016 | PASS | 1.00 > available → 可提现余额不足. 3 concurrent 0.30 → 1 success + 2 refused (available 0.22 / locked 0.30). UI withdraw 0.22 (double confirm) → locked 0.52 |
| R-017 | PASS | Admin 已打款 #10 → `paid`/withdrawn. 拒绝 #11 with reason (dialog + second confirm) → 0.22 back to available. `R017-*.png` |
| R-018 | PASS | Finance overview: profit 24.52 = withdraw_paid 22.80 + available 1.72, and the ledger sums agree. `R018-*.png` |
| R-019 | PASS | Admin PUT of the qa-rsl site config → qaint shows "QA 管理员代改名"; reset → the main brand shows |
| R-020 | PASS | Disable → qaint 404, console readable, writes → "无权限访问". Restore → 200 again. `R020-*.png` |
| R-021 | PASS | sakura frozen → withdraw "当前结算状态暂不可提现"; restored to normal |
| R-022 | N/A | Needs a config change and restart |

## Money trail (all exact)

**qa-z2 wallet on S**: 100.00 → 75.90 → 67.50 → 1.00 → 50.00 → 45.00 → 40.00
- Direct orders took it to 75.90: 5.00 + 4.50 + 3.20 + 11.40.
- Z2 multi-product order took it to 67.50: 5.00 + 3.40.
- Set to 1.00 for the balance test, topped up to 50.00.
- The I-073 retry and the I-075 retry each took 5.00, down to 40.00.

**Z2 buyer**: 100 → 85.26 (9.24 + 5.50).

**Store buyer `qa-sbuyer`**: 200 → 176.00 → 144.20 → 135.20 → 129.48 → +2.86 refund = 132.34
- ACG / DJ / manual orders: 8.40 + 8.40 + 7.20, down to 176.00.
- Regex item 4.80 and ACG items 9 + 9 + 9 (I-006, I-007a, I-007b), down to 144.20.
- I-008 order 9, down to 135.20.
- sakura order 5.72, down to 129.48.
- **18.00 (I-007a and I-007b) was paid for goods never delivered and is still unrefunded.**

**Other accounts**:
- ACG `qaintsup`: 100 → 93 → 87 → 83 → 0 → 100 → 92.5 → 85.
- DJ `qa-dj`: 100 → 93.
- S `qa-acgdown`: 200 → 61 → 56 → 52.60 (the replay was free).
- S `acg-down`: 4970.00 → 4966.60.
- S `qa-cred2`: 20.00 → 16.60.

## Issues

### P1

**P1-1: Subsite product list and detail ignore the reseller overlay (price and hidden flag).**
- Repro:
  1. On sakura (default markup 10 %) open `qa-int-a`: the page shows 5.20.
  2. Checkout shows and charges 5.72.
  3. As qa-rsl, hide `qa-int-b`; it is still returned on the qaint host.
- Expected: list and detail show the reseller price and omit hidden products.
- Actual: base price shown, hidden products listed; only checkout applies the overlay.
- Evidence: `shots/reseller/R011-a-product.png`, `R011-b-checkout.png`.
- Suspect: `backend/crates/api/src/routes/catalog/product.rs` — `public_products` / `public_product` take no `Extension<Tenant>` and call `list_public` / `get_public` without the reseller pricing overlay.

**P1-2: Upstream (acg-faka) products with an input widget cannot be bought in the storefront.**
- Repro: import an ACG commodity with a `widget` (regex) and buy it in the storefront.
- Expected: the checkout shows the widget field (ACG-06).
- Actual: the checkout shows no field and the order fails with "请填写完整的人工交付信息". The API works when `manual_form_data` is sent.
- Evidence: `shots/integration/I-010ui-b-checkout.png`, `I-010ui-c-after-submit.png`.
- Suspect: `storefront/src/utils/manualForm.ts` `buildManualFormProducts` only collects forms when `fulfillmentType` is `manual`/`upstream`, but the imported product is `auto`. Either the import should keep `upstream` or the UI should use the schema whenever it has fields. The custom ACG `error` text ("账号必须是5-10位数字") is also dropped.

**P1-3: A rejected or canceled procurement leaves the buyer's paid order in limbo.**
- Repro: ACG stock empties between syncs, or insufficient balance with no retry. The buyer is charged and the procurement ends `rejected`.
- Expected:
  - the order is flagged for the admin;
  - 取消 is offered;
  - canceling rolls back or refunds per rule.
- Actual:
  - the buyer order stays "已支付" (buyer sees 已支付/处理中);
  - the SA order list shows no warning (`I076-sa-order-list-stuck.png`);
  - the procurement page offers only 重试 for `rejected`;
  - canceling via the API sets `canceled` but the local order is unchanged and there is no refund.
- The pre-order stock guard trusts a cache up to 5 minutes stale for acg-faka, which makes this common: 2 of my orders (18.00) are stuck.
- Suspect:
  - `admin/src/views/integration/integrationUtils.ts:514` (`canCancelProcurement`);
  - `app/src/integration/procurement.rs` `reject`/`rollback` and cancel path;
  - `app/src/integration/mapping.rs:846` `ensure_upstream_stock` (cache-first).

**P1-4: A partial refund of a reseller order does not claw back profit.**
- Repro: sakura order 5.72 (profit 0.52 available), then SA refund-to-wallet of 2.86 on child #114.
- Expected: a `refund_deduct` of 0.26.
- Actual:
  - no ledger row, and 0.30 of that profit was later withdrawn and paid;
  - finance shows `refund_deducted 0.00`;
  - the parent order `refunded_amount` stays 0.00 while the child shows 2.86.
- Suspect: the reseller accounting refund hook keys by the child order id while the profit is posted on the parent (`order_id=113`) — `app/src/reseller/*accounting*`, the order refund lifecycle.

### P2

**P2-1: Untranslated error keys reach users.**
- `error.upstream_stock_insufficient` is returned as `msg` to the Z2 and S storefronts.
- About 40 keys used in code are missing from `backend/crates/api/src/i18n/messages.json`, including `procurement_*`, `reconciliation_*`, `connection_*`, `duplicate_downstream_order` and `member_level_*` (full list from grepping `"error.*"` in domain/app/api).

**P2-2: The connection-code flow does not register the webhook.**
- The handshake panel suggests `https://zs2.dot2.com/api/v1/zs/events` but only fills it on "应用", while the exchange rate is auto-applied.
- A plain paste + 创建 gives `webhook_status=none` and the connection silently degrades to 5-min polling.
- Protocol doc §3 says the callback is auto-filled.
- Suspect: admin `SiteConnections` form, handshake apply logic.

**P2-3: Default `auto_sync_price=否` leaves cost and sale price stale on zebra-store and dujiao connections.**
- An upstream rise from 3.00 to 3.20 left Z2 with cost 3.00 and price 3.30, so Z2 sells at stale margins and the procurement card shows a stale 本地成本/利润.
- There is no warning when the upstream price exceeds the local sale price.
- Suspect: `domain/src/integration/mapping.rs:519` `plan_sync` — cost is not updated when auto-sync is off.

**P2-4: Network failure ends `rejected`, not `manual_review`, and the message contradicts the status.**
- Seen for both acg-faka and zebra-store, pointed at an HTML-answering host.
- The pre-trade valuation/quote failure is downgraded to `Transport`, so after retries the procurement is `rejected` while `error_message` reads "answer lost (request may have been executed)".
- A transport outage should read as `failed` or network-unreachable, not "rejected by upstream".
- Suspect:
  - `domain/src/integration/protocol.rs` `not_executed()` keeps the "may have been executed" text;
  - `app/src/integration/procurement.rs:536` `submit_failure` → `reject`.

**P2-5: `multi_item` is negotiated but never used by the buyer side.**
- A Z2 cart with 2 upstream products creates 2 procurements and 2 separate supplier orders.
- Suspect: procurement is per child order (`app/src/integration/procurement.rs`, `lines: vec![line]`).

**P2-6: `/shared/*` (acg-faka compat provider) requests are serialised.**
- 40 parallel `connect` calls take 11.6 s, against 0.46 s for `/api/v1/public/config`; about 290 ms each.
- An acg shop syncing many items will be slow, and the 300/min limit can't even be reached.
- Suspect: per-request SQLite writes (key `last_used_at` / rate-limit bookkeeping) in the compat provider service, called from `api/src/routes/integration/provide.rs`.

**P2-7: Reconciliation data problems.**
- `upstream_status` is truncated to 20 chars (`awaiting_manual_chec`).
- An already-delivered ACG manual order is flagged as a status mismatch.
- `upstream_amount 0.00` is compared with 6.00.
- `errors:1` in the summary is never explained in the UI.
- Suspect: `infra/src/db/entity` reconciliation item column length, and the acg-faka `query` mapping.

**P2-8: A subsite order is invisible from the main site.**
- `GET /orders/{no}` on store.dot2.com → 订单不存在, and it is missing from the main `/me/orders`.
- TEST_FLOWS R-011 says both hosts show it. Either the spec or the tenant scoping is wrong; it needs a decision.

### P3

1. The procurement UI shows raw `insufficient_balance`; `retry_count` is reset to 0 after a manual retry (history lost), and `利润` is shown on rejected rows.
2. zebra-store ping auth failure comes back as `status_code 500` ("upstream responded with status 401") instead of a business error.
3. SSRF: saving a connection to `http://127.0.0.1:8081` is accepted (the calls are blocked, but the create should refuse); ACG `extra.currency` accepted "1002".
4. An upstream ACG delete is shown as 上游已下架 (`inactive`), never 上游已删除.
5. The ACG manual-delivery hint is not stored anywhere visible to the admin.
6. Reseller profile rows show 通过/拒绝/禁用/恢复 for every status; a disable reason is stored in `reject_reason`; the withdraw reject needs two dialogs.
7. The product-mapping list shows prices without 2 decimals (`9`, `4.8`); Z2 order detail labels upstream items "人工交付" while the product page says "自动交付" (`storefront` `fulfillmentTypeLabel` has no `upstream`).
8. A single-SKU product exposed via `/shared`/萌次元 imports with race `DEFAULT`, which ACG buyers must select.
9. TEST_FLOWS I-030 workaround ("末尾加 / 或换大小写") is wrong for ACG 3.7: it normalises both. Use an explicit port `:443`.
10. The dujiao-next provider exposes upstream-sourced products (e.g. the DJ-sourced `QA DJ 卡` back to DJ), which allows supply loops; `/shared` correctly hides them.
11. After approval, `/me/api` still says "请点击下方按钮生成您的 API Secret" while showing a secret tail the user never saw.
