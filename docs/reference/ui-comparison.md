# UI acceptance comparison

Acceptance pass comparing every baseline screenshot (`screenshots/`, compatibility service + seed data) with a new
screenshot of the same route (`screenshots-new/`, **Rust backend**, same file name). The UI keeps its
anime visual style on purpose (see `docs/DESIGN.md`); this table compares **information structure and
functionality** only.

How the new set was produced: fresh Rust backend (`e2e/scripts/start-backend.sh`, with
`ZS__RESELLER__ENABLED=true` to match the baseline config) → e2e specs 01–04 → the baseline seed data
(categories `game-cards`/`streaming`, products `steam-100`/`netflix-month`, card secrets, banner, posts,
member level, coupon, promotion, gift cards, user `u1@test.com`, a guest and two member orders) →
Playwright at 1440×900, `zh-CN`, full page, logged in via `user_token` / `admin_token` in localStorage.
Data therefore differs slightly (site name “Zebra E2E …”, extra e2e products/orders); that is not counted
as a difference.

Legend: ✅ same information/sections · 🔧 gap found and fixed.

**Result:** 88/88 pages compared. Gaps fixed on 12 pages (frontend only; backend unchanged). No missing pages,
no broken/empty states on the Rust backend.


## user-guest

| page | baseline | new | same info? | differences | action |
|---|---|---|---|---|---|
| Home (`root`) | [img](screenshots/user-guest/root.png) | [img](screenshots-new/user-guest/root.png) | ✅ | Same sections: banner hero, category chips, product grid, footer (quick links, legal). Footer “联系我们” absent in new run only because the e2e setup saved empty Telegram/WhatsApp links (backend defaults match original). | — |
| Product list (`products`) | [img](screenshots/user-guest/products.png) | [img](screenshots-new/user-guest/products.png) | ✅ | Same search, category filter, grid/list toggle, product cards (tags, price, stock badge). | — |
| Category list (`categories_game-cards`) | [img](screenshots/user-guest/categories_game-cards.png) | [img](screenshots-new/user-guest/categories_game-cards.png) | ✅ | Same breadcrumb/category filter and filtered grid. | — |
| Product detail (auto) (`products_steam-100`) | [img](screenshots/user-guest/products_steam-100.png) | [img](screenshots-new/user-guest/products_steam-100.png) | ✅ | Same: breadcrumb incl. category, tags, purchase/delivery/stock badges, promotion price + saved amount, promotion box, SKU card, description, quantity, add-to-cart / buy-now, detail section. New adds a delivery-safety hint. | — |
| Product detail (member, manual, SKUs) (`products_netflix-month`) | [img](screenshots/user-guest/products_netflix-month.png) | [img](screenshots-new/user-guest/products_netflix-month.png) | ✅ | Same SKU cards (基础版/高级版 with price + stock), member-only badge, manual delivery. | — |
| Cart (empty) (`cart`) | [img](screenshots/user-guest/cart.png) | [img](screenshots-new/user-guest/cart.png) | ✅ | Same empty state + go-shopping CTA. | — |
| Checkout (empty) (`checkout`) | [img](screenshots/user-guest/checkout.png) | [img](screenshots-new/user-guest/checkout.png) | ✅ | Same empty-cart state. | — |
| Guest order lookup (`guest_orders`) | [img](screenshots/user-guest/guest_orders.png) | [img](screenshots-new/user-guest/guest_orders.png) | ✅ | Same email + order-password + order-no form. | — |
| Blog list (`blog`) | [img](screenshots/user-guest/blog.png) | [img](screenshots-new/user-guest/blog.png) | ✅ | Same search box and post cards (badge, date, title, summary, read more). | — |
| Blog detail (`blog_how-to-buy`) | [img](screenshots/user-guest/blog_how-to-buy.png) | [img](screenshots-new/user-guest/blog_how-to-buy.png) | ✅ | Same title, date, content, back link, related products. | — |
| Notice list (`notice`) | [img](screenshots/user-guest/notice.png) | [img](screenshots-new/user-guest/notice.png) | 🔧 | Original: single-column list rows (bell icon, badge + date, title, summary, chevron). New used the blog 3-column card grid. | Fixed: new `NoticeRow` list layout for notices (`components/content/PostCard.tsx`, `PostListPage.tsx`). |
| About (`about`) | [img](screenshots/user-guest/about.png) | [img](screenshots-new/user-guest/about.png) | ✅ | Same hero, introduction, services, contact blocks. | — |
| Terms (`terms`) | [img](screenshots/user-guest/terms.png) | [img](screenshots-new/user-guest/terms.png) | ✅ | Same legal page. | — |
| Login (`auth_login`) | [img](screenshots/user-guest/auth_login.png) | [img](screenshots-new/user-guest/auth_login.png) | 🔧 | Same fields/remember-me/“personal center” badge. New always showed “忘记密码”; original hides it when email verification is off (reset needs a mailed code). | Fixed: link gated on `email_verification_enabled` (`useLogin.ts`, `Login.tsx`). |
| Register (`auth_register`) | [img](screenshots/user-guest/auth_register.png) | [img](screenshots-new/user-guest/auth_register.png) | 🔧 | Original: corner “用户注册” badge; submit disabled until the agreement is checked. New had no badge and an always-enabled submit. | Fixed: badge added, submit disabled until agreed (`Register.tsx`). |
| Forgot password (`auth_forgot`) | [img](screenshots/user-guest/auth_forgot.png) | [img](screenshots-new/user-guest/auth_forgot.png) | ✅ | Same email + code + new password form. | — |
| 404 (`nope-404`) | [img](screenshots/user-guest/nope-404.png) | [img](screenshots-new/user-guest/nope-404.png) | ✅ | Same 404 message + home / back actions. | — |
| Payment page (guest) (`pay_order_no_DJ20260924173502588608_guest_1`) | [img](screenshots/user-guest/pay_order_no_DJ20260924173502588608_guest_1.png) | [img](screenshots-new/user-guest/pay_order_no_DJ20260924173502588608_guest_1.png) | ✅ | Same order summary, amount, payment channel list, pay button, countdown. (New file taken with a freshly created guest order.) | — |

