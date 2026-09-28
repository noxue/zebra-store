# Live QA: storefront (用户前台) on dot2.com

- **Date:** 2026-09-26, about 01:45–03:20 (UTC+8)
- **Sites:** https://store.dot2.com (S), https://sakura.dot2.com (SK), https://neon.dot2.com (NE), https://matcha.dot2.com (MA)
- **Method:** real Chromium through Playwright, in two viewports: desktop 1440×900 and mobile 390×844 (iPhone 13 profile).
  - Scripts: `e2e/live/sf-*.mjs`, with shared helpers in `e2e/live/sf-lib.mjs`.
  - Raw log: `e2e/live/storefront-log.jsonl`.
  - Screenshots: `e2e/live/shots/storefront/`.
- **Test data:** I used only my own accounts.
  - Main accounts: `qa-sf-dmuh9ewag@lab.test` (desktop) and `qa-sf-mmuh9ewag@lab.test` (mobile), each topped up with 500 through admin wallet adjust.
  - Short-lived accounts: `qa-sf-2fa*`, `qa-sf-dis*`, `qa-sf-rl*`, `qa-sf-api*`, `qa-sf-aff*`.
- **Admin-side fixtures (cleaned up by `sf-teardown.mjs`):**
  - Created, then deleted afterwards: coupons `QASF10` and `QASFNOWHL`, the promotion on `gcp-account`, member level "QA-SF 九五折", 2 blog posts, 1 notice and 1 banner.
  - Wholesale tier on `google-account`: added, then cleared.
  - Gift-card batch "QA-SF giftcards": all 3 cards were redeemed.
  - QA products `qa-sf-manual` and `qa-sf-lastcard`: set inactive at the end.
- **Temporary setting changes:** each was restored right after its test, and I checked the final state against `/public/config`.
  - `registration_config`, `captcha_config`, `affiliate_config`
  - `site_config` fields: `template_mode`, `theme.primary_color`, `contact`
  - `order_risk_control_config`, `wallet_config`
- **Payments:** the lab has no online payment channel.
  - Flows that need an external gateway were checked up to the payment step and marked **N/A-external**.
  - Every real purchase was paid with the wallet balance.
- **Concurrent testers:** the lab was being edited by other QA agents at the same time. At one point the site name and logo changed to "QA品牌测试站 QA-Brand" with a green placeholder logo. That was another agent's admin test, not a storefront bug.

## Summary

| Result | Count |
|---|---|
| PASS | 64 |
| PARTIAL | 5 |
| FAIL | 0 (in the 87 F-flows) |
| N/A (incl. N/A-external) | 18 |
| **Extra: subsite checks** | R-008 PARTIAL, R-011 **FAIL** |

**Overall:**
- No console errors, no uncaught JS errors, no 5xx responses and no broken images on any page I visited, on any of the 4 sites.
- The only 4xx responses were the expected 429s from login rate-limit tests.
- i18n works in both en-US and zh-TW. The only leftover Chinese is product and announcement **data**, which the seed never translated.
- No page scrolls horizontally at 390 px width.

**Speed:** loading to network idle usually took **2–6 s** from where I ran the tests.
- The HTML itself took about 1.2 s to start arriving (TTFB) and first paint came at about 2.1 s, so most of this is network distance to the server.
- Still, a first home load makes about 10 separate `m-plus-rounded-1c` font-subset requests plus 5 full-size product PNGs.

## Flow results

Screenshot paths are relative to `e2e/live/shots/storefront/`.

### 1.1 Browsing, search, language, theme