## user-member

| page | original | new | same info? | differences | action |
|---|---|---|---|---|---|
| Personal center overview (`me`) | [img](screenshots/user-member/me.png) | [img](screenshots-new/user-member/me.png) | ✅ | Same profile header, side nav (overview/orders/wallet/affiliate/reseller/gift card/security/API/profile), stats cards, recent orders. | — |
| Profile (`me_profile`) | [img](screenshots/user-member/me_profile.png) | [img](screenshots-new/user-member/me_profile.png) | ✅ | Same nickname/locale form, email change & bindings sections. | — |
| Security (`me_security`) | [img](screenshots/user-member/me_security.png) | [img](screenshots-new/user-member/me_security.png) | ✅ | Same change-password, 2FA, login history (time/status/IP/reason) sections. | — |
| My orders (`me_orders`) | [img](screenshots/user-member/me_orders.png) | [img](screenshots-new/user-member/me_orders.png) | ✅ | Same product/top-up tabs, stats, order-no + status filters, search/reset/refresh, order rows. | — |
| Wallet (`me_wallet`) | [img](screenshots/user-member/me_wallet.png) | [img](screenshots-new/user-member/me_wallet.png) | ✅ | Same balance card, recharge form with channels, transaction list. | — |
| Gift cards (`me_gift-cards`) | [img](screenshots/user-member/me_gift-cards.png) | [img](screenshots-new/user-member/me_gift-cards.png) | ✅ | Same redeem form + redemption history. | — |
| API integration (`me_api`) | [img](screenshots/user-member/me_api.png) | [img](screenshots-new/user-member/me_api.png) | ✅ | Same credential status / apply section and docs hints. | — |
| Affiliate (`me_affiliate`) | [img](screenshots/user-member/me_affiliate.png) | [img](screenshots-new/user-member/me_affiliate.png) | ✅ | Same disabled-state card (affiliate off in both seeds). | — |
| Reseller console (`reseller`) | [img](screenshots/user-member/reseller.png) | [img](screenshots-new/user-member/reseller.png) | 🔧 | Header lacked the original “personal center” icon and “logout” button; “返回店铺” pointed to /me instead of /. Body showed “not opened” in new run because `reseller.enabled` was false in the Rust config used for the first screenshots. | Fixed header (`views/reseller/ResellerLayout.tsx`); final screenshots use `ZS__RESELLER__ENABLED=true` like the original config. |
| Reseller apply (`reseller_apply`) | [img](screenshots/user-member/reseller_apply.png) | [img](screenshots-new/user-member/reseller_apply.png) | ✅ | With reseller enabled: same status stepper, benefits card and application form (reason + submit). Earlier “cannot apply” state was config, not a code gap (`can_apply = enabled && self_apply_enabled`, same as Go). | Config only. |

## user-dark

| page | original | new | same info? | differences | action |
|---|---|---|---|---|---|
| Home (dark) (`root`) | [img](screenshots/user-dark/root.png) | [img](screenshots-new/user-dark/root.png) | ✅ | Same structure as light home. | — |
| Product detail (dark) (`products_steam-100`) | [img](screenshots/user-dark/products_steam-100.png) | [img](screenshots-new/user-dark/products_steam-100.png) | ✅ | Same structure as light detail. | — |

## admin

| page | original | new | same info? | differences | action |
|---|---|---|---|---|---|
| Login (`login`) | [img](screenshots/admin/login.png) | [img](screenshots-new/admin/login.png) | ✅ | Same username/password form + brand panel. (A transient “network error” toast in the first run was a capture during dev-server warm-up; re-shot.) | — |
| Dashboard (`root`) | [img](screenshots/admin/root.png) | [img](screenshots-new/admin/root.png) | ✅ | Same time-range filter, KPI cards, trend charts, alerts, top products/channels. (First run captured the sidebar mid-collapse; re-shot with the sidebar pinned open.) | — |
| Categories (`categories`) | [img](screenshots/admin/categories.png) | [img](screenshots-new/admin/categories.png) | ✅ | Same columns (ID, icon, name, slug, parent, sort, created, actions) and create button. | — |
| Products (`products`) | [img](screenshots/admin/products.png) | [img](screenshots-new/admin/products.png) | ✅ | Same filters (keyword, category, status, fulfillment) and columns incl. stock and actions. | — |
| Card secrets (`card-secrets`) | [img](screenshots/admin/card-secrets.png) | [img](screenshots-new/admin/card-secrets.png) | ✅ | Same product/SKU/status/batch filters, stats, columns, batch actions. | — |
| Card secret imports (`card-secret-imports`) | [img](screenshots/admin/card-secret-imports.png) | [img](screenshots-new/admin/card-secret-imports.png) | ✅ | Same import history columns. | — |
| Card secret exports (`card-secret-exports`) | [img](screenshots/admin/card-secret-exports.png) | [img](screenshots-new/admin/card-secret-exports.png) | ✅ | Same export history columns. | — |
| Orders (`orders`) | [img](screenshots/admin/orders.png) | [img](screenshots-new/admin/orders.png) | 🔧 | Original: all columns visible incl. 创建时间/更新时间, actions stacked vertically, created-range filter captioned “创建时间”. New: 更新时间 hidden behind the sticky action column (wide horizontal action strip), range filter uncaptioned. | Fixed: vertical action stack, dense cell padding (new `DataTable dense`), two-line dates, `RangeFilter` with caption; `FilterBar` items bottom-aligned. |
| Order risk control (`order-risk-control`) | [img](screenshots/admin/order-risk-control.png) | [img](screenshots-new/admin/order-risk-control.png) | ✅ | Same rules / blocked lists sections. | — |
| Refund records (`order-refunds`) | [img](screenshots/admin/order-refunds.png) | [img](screenshots-new/admin/order-refunds.png) | 🔧 | Created-time range filter lacked the “创建时间” caption. | Fixed: `RangeFilter` with caption. |
| Payment channels (`payment-channels`) | [img](screenshots/admin/payment-channels.png) | [img](screenshots-new/admin/payment-channels.png) | ✅ | Same columns; new also shows 删除 in view (original has it too, clipped off-screen). | — |
| Payment records (`payments`) | [img](screenshots/admin/payments.png) | [img](screenshots-new/admin/payments.png) | 🔧 | Created-time range filter lacked caption. | Fixed: `RangeFilter` with caption. |
| Callback routes (`callback-routes`) | [img](screenshots/admin/callback-routes.png) | [img](screenshots-new/admin/callback-routes.png) | ✅ | Same route table. | — |
| Users (`users`) | [img](screenshots/admin/users.png) | [img](screenshots-new/admin/users.png) | 🔧 | 管理员备注 and 操作 columns pushed off-screen. | Fixed: dense table, wrapped date cells, narrower note cell — all columns + actions visible. |
| User detail (`users_1`) | [img](screenshots/admin/users_1.png) | [img](screenshots-new/admin/users_1.png) | ✅ | Same profile, wallet, orders, login logs blocks. | — |
| Wallet recharges (`wallet-recharges`) | [img](screenshots/admin/wallet-recharges.png) | [img](screenshots-new/admin/wallet-recharges.png) | 🔧 | 创建时间 column cut off at the right edge. | Fixed: dense table + tighter min widths. |
| Wallet config (`wallet-config`) | [img](screenshots/admin/wallet-config.png) | [img](screenshots-new/admin/wallet-config.png) | ✅ | Same recharge channel / amount settings. | — |
| User login logs (`user-login-logs`) | [img](screenshots/admin/user-login-logs.png) | [img](screenshots-new/admin/user-login-logs.png) | 🔧 | Created range uncaptioned (original: “开始时间” caption). | Fixed: `RangeFilter` with caption. |
| Member levels (`member-levels`) | [img](screenshots/admin/member-levels.png) | [img](screenshots-new/admin/member-levels.png) | ✅ | Same columns (name, slug, discount, thresholds, sort, status). | — |
| Posts – blog (`posts_blog`) | [img](screenshots/admin/posts_blog.png) | [img](screenshots-new/admin/posts_blog.png) | ✅ | Same tabs, columns, publish button. | — |
| Posts – notice (`posts_notice`) | [img](screenshots/admin/posts_notice.png) | [img](screenshots-new/admin/posts_notice.png) | ✅ | Same. | — |
| Post categories (`posts_categories`) | [img](screenshots/admin/posts_categories.png) | [img](screenshots-new/admin/posts_categories.png) | ✅ | Same. | — |
| Banners (`banners`) | [img](screenshots/admin/banners.png) | [img](screenshots-new/admin/banners.png) | ✅ | Same filters + columns (image, name, position, link type, sort, status). | — |
| Media library (`media`) | [img](screenshots/admin/media.png) | [img](screenshots-new/admin/media.png) | ✅ | Same search, scene filter, batch mode, upload; new run has uploaded logos (data). | — |
| Coupons (`coupons`) | [img](screenshots/admin/coupons.png) | [img](screenshots-new/admin/coupons.png) | ✅ | Same filters and columns incl. limits block and validity. | — |
| Promotions (`promotions`) | [img](screenshots/admin/promotions.png) | [img](screenshots-new/admin/promotions.png) | ✅ | Same. | — |
| Wholesale prices (`wholesale-prices`) | [img](screenshots/admin/wholesale-prices.png) | [img](screenshots-new/admin/wholesale-prices.png) | ✅ | Same. | — |
| Gift cards (`gift-cards`) | [img](screenshots/admin/gift-cards.png) | [img](screenshots-new/admin/gift-cards.png) | 🔧 | 操作 column (编辑/删除) off-screen; batch-no column wrapped into a tall 4-char strip. | Fixed: dense table, min width for batch no, stacked actions. |
| Affiliate settings (`affiliates_settings`) | [img](screenshots/admin/affiliates_settings.png) | [img](screenshots-new/admin/affiliates_settings.png) | ✅ | Same toggle, rate, confirm days, min withdraw, channels. | — |
| Affiliate users (`affiliates_users`) | [img](screenshots/admin/affiliates_users.png) | [img](screenshots-new/admin/affiliates_users.png) | ✅ | Same filters/columns. | — |
| Commissions (`affiliates_commissions`) | [img](screenshots/admin/affiliates_commissions.png) | [img](screenshots-new/admin/affiliates_commissions.png) | ✅ | Same. | — |
| Affiliate withdraws (`affiliates_withdraws`) | [img](screenshots/admin/affiliates_withdraws.png) | [img](screenshots-new/admin/affiliates_withdraws.png) | ✅ | Same. | — |
| Reseller overview (`resellers_operations`) | [img](screenshots/admin/resellers_operations.png) | [img](screenshots-new/admin/resellers_operations.png) | ✅ | Same range filter, KPI groups, alerts, active resellers, finance tables. | — |
| Reseller profiles (`resellers_profiles`) | [img](screenshots/admin/resellers_profiles.png) | [img](screenshots-new/admin/resellers_profiles.png) | ✅ | Same filters + columns (action column off-screen in both). | — |
| Reseller domains (`resellers_domains`) | [img](screenshots/admin/resellers_domains.png) | [img](screenshots-new/admin/resellers_domains.png) | ✅ | Same. | — |
| Reseller site configs (`resellers_site-configs`) | [img](screenshots/admin/resellers_site-configs.png) | [img](screenshots-new/admin/resellers_site-configs.png) | ✅ | Same. | — |
| Reseller product settings (`resellers_product-settings`) | [img](screenshots/admin/resellers_product-settings.png) | [img](screenshots-new/admin/resellers_product-settings.png) | ✅ | Same; new selects are labelled (定价模式 · 全部). | — |
| Reseller ledger (`resellers_ledger-entries`) | [img](screenshots/admin/resellers_ledger-entries.png) | [img](screenshots-new/admin/resellers_ledger-entries.png) | ✅ | Same. | — |
| Reseller balances (`resellers_balance-accounts`) | [img](screenshots/admin/resellers_balance-accounts.png) | [img](screenshots-new/admin/resellers_balance-accounts.png) | ✅ | Same. | — |
| Reseller withdraws (`resellers_withdraws`) | [img](screenshots/admin/resellers_withdraws.png) | [img](screenshots-new/admin/resellers_withdraws.png) | ✅ | Same. | — |
| Site connections (`site-connections`) | [img](screenshots/admin/site-connections.png) | [img](screenshots-new/admin/site-connections.png) | 🔧 | 操作 column header off-screen (table min-width 1180px). | Fixed: min-width 1100px. |
| Product mappings (`product-mappings`) | [img](screenshots/admin/product-mappings.png) | [img](screenshots-new/admin/product-mappings.png) | ✅ | Same. | — |
| Procurement orders (`procurement-orders`) | [img](screenshots/admin/procurement-orders.png) | [img](screenshots-new/admin/procurement-orders.png) | ✅ | Same status cards, filters, table. | — |
| Reconciliation (`reconciliation`) | [img](screenshots/admin/reconciliation.png) | [img](screenshots-new/admin/reconciliation.png) | ✅ | Same. | — |
| API credentials (`api-credentials`) | [img](screenshots/admin/api-credentials.png) | [img](screenshots-new/admin/api-credentials.png) | ✅ | Original screenshot is broken (“404 page not found” from the Go dev proxy). New page has status filter, search, full column set. | — |
| Telegram bot overview (`telegram-bot`) | [img](screenshots/admin/telegram-bot.png) | [img](screenshots-new/admin/telegram-bot.png) | ✅ | Same license notice, connection status, 4 entry cards, quick actions. | — |
| Telegram settings (`telegram-bot_settings`) | [img](screenshots/admin/telegram-bot_settings.png) | [img](screenshots-new/admin/telegram-bot_settings.png) | ✅ | Same global/basic/welcome sections. | — |
| Help center (`telegram-bot_help-center`) | [img](screenshots/admin/telegram-bot_help-center.png) | [img](screenshots-new/admin/telegram-bot_help-center.png) | ✅ | Same FAQ editor. | — |
| Menu config (`telegram-bot_menu`) | [img](screenshots/admin/telegram-bot_menu.png) | [img](screenshots-new/admin/telegram-bot_menu.png) | ✅ | Same menu item editor. | — |
| Bot status (`telegram-bot_status`) | [img](screenshots/admin/telegram-bot_status.png) | [img](screenshots-new/admin/telegram-bot_status.png) | ✅ | Same runtime status grid. | — |
| Bot clients (`telegram-bot_channel-clients`) | [img](screenshots/admin/telegram-bot_channel-clients.png) | [img](screenshots-new/admin/telegram-bot_channel-clients.png) | ✅ | Same columns. | — |
| Broadcasts (`telegram-bot_broadcasts`) | [img](screenshots/admin/telegram-bot_broadcasts.png) | [img](screenshots-new/admin/telegram-bot_broadcasts.png) | ✅ | Same filters/columns; new adds a refresh button. | — |
| New broadcast (`telegram-bot_broadcasts_create`) | [img](screenshots/admin/telegram-bot_broadcasts_create.png) | [img](screenshots-new/admin/telegram-bot_broadcasts_create.png) | ✅ | Same fields; new uses a rich-text editor for the HTML message. | — |
| System settings (`settings`) | [img](screenshots/admin/settings.png) | [img](screenshots-new/admin/settings.png) | ✅ | Same 13 tabs and basic-tab sections. | — |
| Notification center (`settings_notifications`) | [img](screenshots/admin/settings_notifications.png) | [img](screenshots-new/admin/settings_notifications.png) | ✅ | Same sections incl. test send and history. | — |
| Authorization (`authz`) | [img](screenshots/admin/authz.png) | [img](screenshots-new/admin/authz.png) | ✅ | Same admin list/create, role list, policy editor + catalog, role assignment; new admin table also shows 最近登录 + actions. | — |
| Authz audit logs (`authz-audit-logs`) | [img](screenshots/admin/authz-audit-logs.png) | [img](screenshots-new/admin/authz-audit-logs.png) | ✅ | Same filters/columns. | — |
| Admin security (`security`) | [img](screenshots/admin/security.png) | [img](screenshots-new/admin/security.png) | ✅ | Same change password + 2FA sections (new shows 2FA status badge). | — |