| ID | Result | Evidence / notes |
|---|---|---|
| F-001 | PASS | The home page shows site name, favicon, banner carousel (the mobile layout uses the mobile image), featured products and latest posts (blog + notice). Tested on all 4 sites. `F001-home-S-d.png`, `F001-home-banner-d.png`, `F001-home-banner-m.png`, `F001-home-full-d.png`, `F001-home-{SK,NE,MA}-{d,m}.png`. Issues: P3-10 (mascot overlap), P3-14 (speed). |
| F-002 | PASS | Each dismissal option works as intended. **关闭** hides the popup for the session only; it comes back in a new tab. **今日不再提示** writes `{mode:"today",date}` and **不再提示** writes `{mode:"forever"}`, both into `announcement_dismiss`. Evidence is in the log (F-002 entry). |
| F-003 | PASS | Clicking a category changes the URL to `/categories/ai-accounts` and shows only those 4 products. On mobile the categories sit in a "筛选" drawer. `F003-category-ai-d.png`, `F003-mobile-drawer-m.png`. |
| F-004 | PASS | Search is debounced (at 150 ms all 12 cards are still shown, then it filters to Claude Pro and Claude Max). With no results the page shows "未找到匹配的商品 / 清除筛选条件", and clearing restores the list. `F004-search-claude-d.png`, `F004-search-none-d.png`, `F004-after-clear-d.png`. |
| F-005 | PASS | With `template_mode=list`, the home page and `/products` show a category sidebar and grouped rows. `F005-list-home-d.png`, `F005-list-home-m.png`. |
| F-006 | PASS | The detail page shows price, stock, guest/member badge, auto/manual badge, SKU cards and rich-text content. `F006-detail-aws-account-d.png`, `F006-detail-lab-e2e-card-m.png`. |
| F-007 | PASS | The stock display modes render as follows: exact "剩余 9 件", status "有货", range "6-20 件", hidden "库存可用". `F007-stock-{exact,status,range,hidden}-d.png`. |
| F-008 | PASS | When the last card is sold, the product shows "售罄" and the buy button is disabled. An off-shelf product disappears from the list, and opening its URL directly shows "商品不存在". `F008-soldout-detail-d.png`, `F008-soldout-list-d.png`, `F008-offshelf-direct-d.png`. Issue: P3-4. |
| F-009 | PASS | The language choice is kept in localStorage `locale` and survives a reload. API requests send `X-Lang`. UI strings are fully translated. `F009-lang-zh-TW.png`, `F009-lang-en-US.png`, `I18N-*`. Issue: P3-15 (data only). |
| F-010 | PASS | The theme toggle adds `.dark` to `<html>` and stores it in `dujiao_theme`, which survives a reload. The sakura canvas does not start under `prefers-reduced-motion` (`SakuraCanvas.tsx:94`). `F010-dark-home.png`, `F010-dark-detail.png`. |
| F-011 | PASS | A primary colour of `#16a34a` reached the `--zs-primary` CSS variable and the buttons. `F005-list-home-d.png` shows the green buttons. |
| F-012 | PASS | Tested `/blog`, a blog post (its related product links to the product page), `/notice` with notice detail, `/about`, `/terms` and `/privacy` (empty content shows "暂无内容"). A draft post shows "文章不存在". `F012-blog-detail-d.png`, `F012-notice-list-d.png`, `F012-blog-draft-d.png`. |
| F-013 | PASS | Shows the 404 illustration with "返回首页" and "返回上一页". `F012_not-exist.png`. |
| F-014 | PASS | `sitemap.xml` uses `https://store.dot2.com`. It contains the published post and the notice, and leaves out the draft and the off-shelf product. `robots.txt` is correct. Evidence is in the log (F-014). |
| F-015 | PARTIAL | The mobile bottom nav and the sticky buy bar both appear. However, the sticky bar is translucent, so the footer text shows through it, and the back-to-top button covers "立即购买". `F015-sticky-bar-m.png`, `F015-scroll-1200-m.png`. Issue: P2-2. |
| F-016 | PASS | The quick-buy dialog offers SKU choice, quantity, "查看详情", "加入购物车" and "立即购买". `F016-quickbuy-d.png`, `F016-quickbuy-m.png`. |

### 1.2 Register, login, 2FA, password reset

| ID | Result | Evidence / notes |
|---|---|---|
| F-020 | PASS | The submit button stays disabled until the agreement box is ticked. After registering, the user lands on `/me/orders` with the email already verified. `F020-register-filled-m.png`, `F020-register-after-d.png`, `F020-register-after-m.png`. There is no default member level to assign on this lab. |
| F-021 | N/A | SMTP is not configured on the lab (`smtp_enabled:false`). |
| F-022 | PASS | With the allowlist on, the email field becomes a suffix dropdown (`@ lab.test`), and the API rejects gmail with "当前邮箱后缀不允许注册". `F022-register-allowlist-d.png`. |
| F-023 | PASS | With registration closed, the page shows "注册功能已关闭" and the API returns 403. `F023-register-closed-d.png`. |
| F-024 | PASS | Attempts 1–5 get "邮箱或密码错误". Attempt 6 gets "登录尝试过多，请在 900 秒后重试", and the correct password is refused while locked. The login log records `invalid_credentials`. `F024-correct-after-lock-d.png`. Issue: P3-12. |
| F-025 | PASS | With the image captcha on, the login form shows a 240 px captcha, and a wrong code gets "验证码错误或已失效". `F025-login-captcha-d.png`, `F025-login-captcha-wrong-d.png`. |
| F-026 | PASS | Setup shows a QR code plus the secret. Entering a TOTP code enables 2FA and shows 10 recovery codes once. `F026-2fa-setup-d.png`, `F026-2fa-enabled-d.png`. |
| F-027 | PASS | The TOTP challenge page appears (297 s countdown, "使用恢复码" link) and a TOTP code logs in. A recovery code logs in once; using it again gives "恢复码错误或已使用". `F027-totp-challenge-d.png`, `F027-recovery-1-result-d.png`, `F027-recovery-2-result-d.png`. I did not wait for the challenge to time out. |
| F-028 | N/A | SMTP is off, so `/auth/forgot` shows "密码重置功能已关闭". `F028-forgot-page-d.png`. Issue: P3-6. |
| F-029 | PARTIAL | **Password change passes:** the old session gets 401 "登录状态已失效" and the old password is rejected. `F029-password-changed-d.png`. **Email change is untested:** it needs SMTP codes. |
| F-030 | PASS | The login history shows time, IP, source and result. `F030-security-page-d.png`. |
| F-031 | PASS | The nickname updates in the header card. Issue: P3-7 (the language preference is saved but the UI language does not change). `F031-profile-saved-d.png`. |
| F-032 | PASS | Telegram and Google login are not configured, so their buttons are hidden. `F032-login-page-d.png`. |
| F-033 | PASS | A disabled user gets "账号已禁用" at login, and their existing token gets 401 "账号已禁用". `F033-disabled-login-d.png`. |

### 1.3 Cart, checkout, pricing