## Remaining intentional differences

- Visual language (colours, typography, glass cards, mascot/empty-state illustrations, sakura effects,
  gradient buttons) — per `docs/DESIGN.md`.
- New UI adds small extras that do not remove information: delivery-safety hint on product detail,
  numbered step indicators, status badges on security/2FA, refresh buttons, rich-text editor for broadcasts,
  pagination footer on every admin list (original shows it only on some).
- Admin list tables wider than the viewport still scroll horizontally (as in the original, e.g. reseller
  profiles/domains); only pages where the new layout hid more columns than the original were changed.
- Footer “联系我们” is data-driven: hidden when Telegram/WhatsApp links are empty (the e2e admin setup saves
  empty values). Defaults (`telegram.me/dujiaoka`, `wa.me/1234567890`) match the original backend.

## Fixes (files)

- storefront: `src/composables/useLogin.ts`, `src/views/auth/Login.tsx`, `src/views/auth/Register.tsx`,
  `src/views/reseller/ResellerLayout.tsx`, `src/components/content/PostCard.tsx` (`NoticeRow`),
  `src/components/content/PostListPage.tsx`.
- admin: `src/components/ui/DataTable.tsx` (`dense`), `src/components/ui/RangeFilter.tsx` (moved from
  `views/users/components`, now shared), `src/components/ui/FilterBar.tsx` (items bottom-aligned),
  `src/components/ui/DateTimeInput.tsx` (placeholder exposed as `title`/`aria-label`),
  `src/views/orders/Orders.tsx`, `src/views/orders/OrderRefunds.tsx`, `src/views/payments/Payments.tsx`,
  `src/views/users/{Users,WalletRecharges,UserLoginLogs}.tsx`, `src/views/marketing/GiftCards.tsx`,
  `src/views/integration/SiteConnections.tsx`.
- e2e: `tests/03-storefront-member.spec.ts` — reset `memberOrderNos` for the new member (state.json
  persisted across runs and made specs 03/04 fail on a re-run).