| ID | Result | Evidence / notes |
|---|---|---|
| F-040 | PASS | Adding shows the "已加入购物车" toast. The + button stops at max 5. Delete offers undo. The cart survives a reload through `cart_items`. `F040-cart-d.png`, `F040-cart-deleted-d.png`, `F040-cart-reload-m.png`. Issue: P3-1. |
| F-041 | N/A-external | Guest checkout offers the email + order-password form, but "暂无可用的支付方式" means submit stays disabled. `F041-guest-checkout-filled-d.png`, `F041-guest-checkout-m.png`. |
| F-042 | PASS | For a member-only product the button reads "登录后购买" and goes to `/auth/login?redirect=…`. `F042-member-only-d.png`, `F042-member-only-m.png`. |
| F-043 | PASS | Paying fully from the wallet makes the order "已完成" immediately, with 3 cards delivered. Wallet went 500 → 479.48, correct for 20.52. `F043-before-submit-d.png`, `F043-after-submit-d.png`. |
| F-044 | N/A-external | The UI splits correctly into 余额抵扣 500.00 + 在线支付 229.00, but there is no channel, so submit is disabled. `F044-checkout-insufficient-balance-m.png`. |
| F-045 | PASS | **Correct cases:** 10% off 22.80 gives 20.52. Below the minimum gives "未满足优惠券使用门槛". Per-user limit gives "已达到优惠券使用上限". An unknown code gives "优惠券不存在". `F045-wholesale-coupon-d.png`, `F045-per-user-limit-d.png`. **Issue P2-1:** when a coupon is rejected, the preview drops all other discounts. |
| F-046 | PASS | The detail page shows the struck-out price, "活动价" and "促销活动 单价降至79.00"; checkout shows "活动价 -10.00". `F046-detail-promo-d.png`, `F046-checkout-promo-d.png`. |
| F-047 | PASS | The 95% level gives the right member price (149 → 141.55) and a "会员优惠" line. Issue: P3-2 (labels). |
| F-048 | PASS | At quantity 2 the unit price is 9.90. At 3 it switches to 8.00 with "批发价 -5.70". `F048-wholesale-x3-d.png`. |
| F-049 | PASS | A coupon marked "不可与批发价同享" is rejected with "该优惠券不能参与批发价商品购买". `F049-coupon-no-wholesale-d.png`. The preview then shows 29.70, which is issue P2-1. |
| F-050 | PASS | The front end blocks a missing required field ("充值账号 为必填项"), a bad email and a regex failure ("角色ID 格式不正确"). The API also rejects a bad form ("人工交付表单字段值不合法") and a missing form ("请填写完整的人工交付信息"). The submitted form data appears in the order detail. `F050-empty-submit-d.png`, `F050-bad-email-d.png`, `F050-bad-regex-d.png`, `F050-manual-order-detail-d.png`. |
| F-051 | PASS | A 2-item cart creates a parent order with children `-01` and `-02`, each with its own delivery, and the cart is emptied. `F051-multi-order-detail-d.png`. |
| F-052 | PASS | The unpaid order was auto-cancelled at `expires_at + 0.1 s`. It shows "已取消", which matches the original's timeout-cancel behaviour; TEST_FLOWS expects "已过期" (see P3-13). `F052-expired-order-m.png`. |
| F-053 | PASS | Cancelling from the detail page (with a confirm dialog) gives "已取消" and makes the coupon usable again (a preview with the coupon succeeded afterwards). `F053-pending-detail-m.png`, `F053-cancel-confirm-m.png`, `F053-after-cancel-m.png`. |
| F-054 | PASS | With a limit of 2 guest pending orders per IP, orders 3 and 4 get 429 "当前网络或账号的待支付订单过多". I drove this through the API, because the guest UI cannot submit without a channel. |
| F-055 | PASS | With my IP blacklisted, UI checkout shows the toast "当前网络已被限制下单，请联系客服". `F055-blacklisted-checkout-d.png`. |
| F-056 | PASS | Two browsers raced for the last card. One got the order with `QA-SF-LAST-0001`; the other got "卡密库存不足". `F056-race-A.png`, `F056-race-B.png`. |

### 1.4 Payment

| ID | Result | Evidence / notes |
|---|---|---|
| F-060 | N/A-external | The `/pay` page renders the order, a countdown and "暂无在线支付渠道，可使用钱包余额支付"; no QR or redirect can be tested. `F060-pay-page-m.png`. |
| F-061 | N/A-external | No gateway channel on the lab. The integration tests cover this. |
| F-062 | N/A-external | Same as F-061. |
| F-063 | N/A-external | Same as F-061. |
| F-064 | N/A-external | Only one method (the wallet) exists, so there is nothing to switch between. |
| F-065 | N/A-external | No channel with a fee. |
| F-066 | PASS | With wallet-only on, "使用余额" is checked and disabled, with the note "当前仅支持钱包余额支付". `F066-wallet-only-checkout-d.png`. Issue: P2-3 (guest dead end). |
| F-067 | N/A-external | No channels. |
| F-068 | N/A | Needs real merchants, as TEST_FLOWS itself says. |

### 1.5 Order lookup and delivery

| ID | Result | Evidence / notes |
|---|---|---|
| F-070 | PASS | Guest lookup by email + password lists the order and opens its detail. "清除已保存信息" clears `guest_order_auth`. `F070-guest-found-d.png`, `F070-guest-detail-m.png`. |
| F-071 | PASS | A wrong password or unknown email shows the same "暂无订单记录", so nothing leaks. `F071-guest-wrong-pw-d.png`. Issue: P3-11. |
| F-072 | PASS | The status filter and order-number search work, and the stat cards match the list. The "余额充值订单" tab works. `F072-orders-m.png`, `F072-search-m.png`, `F072-recharge-tab-m.png`. |
| F-073 | PASS | Cards are shown one per line. "复制内容" puts the exact text on the clipboard. The download button only appears when the delivery is over 100 lines (by design). `F043-after-submit-d.png`. |
| F-074 | PASS | After the admin delivered through `/admin/fulfillments`, the buyer sees "人工交付 · 已交付" with the delivered text. `F07x-08x-order-manual-d.png`. |
| F-075 | PASS | "使用说明" HTML appears under both manual and auto products. `F07x-08x-order-manual-d.png`, `F056-race-B.png`. |
| F-076 | N/A | SMTP is off. |

### 1.6 After-sales and refunds

| ID | Result | Evidence / notes |
|---|---|---|
| F-080 | PASS | A refund of 5.00 to the wallet: balance +5.00, order "部分退款", and a refund record appears in the order detail. `F07x-08x-order-multi-d.png`, `F080-wallet-after-refunds-d.png`. |
| F-081 | PASS | A manual refund of 1.00 recorded the refund and left the balance unchanged. `F07x-08x-order-single-d.png`. The buyer view does not show the refund type, but that is admin-side information. |
| F-082 | PASS | Three parallel full refunds: exactly one succeeded and the wallet rose by 19.52 once. The order then reads "已退款". Issue: P2-5 (the others get a generic error). |
| F-083 | N/A | Needs an order older than `max_refund_days`. |
| F-084 | PASS | Contact links set in admin appear in the footer and the about page (`t.me` / `wa.me`). `F084-about-contact-d.png`, `F084-about-contact-m.png`. |

### 1.7 Wallet and gift cards

| ID | Result | Evidence / notes |
|---|---|---|
| F-090 | N/A-external | The recharge form works, but choosing a channel shows "暂无可用支付方式，请联系管理员配置支付渠道". `F090-recharge-d.png`. |
| F-091 | N/A-external | No recharge channel. |
| F-092 | PASS | Transactions show type, direction, amount, balance after, and remark. `F092-wallet-d.png`, `F080-wallet-after-refunds-d.png`. |
| F-093 | PASS | Redeeming a lower-case code gives +5.00. Redeeming again gives "礼品卡已兑换". Four parallel redeems of one card across 2 users: exactly 1 succeeded. `F093-giftcard-redeemed-d.png`, `F093-giftcard-reuse-m.png`. |
| F-094 | PASS | With the redeem captcha on, the image captcha appears and a wrong code is rejected. Without the captcha, the 4th wrong code gets 429 "请求过于频繁". `F094-giftcard-captcha-m.png`. |
| F-095 | N/A | Deliberately skipped: an auto-upgrade level would change the membership of every user on the shared lab, including buyer@ and the reseller price checks. |

### 1.8 Affiliate

| ID | Result | Evidence / notes |
|---|---|---|
| F-100 | PASS | Opening the programme gives the code `N935X8TL` and the link `/?aff=N935X8TL`. `F100-affiliate-closed-m.png`, `F104-affiliate-before-withdraw-m.png`. |
| F-101 | PASS | Two visits from the same visitor count as 1 click, and `dj_affiliate_attribution` is set. |
| F-102 | PASS | The referred visitor registered and bought a 6.00 product, which created a 0.60 commission (available immediately with `confirm_days=0`). |
| F-103 | PASS | A 50% refund cut the commission 0.60 → 0.30. `F104-affiliate-before-withdraw-m.png`. Issue: P3-8. |
| F-104 | PASS | A withdrawal of 0.30 reaches `pending_review`. Admin reject returns it to available; admin pay moves it to withdrawn (0.30). `F104-after-reject-m.png`, `F104-after-direct-m.png`. Issue: P2-5. |

### 1.9 API credentials

| ID | Result | Evidence / notes |
|---|---|---|
| F-110 | PASS | Apply → "审核中"; after admin approval the page shows the API key and secret tail. `F110-api-none-d.png`, `F110-api-pending-d.png`, `F110-api-approved-d.png`. Issue: P3-9. |
| F-111 | PASS | A rejection shows "拒绝原因：QA-SF 请补充用途说明" with "重新申请", which returns to "审核中". `F111-api-rejected-d.png`. |
| F-112 | PARTIAL | The UI confirm dialog works and the new secret is shown once while the tail changes, but I did not verify the old secret is rejected (needs a signed downstream call). `F112-api-regenerated-d.png`. |
| F-113 | PASS | Produces a `zsc1_…` code shown once, with "密钥轮换中 … 旧 Secret 将于 2026-10-03 失效". `F113-api-connection-code-d.png`. |
| F-114 | PARTIAL | The toggle gives "API 已禁用。" and `is_active=false`; I did not test that downstream calls then get 403. `F114-api-disabled-d.png`. |
| F-115 | PASS | Shows site URL, merchant ID `24` and a 32-character key; after a reload the key is masked. `F115-compat-generated-d.png`. |
| F-116 | PARTIAL | The compat toggle and IP allowlist controls are present but I did not exercise them against `/shared/*`. `F116-compat-controls-d.png`. |
| F-117 | N/A | Needs an environment change on the server (`ZS__INTEGRATION__ACG_FAKA_COMPAT=false`). |

### Extra: subsite checks (user request: "subsites show their own branding and prices")

| ID | Result | Evidence / notes |
|---|---|---|
| R-008 | PARTIAL | **Branding passes on all three subsites:** each has its own name, logo, favicon, SEO title and announcement (Sakura 樱花小铺 / NEON 霓虹卡屋 / Matcha 抹茶杂货铺). **Subsite rules pass:** there is no coupon input and no "分销中心" entry. **Prices fail:** every subsite lists the main-site base prices (see P0-1). `F001-home-{SK,NE,MA}-{d,m}.png`, `R008-SK-personal-center-d.png`. |
| R-011 | **FAIL** | A buyer on Sakura was shown "会员价 9.50 CNY / 10.00 CNY 立省 0.50" on the detail page but charged **11.50** at checkout (wallet 357.34 → 345.84). The order is also not visible on the main site (P2-4). `R011-SK-detail-d.png`, `R011-SK-checkout-d.png`, `R011-SK-order-d.png`. |

## Issues (by severity)

### P0: money, security, data loss

**P0-1: Subsite (reseller) catalogue shows base and member prices, but checkout charges the reseller's marked-up price.**
- **Page:** https://sakura.dot2.com/products/lab-e2e-card (same on neon and matcha, and in every list and card).
- **Repro:**
  1. Open the Sakura home page or `/products/lab-e2e-card`. The price shown is 10.00 (logged in with a member level: "会员价 9.50").
  2. Click 立即购买. The checkout item line and total say **11.50**.
  3. Pay with balance. The buyer is charged 11.50.
- **Expected:** the subsite catalogue (lists, cards, detail, quick-buy, cart) shows the reseller price: 10.00 × 1.15 = 11.50 on SK, ×1.25 on NE, ×1.35 on MA, and the default markup on other products. No member-price badge on a subsite, because member discounts are disabled there. Products the reseller unlisted (`is_listed=false`, R-010) should be hidden.
- **Actual:**
  - `GET /api/v1/public/products` and `/public/products/{slug}` return exactly the same data on every Host.
  - The storefront also applies the user's member level to the display price.
  - The customer sees a price 1.50–3.50 lower than they pay.
  - The subsite personal centre still shows "当前等级 QA-SF 九五折 会员折扣 9.5折".
- **Evidence:** `R011-SK-detail-d.png`, `R011-SK-checkout-d.png`, `R011-SK-order-d.png`, `F001-home-SK-d-full.png`. Admin `resellers/product-settings` shows `markup_percent` 15 / 25 / 35 for product 8.
- **Suspected code:**
  - `backend/crates/api/src/routes/catalog/product.rs`: `public_products` / `public_product` take no `TenantCtx`.
  - `zs_app::catalog::product::list_public` / `get_public` have no reseller price overlay.
  - Reseller pricing is only applied in `backend/crates/app/src/order/checkout.rs::reseller_pricing` (`apply_reseller_prices`).
  - Front end: `storefront/src/composables/useMemberPricing.ts` should not apply member pricing when `config.tenant.mode === 'reseller'`.

### P1: broken feature

None found in the storefront flows. The P0 above is also a broken feature, R-008/R-011.

### P2: wrong behaviour / UX

**P2-1: An invalid coupon wipes every other discount from the checkout preview.**
- **Page:** `/checkout`.
- **Repro:** as a 95% member, buy google-account ×3 (wholesale tier ≥3 = 8.00). The preview reads 22.80. Enter a bogus, over-limit, below-minimum or "no wholesale" coupon.
- **Expected:** the coupon line shows the error, and the member/wholesale/promotion discounts and the 22.80 total stay.
- **Actual:** 批发价 0.00, 会员优惠 disappears, and 应付金额（预估）29.70. With quantity 2 the total goes 18.82 → 19.80; the multi-item cart goes 150.96 → 158.90. It looks as if applying a coupon raises the price.
- **Evidence:** `F045-coupon-below-min-d.png`, `F045-coupon-bogus-d.png`, `F045-per-user-limit-d.png`, `F049-coupon-no-wholesale-d.png`.
- **Suspected code:** `storefront/src/composables/useCheckout.ts` lines 111–117. When the preview request fails, `preview` becomes null and every figure falls back to the raw cart total. Keep the last good preview, or re-preview without the coupon.

**P2-2: The mobile sticky purchase bar is translucent and covered by the back-to-top button.**
- **Page:** `/products/aws-account`, mobile, scrolled down.
- **Actual:** footer links ("首页 / 商品中心") show through the bar's price; the round "回到顶部" button (x=330..374, y=704..748) sits on top of "立即购买" (x=285..369, y=725..761).
- **Expected:** an opaque bar, with the back-to-top button raised above it or hidden while the bar is shown.
- **Evidence:** `F015-sticky-bar-m.png`.
- **Suspected code:** the sticky bar in `storefront/src/views/ProductDetail.tsx` and the back-to-top component (z-index/offset).

**P2-3: Guests reach a dead end in wallet-only mode.**
- **Page:** `/checkout` as a guest with `wallet_only_payment=true`.
- **Actual:** the payment section is empty ("支付方式 / 提交订单并支付") with no explanation, and submit is disabled.
- **Expected:** a clear prompt such as "本站仅支持余额支付，请登录后购买", with a login link.
- **Evidence:** `F066-wallet-only-guest-m.png`.
- **Suspected code:** `useCheckout.ts` / `views/Checkout.tsx` (guest branch).

**P2-4: A subsite order is not visible in the main-site order list.**
- **Repro:** buy on sakura.dot2.com, then open store.dot2.com `/me/orders`.
- **Actual:** the order is missing on the main site (3 orders listed), and `GET /orders/DJ20260926031117002546` on S returns 404 "订单不存在". It appears only on SK.
- **Expected per TEST_FLOWS R-011:** "买家在主站和分站都能看到".
- **Evidence:** log entry `R-011-main`.
- **Suspected code:** `backend/crates/app/src/order/query.rs::scope_of`. On the main host it scopes to `Scope::Main`, so reseller-site orders are excluded. If this matches the original, TEST_FLOWS needs correcting instead.

**P2-5: Business-rule rejections return a generic "请求参数错误" error.**
- **Affiliate withdrawal:** an amount below `min_withdraw_amount`, or above the available commission, just shows the toast "请求参数错误". The UI does not show the minimum or the available amount next to the field. After submitting, the frozen amount appears nowhere on the dashboard (available 0, withdrawn 0).
- **Refunds (admin side):** over-refunds and the losing concurrent refunds get the same generic message.
- **Evidence:** `F104-withdraw-below-min-m.png`, log F-104-debug.
- **Suspected code:** `backend/crates/domain/src/affiliate/rules.rs::validate_withdraw` (every branch returns `Error::invalid()`), `storefront/src/views/personal/*Affiliate*`.

### P3: cosmetic / minor

- **P3-1 (cart quantity):** Typing 99 into a cart quantity box leaves "99" in the input while the cart keeps 1 and the subtotal stays at 9.90. The "该商品单次下单最多可购买 5 件" hint is rendered twice. `F040-qty99-d.png`, `useCart.ts`.
- **P3-2 (price labels):**
  - The checkout labels a member price "优惠价".
  - On the detail page, the SKU cards show the base price (79.00) while the header shows the member price (75.05).
  - `F040-qty99-checkout-d.png`, `F046-detail-promo-d.png`.
- **P3-3 (order detail):**
  - The sub-order breakdown leaves out "批发价分摊", even though "共减 9.18" includes it.
  - The coupon is called "礼券金额" in 金额明细 but "优惠券" elsewhere.
  - Unpaid orders show "实付金额 21.60" and "在线支付 21.60".
  - Paid orders still show "过期时间".
  - `F043-after-submit-d.png`, `F053-pending-detail-m.png`.
- **P3-4 (off-shelf product):** Opening an off-shelf or missing product shows "商品不存在" with a pointless **重试** button, instead of the 404 page. `F008-offshelf-direct-d.png`.
- **P3-5 (register):** The register subtitle says "使用邮箱注册账号并完成验证" even when email verification is off. `F020-register-filled-m.png`.
- **P3-6 (forgot password):** The forgot-password mascot says "通过邮箱验证码即可重新设置密码" next to "密码重置功能已关闭". `F028-forgot-page-d.png`.
- **P3-7 (security and profile pages):**
  - The security page's subtitle is "通过双验证码流程完成邮箱换绑" (a section description used as the page subtitle).
  - "站点暂未开启 Telegram 登录" is printed twice per provider.
  - The email-change form is shown even though SMTP is off, so it cannot work.
  - Saving the profile language to English persists `locale=en-US` on the server, but the UI stays in Chinese.
  - `F030-security-page-d.png`, `F031-profile-saved-d.png`.
- **P3-8 (affiliate):** The affiliate "佣金记录" row shows order number "-". `F104-affiliate-before-withdraw-m.png`.
- **P3-9 (API page):** After approval, the API page shows "API Secret ••••••••938c" before the user has generated any secret, which is confusing. `F110-api-approved-d.png`.
- **P3-10 (home and product cards):**
  - The home hero mascot is drawn over the banner artwork.
  - The banner title and picture are not clickable (only the CTA is).
  - Product cards are `<article>` elements with click handlers rather than links, so there is no keyboard focus, middle-click or "open in new tab".
  - The list page has nested `<main>` landmarks.
  - The mobile "加入购物车" cart icon renders tiny.
  - `F001-home-banner-d.png`, `F015-detail-scrolled-aws-account-m.png`.
- **P3-11 (guest lookup):** Guest lookup saves email and password to storage ("已保存邮箱：…") even when the lookup finds nothing, including an unknown email. `F071-guest-wrong-pw-d.png`.
- **P3-12 (login limiter):**
  - The user login limiter counts **every** attempt, successful ones included: 5 per 5 minutes per email+IP.
  - A user who logs in and out a few times gets locked out for 15 minutes; my own test accounts hit this.
  - `verify-2fa` is limited per IP only, so users behind a shared NAT share 5 attempts.
  - `backend/crates/api/src/routes/identity/user.rs` (`login`, `verify_2fa`). Check whether this matches the original.
- **P3-13 (test-plan wording):** An expired order is shown as "已取消". TEST_FLOWS F-052 says "已过期", but backend-spec says a timeout cancels the order, so this is a test-plan wording issue.
- **P3-14 (speed):** Loading to network idle took 2–6 s. From here TTFB was ~1.2 s and first contentful paint ~2.1 s. There are about 10 separate font-subset requests, and full-size PNGs are used as card images. Consider preloading the main font subset and serving thumbnail-size images.
- **P3-15 (seed data, not code):**
  - zh-TW and en-US product titles, descriptions and tags, category names, and the home announcement hold Simplified Chinese.
  - The wallet ledger shows the admin's internal remark ("QA storefront test top-up") to the user; this probably matches the original.
  - Admin QA agents changing site settings during the run briefly re-branded the main store.
