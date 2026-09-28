Storefront behavior and interface reference for the TSX implementation. Paths are relative to the storefront source root.

Views stay thin while reusable behavior lives in `src/composables/*`; this keeps the TSX pages testable and consistent.

---

## 1. Build setup

**`package.json`** (pnpm 10):
- **Runtime dependencies:**
  - vue 3.5, vue-router 4.6, pinia 3, vue-i18n 9.14, @unhead/vue 2 (head/SEO), @vueuse/core 14
  - reka-ui 2.9 (shadcn-vue primitives), class-variance-authority, clsx, tailwind-merge, lucide-vue-next (icons)
  - qrcode, dompurify, tiptap 2 (used only by the reseller rich text editor)
  - @fontsource/rubik and @fontsource/nunito-sans (loaded only by the vault template)
- **Dev dependencies:** vite 7, @vitejs/plugin-vue 6, @intlify/unplugin-vue-i18n 11, tailwindcss 4.1 with @tailwindcss/postcss, @tailwindcss/typography, tw-animate-css, typescript 5.9, vue-tsc.
- **Scripts:** `dev` is vite. `build` is `vue-tsc -b && vite build`. `test` is `node --experimental-strip-types --test tests/*.test.ts` (pure util tests in ROOT/tests).

**`vite.config.ts`**:
- Alias `@` points to `src`.
- VueI18nPlugin precompiles `src/i18n/locales/**` with `dropMessageCompiler: true`, and `__INTLIFY_JIT_COMPILATION__: true` is defined.
- A custom plugin adds `data-cfasync="false"` to module scripts (for Cloudflare Rocket Loader).
- In production, esbuild drops `console` and `debugger`.
- Manual chunks: `vendor-qrcode`, `vendor-vue-i18n`.
- **Dev server:** host `0.0.0.0`, **port 5173** (`strictPort`), `allowedHosts: ['.dujiao.test']`.
- **Proxy** to `http://localhost:8080` with `changeOrigin: false` (the original Host header is kept so the backend can tell which reseller site is being served) for `/api`, `/uploads`, `/sitemap.xml`, `/robots.txt`.
- Env: `VITE_API_BASE_URL` (optional prefix). The API prefix is `/api/v1`.
- Other files: `ROOT/components.json` (shadcn-vue config), `ROOT/postcss.config.js`, `ROOT/tailwind.config.js` (small; the Tailwind v4 config lives in CSS).

## 2. Router (`src/router/index.ts`)

History mode is `createWebHistory`. Scroll behaviour: restore the saved position, else scroll to the hash with an 80px offset, else go to the top. Every page component is wrapped in `templateView(name, classicLoader)` (see §4).

| Path | Name | View (classic) | Meta / props |
|---|---|---|---|
| `/` | home | views/Home.vue | – |
| `/products` | products | Products.vue, **or Home.vue when `config.template_mode==='list'`** | – |
| `/categories/:slug` | category-products | same switch as `/products` | – |
| `/products/:slug` | product-detail | ProductDetail.vue | – |
| `/cart` | cart | Cart.vue | – |
| `/checkout` | checkout | Checkout.vue (`?mode=buynow` for Buy Now) | – |
| `/pay` | payment | Payment.vue (query: `order_no`, `guest=1`, `recharge_no`, `out_trade_no`, `*_return`) | – |
| `/me` | personal-center | PersonalCenter.vue | props `{section:'overview'}`, requiresUserAuth |
| `/me/profile` | personal-center-profile | PersonalCenter | section `profile`, auth |
| `/me/security` | personal-center-security | PersonalCenter | section `security`, auth |
| `/me/orders` | personal-center-orders | PersonalCenter | section `orders`, auth |
| `/me/wallet` | personal-center-wallet | PersonalCenter | section `wallet`, auth |
| `/me/gift-cards` | personal-center-gift-cards | PersonalCenter | section `giftCard`, auth |
| `/me/api` | personal-center-api | PersonalCenter | section `api`, auth |
| `/me/affiliate` | personal-center-affiliate | PersonalCenter | section `affiliate`, auth |
| `/me/reseller` | personal-center-reseller | redirects to `/reseller` | auth, resellerConsole |
| `/reseller` (layout ResellerConsoleLayout.vue) | children: `''` dashboard, `apply`, `domains`, `site`, `products`, `orders`, `orders/:order_no`, `finance`, `ledger`, `withdraws` | views/reseller/* | auth, resellerConsole (no store header/footer) |
| `/orders/:order_no` | order-detail | OrderDetail.vue | auth |
| `/recharge-orders/:recharge_no` | recharge-order-detail | RechargeOrderDetail.vue | auth |
| `/guest/orders` | guest-orders | GuestOrders.vue | – |
| `/guest/orders/:order_no` | guest-order-detail | GuestOrderDetail.vue | – |
| `/blog` | blog | Blog.vue | – |
| `/blog/:slug` | blog-detail | BlogDetail.vue (also used for notices) | – |
| `/notice` | notice | Notice.vue | – |
| `/about` | about | About.vue | – |
| `/terms`, `/privacy` | terms, privacy | Legal.vue | props `{type}` |
| `/auth/login` | user-login | auth/Login.vue | userGuest |
| `/auth/register` | user-register | auth/Register.vue | userGuest |
| `/auth/forgot` | user-forgot | auth/Forgot.vue | userGuest |
| `/auth/telegram/callback` | user-telegram-callback | auth/TelegramCallback.vue | – |
| `/auth/google/callback` | user-google-callback | auth/GoogleCallback.vue | – |
| `/:pathMatch(.*)*` | not-found | NotFound.vue | – |

**Before each navigation (`beforeEach`):**
1. `captureAffiliateFromRoute(to)`: reads `?aff=CODE`, stores it in localStorage for 30 days and posts a click.
2. Awaits `appStore.loadConfig()` if the config is not loaded yet.
3. `requiresUserAuth` without a token goes to `/auth/login?redirect=…`. If the route is also `resellerConsole` and `!canAccessResellerConsole`, it goes to `/me/orders`.
4. `userGuest` pages redirect a logged-in user to `/me/orders`.

**After each navigation:** `applySEO()` (a no-op) and the Telegram Mini App back-button sync.

**Route warmup:** once the page is idle, it preloads Products, ProductDetail, Cart, Checkout, Payment, Blog, Notice and Login. This is skipped on saveData or 2g connections.

## 3. Views

The shared layout is in `src/App.vue`:
- **Classic:** `Navbar` + `<main class="pb-14 lg:pb-0">` + `Footer` + `BackToTop` + `MobileBottomNav`.
- **Vault:** the page is wrapped in `VaultLayout`.
- **Reseller console:** a bare `RouterView`.
- Always present: `Loading` overlay (`appStore.loading`), `Toast`, `ConfirmDialog`, `ErrorBoundary`, and a `page-fade` transition.

### Home (`src/views/Home.vue`, composables `useBannerCarousel`, `useProductList`, `useProductListGroups`, `useAnnouncement`)
- **Hero banner carousel:** `bannerAPI.list({position:'home_hero', limit:5})`.
  - Uses `mobile_image` when the viewport is under 768px. Title and subtitle are localized, with i18n `home.hero.*` fallbacks.
  - Autoplay every 5s, prev/next arrows, dots, touch swipe over 50px.
  - CTA uses `link_type` (`none`/`internal`/`external`), `link_value`, `open_in_new_tab`. Skeleton while loading.
- **Card mode** (default, `template_mode !== 'list'`):
  - "Featured": `productAPI.list({page:1,page_size:15})` in a ProductCard grid (2/3/4/5 columns).
  - "Latest": `postAPI.list({page:1,page_size:3,type?})`, shown only if blog or notice nav is enabled.
- **List mode:** CategorySidebar + search input + products grouped by category (ProductListItem rows), page size 20, PaginationNav.
- **Quick buy:** `ProductQuickBuy` modal.
- **Announcement:** `AnnouncementModal` for `config.announcement` (`{type,title,content,version}`).
  - Dismiss options: session only, today only, or forever.
  - Storage: localStorage `announcement_dismiss`, sessionStorage `announcement_closed`.
- SEO via `usePageSeo`.

### Products (`views/Products.vue`, `useProductList`)
- Header, then CategorySidebar (tree with collapsible parents, mobile drawer, search box), then a ProductCard grid with page size 12.
- **Data:** `categoryAPI.list()` and `productAPI.list({page,page_size,category_id?,search?})` (debounced 300ms).
- Selecting a category calls `router.replace` to `/categories/:slug`.
- **States:** skeleton, empty, and "filtered empty" with a clear-filters button.

### ProductDetail (`views/ProductDetail.vue`, `useProductDetail` ~700 lines)
- **Data:** `productAPI.detail(slug)`.
- **Layout:** breadcrumb; ProductImageGallery on the left; on the right: category, tags, title, then badges (purchase_type guest/member, fulfillment auto/manual, stock status).
- **Price block**, with variants:
  - Promotion price vs. original, with "save X".
  - Member price, from `member_prices` or the level `discount_rate`.
  - Wholesale tier price (`wholesale_prices[{sku_id,sku_code,min_quantity,unit_price}]`).
- **Rule panels:** wholesale rules and promotion rules (`promotion_rules[{type,min_amount,value}]`).
- **SKU selector:** active SKUs, stock badge per SKU, disabled when sold out.
- **Other content:** description; quantity stepper (min/max purchase and stock limits); alerts for cannot-purchase reasons and warnings.
- **Actions:** "Login to buy" when the product is member-only and the user is not logged in; otherwise "Add to cart" / "Buy now". Buy now uses `buyNowStore.setItem` and goes to `/checkout?mode=buynow`.
- **Below:** rich HTML `content` (prose), `related_posts`, back link.
- **Mobile:** fixed ProductMobileBar, shown once the purchase section scrolls out of view.
- **Error state:** retry button.

### Cart (`views/Cart.vue`, `useCart`, `utils/cartStock`)
- CheckoutSteps (cart → checkout → payment).
- **Line items:** image, title link, price, SKU text, stock hint, guest/member and auto/manual badges, quantity stepper (min/max/stock), subtotal (wholesale-aware), remove with undo toast.
- **Summary:** item count, total, disclaimer, checkout button, continue shopping.
- **Stock refresh:** re-fetches `productAPI.detail` for each item to refresh stock snapshots.

### Checkout (`views/Checkout.vue`, `useCheckout` ~1186 lines)
- **Item source:** the cart, or the buy-now store when `?mode=buynow`.
- **Items list:** discounted per-item price taken from `preview.items`.
- **CheckoutManualForm:** per-product `manual_form_schema.fields[{key,type,required,label{},placeholder{},regex,min,max,max_len,options[]}]`.
  - Field types: text, textarea, select, radio, checkbox, number, email, phone.
- **Coupon input:** hidden on reseller sites.
- **Guest mode** (not logged in): buttons "Guest purchase" and "Member purchase" (the latter links to login).
  - Fields: guest email and order password.
  - Captcha (image or Turnstile) when `config.captcha.scenes.guest_create_order` is set.
- **Summary:** original, coupon, promotion, wholesale, member discount, total (from preview).
- **Wallet balance:** logged-in users get a "use balance" checkbox, which is forced on when `wallet_only_payment`.
- **Payment channel grid:** icon, name, fee rate and fixed fee (shown when `fee_policy==='customer_surcharge'`), `min_amount`/`max_amount` limits.
- **APIs:**
  - Preview: `userOrderAPI.preview` or `guestOrderAPI.preview` (debounced).
  - Channels: `userOrderAPI.getPaymentChannels({amount, items})`.
  - Balance: `walletAPI.account`.
  - Submit: `userOrderAPI.createAndPay` or `guestOrderAPI.createAndPay`.
- **Submit behaviour:** clears the cart or buy-now store, saves the guest credentials, then routes to `/pay?order_no=…` (`&guest=1` for guests).

### Payment (`views/Payment.vue`, `usePayment` ~1429 lines)
Page states:
- skeleton
- guest auth form (email + order password)
- order not found
- expired or canceled
- **pre-payment view:**
  - Order info: payable amount, amount breakdown, `expires_at` countdown.
  - Items list.
  - Channel card: wallet balance toggle, "cached payment" restore banner, PaymentChannelSelector.
  - Sidebar with a Pay button.
- **result view:**
  - QR code, drawn locally with the `qrcode` library; falls back to encoding the pay link.
  - Crypto details: wallet_address, chain, chain_amount, token_id, with a copy button.
  - Or an "open pay link" button plus copy link.
  - Telegram in-app browser hint.
  - PaymentAmountBreakdown panel, "refresh status" and "change method" buttons.

**APIs:**
- Order: `userOrderAPI.detail` or `guestOrderAPI.detail`.
- Channels and balance: `userOrderAPI.getPaymentChannels`, `walletAPI.account`.
- Create payment: `paymentAPI.create({order_no, channel_id?, use_balance})` or `guestOrderAPI.createPayment`.
- Latest payment: `paymentAPI.latest({order_no})` or `guestOrderAPI.latestPayment`.
- Capture: `paymentAPI.capture(id)` or `guestOrderAPI.capturePayment`. Used for PayPal and Stripe returns (`stripe_return`, `pp_return`, and other `*_return` markers).

**Behaviour:**
- Polls every 5s (capture + reload).
- When the status becomes paid, fulfilling, partially_delivered, delivered or completed, it redirects after 600ms to `/orders/:no` or `/guest/orders/:no`.
- A recharge return (`recharge_no` or an `order_no` starting with `WR`) redirects to `/recharge-orders/:no`.
- `interaction_mode` is qr, redirect, wap or page.
- Channel type labels: wechat, wxpay, alipay, qqpay, paypal, stripe, usdt, usdt-trc20, usdc-trc20, trx.

### OrderDetail (`views/OrderDetail.vue`, `useOrderDetail`, `useOrderDisplayHelpers`)
- **APIs:** `userOrderAPI.detail`, `cancel` (with confirm dialog), `downloadFulfillment` (blob).
- **Header:** order number, status badge, "Pay now" (when pending), "Cancel".
- **Amount card:** original, discount, promotion, wholesale, member, total, wallet paid, online paid, refunded.
- **Time card:** created, paid, expires, canceled.
- **Items:** unit and total price, per-item discounts, SKU text, fulfillment type, tags, manual form submission rows (`manual_form_submission` + `manual_form_schema_snapshot`).
- **Fulfillment:** `order.fulfillment{type,status,payload,payload_line_count}` with copy and download buttons and a "truncated" hint; per-item `instructions` HTML blocks.
- **Child orders:** `order.children[]`, each with its own items and fulfillment.
- **Refunds:** `refund_records[{amount,currency,remark,created_at}]`.

### GuestOrders (`views/GuestOrders.vue`, `useGuestOrders`)
- **Form:** email, order password, optional order number. The last credentials are saved, with a "clear saved" action.
- **API:** `guestOrderAPI.list` (Guest Authorization header).
- **List:** order rows with view and pay actions, pagination.

### GuestOrderDetail (`views/GuestOrderDetail.vue`, `useGuestOrderDetail`)
Asks for guest auth, then shows the same body as OrderDetail using `guestOrderAPI.detail` and `downloadFulfillment`.

### RechargeOrderDetail (`views/RechargeOrderDetail.vue`, `useRechargeOrderDetail`)
- **APIs:** `walletAPI.rechargeDetail`, `walletAPI.captureRechargePayment`.
- **Shows:** amount, fee, payable, created and paid times, remark; payment section with QR or pay link and crypto details; "check payment status" button; success state.

### Blog, Notice, BlogDetail
- **Blog / Notice** (`usePostList(type)`): `postAPI.list({type, page, page_size:12, search?})`.
  - Blog has a search box. Notice is a plain list.
  - Cards show date, title, summary, thumbnail. Pagination.
- **BlogDetail** (`useBlogDetail`): `postAPI.detail(slug)`.
  - Shows title, date, HTML content, `related_products`.
  - Back link goes to blog or notice depending on the post type.

### About, Legal, NotFound
- **About** (`useAbout`): no API call; everything comes from `config.about`:
  - `hero{title,subtitle}`, `introduction`, `services{title,items[]}`, `contact{title,text}`
  - plus `config.contact{telegram,whatsapp}`
- **Legal:** `config.legal.terms[locale]` or `.privacy[locale]` rendered as HTML.
- **NotFound:** icon, "back home" and "back" buttons, quick links to products, blog, notice, about.

### Auth pages
- **Login** (`useLogin`):
  - Fields: email, password (show/hide), remember me, captcha (`scenes.login`).
  - Calls `userAuthStore.login({email,password,remember_me,captcha_payload})`.
  - If the response has `requires_totp`, switches to a **2FA step**: 6-digit code or recovery code, with a countdown to `challenge_expires_at`, then `verify2FA`.
  - Third-party login options:
    - Telegram widget (`telegram_auth.mode==='widget'`, `bot_username`)
    - Telegram OIDC (`telegramOidcStart` returns a URL)
    - Telegram Mini App auto-login (`init_data`)
    - Google (GoogleIdentityButton, popup or redirect flow via intent/exchange)
  - Links: forgot password, register (if `registration_enabled`).
  - After login, redirects to `?redirect` or `/me`.
- **Register** (`useRegister`):
  - Fields: email (or local part + a domain select when `email_domain_allowlist_enabled` / `allowed_email_domains`), password (with strength validation), email code with a send button (captcha scene `register_send_code`), agreement checkbox linking to terms and privacy.
  - Calls `sendVerifyCode({email,purpose:'register',captcha_payload})`, then `register({email,password,code,agreement_accepted})`, then goes to `/me/orders`.
  - Shows a "disabled" state when `registration_enabled===false`.
- **Forgot** (`useForgot`):
  - Fields: email, code (captcha scene `reset_send_code`), new password.
  - Calls `sendVerifyCode({purpose:'reset'})`, then `forgotPassword({email,code,new_password})`, then goes to `/auth/login`.
  - Disabled if `email_verification_enabled===false`.
- **TelegramCallback:** reads `code` and `state` from the query and calls `telegramOidcLogin`, or `telegramOidcBindCallback` for the bind flow.
- **GoogleCallback:** redirect-flow exchange (login or bind).

### PersonalCenter (`views/PersonalCenter.vue`, `usePersonalCenter`)
**Shell:** header card with an avatar initial, display name, email, "email verified" badge and member-level pill. Left sidebar on desktop, horizontal scrolling tabs on mobile.

Sections, in sidebar order:
- **Overview:** 4 StatCards (order count, level, discount, account status); member level card with upgrade progress bars (recharge/spend thresholds); recent 5 orders.
  - Uses `userProfileAPI.current`, `userOrderAPI.list`, `memberLevelAPI.list`.
- **Orders** (`views/personal/OrdersPanel.vue`): tabs for product orders and recharge orders.
  - Filters: status select + order number / recharge number. Stats cards. Paginated list with view and pay actions.
  - APIs: `userOrderAPI.list({page,page_size,status,order_no})`, `userOrderAPI.stats`, `walletAPI.rechargeOrders({…,recharge_no})`, `walletAPI.rechargeStats`.
  - Order statuses: pending_payment, paid, fulfilling, partially_delivered, partially_refunded, delivered, completed, expired, canceled, refunded.
  - Recharge statuses: pending, success, failed, expired.
- **Wallet** (`WalletPanel.vue`, with WalletBalanceCard, WalletRechargeForm, WalletTransactionList):
  - Recharge form: amount, channel select (filtered by `config.wallet_recharge_channel_ids`, with fee info), remark.
  - Calls `walletAPI.recharge`, then goes to `/recharge-orders/:no`.
  - Transactions table: type, direction, amount, balance_after, remark, time.
  - APIs: `walletAPI.account`, `getPaymentChannels(amount)`, `transactions`.
- **Affiliate** (`AffiliatePanel.vue`):
  - Not opened yet: "Open" button (`affiliateAPI.open`).
  - Dashboard stats: clicks, valid orders, conversion rate, pending/available/withdrawn commission; affiliate code; promotion URL with copy (`origin + promotion_path` or `/?aff=CODE`).
  - Commissions table and withdraws table, both paginated.
  - Withdraw form: amount, channel (from `config.affiliate.withdraw_channels`), account.
- **Reseller:** redirects to `/reseller`. Hidden on reseller sites.
- **Gift card** (`GiftCardPanel.vue`): code input + captcha (`scenes.gift_card_redeem`), calls `giftCardAPI.redeem`. The success card shows code, amount and new balance.
- **Security** (`SecurityPanel.vue`, components in `src/components/security/*`):
  - Email change: `email_change_mode` `bind_only` or `change_with_old_and_new`, with old and new codes.
  - Password change: `set_without_old` or `change_with_old`.
  - Telegram binding (widget, OIDC or Mini App; unbind).
  - Google binding.
  - 2FA TOTP: status, setup (secret + QR), enable, disable (code or recovery), regenerate recovery codes (copy/acknowledge).
  - Login history table (`/me/login-logs`).
- **API** (`ApiPanel.vue`):
  - No credential yet: "Apply" button (`apiCredentialAPI.apply`).
  - Status pending, rejected (shows reason, "reapply") or approved.
  - Shows `api_key`, the masked secret (`api_secret_tail`), active switch (`updateStatus`), regenerate with a confirm modal. The new secret is shown once.
- **Profile** (`ProfilePanel.vue`): nickname and locale select, `userProfileAPI.updateProfile`.

### Reseller console (`src/views/reseller/*`, composables `src/composables/reseller/*`, components `src/components/reseller-console/*`, `src/components/reseller/*`)
A separate layout with ResellerConsoleTopbar. Pages:
- **Dashboard:** setup checklist, balances, order distribution donut, recent orders, quick actions.
- **Apply:** reason textarea, `resellerAPI.apply`.
- **Domains:** system subdomain and custom domains with verification token, status and copy button; `submitDomain`.
- **Site:** ResellerSiteConfigPanel. Fields: site_name, logo, favicon (upload), announcement (localized, rich text via tiptap), support (telegram/whatsapp/email/url), SEO, footer_links, nav_config.
- **Products:** ResellerProductSettingsPanel and ResellerProductRuleEditor.
  - Per-SKU `pricing_mode`: inherit, markup_percent, fixed_markup, fixed_price; plus is_listed and sort_order; preview, save, reset.
- **Orders:** filters (status, order_no, date range); orders list, stats, detail with profit snapshot.
- **Finance:** balances, settlement status.
- **Ledger:** filters by type, status, order_id.
- **Withdraws:** form (amount, currency, channel, account) and a table.

Status constants are in `src/constants/reseller.ts`. The reseller console is admin-like; decide whether it is in scope for the anime restyle.

## 4. `src/templates`: storefront template system

`src/templates/registry.ts` switches the whole storefront between two skins, **`classic` | `vault`**.

**Which template is active**, in priority order:
1. localStorage `dj-storefront-template`, set with `?template=vault|classic`. `?template=reset` clears it. `initTemplateOverride()` handles this in `main.ts`.
2. `config.storefront_template`.
3. Default `classic`.

**How pages are resolved:** `templateView(name, classicLoader)` uses `import.meta.glob('./vault/**/*.vue')`. If vault is active and `./vault/${name}.vue` exists, that file is loaded; otherwise the classic view is used. This lets a template be built one page at a time.

**Vault files:**
- `src/templates/vault/*` has Home, Products, ProductDetail, Cart, Checkout, Payment, PersonalCenter, OrderDetail, RechargeOrderDetail, GuestOrders, GuestOrderDetail, Blog, BlogDetail, Notice, About, Legal, NotFound, `auth/*`.
- Sub-components: VaultBannerHero, VaultCategorySidebar, VaultProductCard, VaultProductListItem, VaultOrderBody, VaultOrderFulfillment, VaultOrderItem, VaultCheckoutSteps, VaultProductMobileBar.

**Vault shell and styling:**
- `src/App.vue` wraps vault pages in `layout/VaultLayout.vue`, which has its own header/footer/search/menus, loads Rubik and Nunito Sans, and adds the `body.vault-tokens` class.
- `src/templates/vault/styles/vault.css` holds only design tokens under `.vault-scope` / `body.vault-tokens`. It remaps the shadcn `--ui-*` variables to an indigo/gold palette. The tokens are also placed on `body` because overlays such as Toast and Select render outside the page wrapper.

**Shared logic:** both templates use the same composables (`usePostList`, `useAbout`, `useLegal`, `usePersonalCenter`, `useCheckout`, …), and vault PersonalCenter reuses the classic `views/personal/*Panel.vue` components.

**For the rewrite:** a new "anime" template can be added the same way (e.g. `templates/anime/*.tsx`, extending the glob to include `.tsx`), or it can replace the views outright.

## 5. Stores (Pinia, `src/stores`)

- **`app.ts`** (`useAppStore`):
  - State: `locale` (from `detectLocale`), `config` (any), `loading`, `serverTimeOffset`.
  - Computed: `siteIconHref`, `isResellerTenant` (`config.tenant.mode==='reseller'`), `canAccessResellerConsole`.
  - Actions: `setLocale` (writes localStorage `locale` and calls `setI18nLocale`), `loadConfig(force)` (GET `/public/config`; computes the server time offset from `server_time`; injects `config.scripts`), `getServerTime`.
  - A global `useHead` sets `html lang`, the title (`seo.title[locale]`, falling back to `brand.site_name`), favicon (`brand.site_icon` or `/dj.svg`), and meta keywords/description.
- **`userAuth.ts`:**
  - State: `token` (localStorage `user_token`), `user` (localStorage `user_profile`), `loading`, `challengeToken`, `challengeExpiresAt`.
  - Actions: login, verify2FA, register, sendVerifyCode, telegramLogin, telegramOidcLogin, telegramMiniAppLogin, googleLogin, googleRedirectLogin, forgotPassword, syncUserProfile, logout.
- **`userProfile.ts`:**
  - State: profile, recentOrders, ordersTotal, login logs, telegram/google bindings, memberLevels, many loading flags, errors.
  - Computed: `displayName`, `currentLevel`, `nextLevel`, `upgradeProgress`.
- **`cart.ts`:**
  - `items: CartItem[]` persisted in localStorage `cart_items`, keyed by `productId:skuId`.
  - Each item carries a stock snapshot, min/max purchase quantity, wholesalePrices, manualFormSchema and paymentChannelIds.
  - Actions: addItem, updateQuantity, patchItem, removeItem, clear. Getter: `totalItems`.
- **`buyNow.ts`:** a single in-memory `item` for `/checkout?mode=buynow`.
- **`telegramMiniApp.ts`:** Telegram WebApp detection, initData, theme params mapped to CSS variables, viewport, BackButton handling.

**How site settings are applied** (the config comes from `GET /api/v1/public/config`):

| Setting | Config field | Where used |
|---|---|---|
| Site name | `brand.site_name` | Navbar wordmark (fallback "Dujiao-Next"), title |
| Logo | `brand.site_logo` | Via `getImageUrl`, which prefixes relative `/uploads/...` with `VITE_API_BASE_URL` |
| Favicon | `brand.site_icon` | Global head |
| Other brand fields | `brand.site_description{locale}`, `brand.site_url` | Footer; canonical URLs |
| **Currency** | `config.currency` (3-letter code, default CNY) | `useLocalized().formatPrice` prints `"12.34 CNY"`; amounts are handled as decimal strings via `src/utils/money.ts` (integer cents) |
| **Theme** | `src/utils/theme.ts` | Light/dark via the `.dark` class on `<html>`; localStorage `dujiao_theme`, else the OS preference. Only the storefront template comes from config. Colors are CSS variables in `src/style.css` (`--ui-*` mapped to Tailwind `@theme inline`; Apple-like, accent `#0071e3`). |
| **Language** | localStorage `locale` > browser language > zh-CN | Localized content fields are JSON objects `{ "zh-CN": "", "zh-TW": "", "en-US": "" }`, resolved by `getLocalizedText` |

Other config keys in use:
- `template_mode` (`card` | `list`), `storefront_template`
- `nav_config{builtin{blog,notice,about}, custom_items[{title,url,link_type,target,icon,sort_order,enabled}]}`
- `footer_links`, `contact{telegram,whatsapp}`, `support{…}`
- `announcement`, `about`, `legal{terms,privacy}`
- `captcha{provider:'none'|'image'|'turnstile', scenes{login,register_send_code,reset_send_code,guest_create_order,gift_card_redeem}, turnstile{site_key}}`
- `telegram_auth{enabled,bot_username,mode,mini_app_url}`, `google_auth{enabled,client_id}`
- `registration_enabled`, `email_verification_enabled`, `email_domain_allowlist_enabled`, `allowed_email_domains`
- `wallet_only_payment`, `wallet_recharge_channel_ids`, `affiliate.withdraw_channels`
- `seo{title,keywords,description,default_og_image}`, `scripts[{name,enabled,position:'head'|'body_end',code}]`
- `tenant.mode`, `server_time`, `app_version`

## 6. i18n (`src/i18n/index.ts`, `src/i18n/locales/*.json`)

- **Languages:** `zh-CN` (default and fallback, bundled statically), `zh-TW` and `en-US` (lazy-loaded chunks, prefetched when idle). Each file has about 1728 lines.
- **Setup:** Composition API (`legacy:false`). `setI18nLocale` loads the language pack before switching. Every request sends an `X-Lang` header with the current locale.
- **Top-level keys:** common, errorBoundary, formValidation, error, nav, vault, breadcrumb, emptyState, pagination, checkoutSteps, footer, notFoundPage, home, products, blog, blogDetail, notice, announcement, about, navbar, toast, bottomNav, personalCenter (tabs, overview, memberLevel, profile, security, wallet, affiliate, reseller, giftCard, apiPanel), auth (common, login, register, forgot, telegramCallback, googleCallback), order.status, cart, checkout, payment, resellerConsole, orders, rechargeOrder, orderDetail, guestOrders, guestOrderDetail, productDetail, quickBuy, productPurchase.
- **Language switcher labels:** 简体中文, 繁體中文, English.

## 7. Shared components (`src/components`)

- **Navbar.vue:** fixed, blurred header that shrinks on scroll.
  - Logo + name; primary nav items from `useNavConfig` (Home, Products unless list mode, Blog/Notice/About toggles, custom items with preset icons).
  - Cart with a count badge that bounces on change; guest orders link; login or "me" + logout; theme toggle; language popover.
  - Mobile "more" drawer (Teleport) with secondary items.
- **Footer.vue:** brand, description, quick links, `footer_links`, Telegram/WhatsApp contacts, terms and privacy links, copyright.
- **MobileBottomNav.vue:** Home, Products (hidden in list mode), Cart (with badge), Me (goes to `/auth/login` when logged out).
- **ProductCard.vue:**
  - Image (4:3), sold-out overlay, tags, category, title.
  - Badges: purchase type, fulfillment type, stock.
  - Promotion / original price; wholesale or promotion badge; quick-buy button.
  - Emits `click(slug)` and `quickBuy(product)`.
- **ProductListItem.vue:** compact row version of ProductCard, used in list mode.
- **ProductQuickBuy.vue** (895 lines): modal (Teleport) with SKU select, quantity, price rules, add to cart (toast) or buy now, login, "view details".
- **CategorySidebar.vue:** category tree with expand/collapse, mobile drawer, optional search.
- **Payment:**
  - `payment/PaymentChannelSelector.vue`: channel grid with icon, name, fee and limit hints.
  - `payment/PaymentAmountBreakdown.vue`.
- **Checkout:** `checkout/CheckoutSteps.vue`, `checkout/CheckoutManualForm.vue`.
- **Product:** `product/ProductImageGallery.vue`, `product/ProductMobileBar.vue`.
- **Captcha:**
  - `captcha/ImageCaptcha.vue`: GET `/public/captcha/image` returns `{captcha_id, image_base64}`; v-model `{captcha_id, captcha_code}`; exposes `refresh()`.
  - `captcha/TurnstileCaptcha.vue`: v-model token, prop `siteKey`, exposes `reset()`.
- **Auth:** `auth/GoogleIdentityButton.vue` (GIS popup or redirect).
- **Feedback and dialogs:** AnnouncementModal, EmptyState (icon/title/action/variant/size), Loading overlay, Toast (with the `useToast` composable), ConfirmDialog (`useConfirmDialog`), ErrorBoundary.
- **Navigation:** BackToTop, BreadcrumbNav, PaginationNav.
- **Media:** SmartImage (lazy loading with a fallback).
- **Personal center parts:** `shared/StatCard.vue`, `shared/PanelHeading.vue`, `security/*`, `wallet/*`.
- **UI primitives:** `ui/*` is the shadcn-vue set on reka-ui: alert, badge (variants success/warning/info/danger/accent/neutral/destructive, size xs/sm), button (default/secondary/ghost/outline/destructive, `as-child`), card, checkbox, input, label, popover, select, switch, table, textarea, tooltip.

## 8. API endpoints (base `/api/v1`)

**Client** (`src/api/client.ts`): fetch-based, 10s timeout.
- **Response envelope:** `{status_code:0, msg, data, pagination?{page,page_size,total,total_page}}`. Any non-zero `status_code` is treated as an error.
- `api` is the public client. `userApi` adds `Authorization: Bearer <user_token>`.
- On a 401 (except public auth endpoints) the client clears the token and does a hard redirect to `/auth/login`.
- **Guest endpoints** send `Authorization: Guest base64url(lower(email) + "\n" + order_password)` (`src/api/order.ts`).
- Types are in `src/api/types.ts`; the list below repeats the main shapes.

**Public**
- `GET /public/config`: site config (§5).
- `GET /public/products` `{page,page_size,category_id?,search?}` returns a product list.
  - Product fields: `id, slug, title{}, description{}, content{}, images[], tags[], category{name{}}, price_amount, promotion_price_amount, promotion_rules[], wholesale_prices[], member_prices[{member_level_id,sku_id,price_amount}], purchase_type(guest|member), fulfillment_type(auto|manual), stock_status(unlimited|in_stock|low_stock|out_of_stock), stock_display_mode, stock_quantity_hidden, is_sold_out, manual/auto_stock_available, min/max_purchase_quantity, manual_form_schema, payment_channel_ids, skus[], related_posts[]`.
  - SKU fields: `id, sku_code, spec_values{}, price_amount, promotion_price_amount, is_active, stock_status, stock_display, stock_display_mode, stock_range_min/max, stock_quantity_hidden, manual_stock_total/locked/sold, auto_stock_available, upstream_stock`.
- `GET /public/products/:slug`
- `GET /public/posts` `{type:'blog'|'notice', page, page_size, search?}`. Post fields: `id, slug, type, title{}, summary{}, content{}, thumbnail, published_at`.
- `GET /public/posts/:slug` (includes `related_products`).
- `GET /public/banners` `{position:'home_hero', limit}`. Banner fields: `image, mobile_image, title{}, subtitle{}, link_type, link_value, open_in_new_tab`.
- `GET /public/categories`. Category fields: `id, parent_id, slug, name{}, icon`.
- `GET /public/member-levels` returns `PublicMemberLevel[]`: `id, name{}, slug, icon, discount_rate, recharge_threshold, spend_threshold, is_default, sort_order`.
- `GET /public/captcha/image` returns `{captcha_id, image_base64}`.
- `POST /public/affiliate/click` `{affiliate_code, visitor_key, landing_path, referrer}`.

**Auth**
- `POST /auth/send-verify-code` `{email, purpose:'register'|'reset', captcha_payload}`
- `POST /auth/register` `{email,password,code,agreement_accepted}` returns `{token,user}`.
- `POST /auth/login` `{email,password,remember_me,captcha_payload}` returns `{token,user}` or `{requires_totp,challenge_token,challenge_expires_at}`.
- `POST /auth/login/verify-2fa` `{challenge_token, code? | recovery_code?}`
- `POST /auth/telegram/login` (widget payload `{id,first_name,last_name,username,photo_url,auth_date,hash}`)
- `POST /auth/telegram/miniapp/login` `{init_data}`
- `GET /auth/telegram/oidc/start`; `POST /auth/telegram/oidc/callback` `{code,state}`
- `POST /auth/google/login` `{credential}`
- `POST /auth/google/redirect/intent` and `POST /auth/google/redirect/exchange` (sent with credentials: include); backend callback `/auth/google/redirect/callback`.
- `POST /auth/forgot-password` `{email,code,new_password}`

`CaptchaPayload` is `{captcha_id?, captcha_code?, turnstile_token?}`.

**Me** (Bearer)
- `GET /me` returns `UserProfileData`: `id, email, nickname, email_verified_at, locale, member_level_id, total_recharged, total_spent, email_change_mode, password_change_mode`.
- `GET /me/login-logs`
- `PUT /me/profile` `{nickname?, locale?}`
- `POST /me/email/send-verify-code` `{kind:'old'|'new', new_email?}`
- `POST /me/email/change` `{new_email, old_code?, new_code}`
- `PUT /me/password` `{old_password?, new_password}`
- Telegram:
  - `GET /me/telegram` returns `{bound, provider_user_id, username, avatar_url, auth_at, can_unbind}`.
  - `POST /me/telegram/bind`, `POST /me/telegram/miniapp/bind`
  - `GET /me/telegram/oidc/start`, `POST /me/telegram/oidc/callback`
  - `DELETE /me/telegram/unbind`
- Google: `GET /me/google`, `POST /me/google/bind`, `POST /me/google/redirect/intent`, `POST /me/google/redirect/exchange`, `DELETE /me/google/unbind`.
- 2FA:
  - `GET /me/2fa/status`, `POST /me/2fa/setup`
  - `POST /me/2fa/enable {code}`, `POST /me/2fa/disable {code?|recovery_code?}`
  - `POST /me/2fa/recovery-codes/regenerate {code}`

**Orders and payments**
- `POST /orders/preview` and `POST /guest/orders/preview`.
  - Request: `{coupon_code?, affiliate_code?, affiliate_visitor_key?, items:[{product_id,sku_id?,quantity,fulfillment_type?}], manual_form_data:{[itemKey]:{field:value}}}`. Guests add `email, order_password`.
  - Response: `{currency, original_amount, discount_amount, promotion_discount_amount, wholesale_discount_amount, member_discount_amount, total_amount, items[…original_total_price…], payment_channels[]}`.
- `POST /order/payment-channels` `{amount, items}` returns a channel list.
  - Channel fields: `id, name, icon, channel_type, provider_type, interaction_mode, fee_policy, fee_rate, fixed_fee, min_amount, max_amount`.
- `POST /orders/create-and-pay` (preview payload + `channel_id?, use_balance`) and `POST /guest/orders/create-and-pay` (+ `email, order_password, captcha_payload`). Both return `{order_no, …}`.
- `POST /orders` and `POST /guest/orders`: plain create (defined but not called).
- `GET /orders` `{page,page_size,status?,order_no?}`; `GET /orders/stats` returns `{by_status{}}`.
- `GET /orders/:no`.
  - Fields: `order_no, status, currency, original_amount, discount_amount, promotion/wholesale/member_discount_amount, total_amount, wallet_paid_amount, online_paid_amount, refunded_amount, created_at, paid_at, expires_at, canceled_at, items[], fulfillment{}, children[], refund_records[]`.
- `POST /orders/:no/cancel`; `GET /orders/:no/fulfillment/download` (blob).
- Guest: `GET /guest/orders`, `GET /guest/orders/:no`, `GET /guest/orders/:no/fulfillment/download`, `POST /guest/payments`, `POST /guest/payments/:id/capture`, `GET /guest/payments/latest`.
- `POST /payments` `{order_no, channel_id?, use_balance?}` returns `PaymentCreateResult`:
  - `{order_paid, wallet_paid_amount, online_pay_amount, payable_amount, currency, fee_amount, fee_policy, payment_id, channel_id, provider_type, channel_type, interaction_mode, pay_url, qr_code, wallet_address, chain_amount, chain, token_id, expires_at}`
- `POST /payments/:id/capture`; `GET /payments/latest {order_no}`.

**Wallet and gift cards**
- `GET /wallet` returns `{balance}`.
- `GET /wallet/transactions` returns rows `{id,type,direction,amount,balance_after,remark,created_at}`.
- `POST /wallet/payment-channels {amount}`
- `POST /wallet/recharge {amount, channel_id, currency?, remark?}` returns `WalletRechargeResult` (recharge_no + payment fields).
- `GET /wallet/recharges`, `GET /wallet/recharges/stats`, `GET /wallet/recharges/:no`, `POST /wallet/recharge/payments/:id/capture`.
- `POST /gift-cards/redeem {code, captcha_payload?}` returns `{gift_card, wallet, transaction, wallet_delta}`.

**Affiliate:** `POST /affiliate/open`, `GET /affiliate/dashboard` (fields in §3 PersonalCenter), `GET /affiliate/commissions`, `GET /affiliate/withdraws`, `POST /affiliate/withdraws {amount,channel,account}`.

**API credential:** `GET /api-credential` returns `{status:'none'|'pending'|'approved'|'rejected', id, api_key, api_secret_tail, is_active, reject_reason}`. Also `POST /api-credential/apply`, `POST /api-credential/regenerate` (returns the new secret), `PUT /api-credential/status {is_active}`.

**Reseller:**
- Profile and apply: `GET /reseller/profile`, `POST /reseller/apply {reason}`.
- Domains: `GET|POST /reseller/domains {domain}`.
- Site config: `GET|PUT /reseller/site-config`; `POST /reseller/upload` (multipart `file`).
- Product settings: `GET /reseller/product-settings`, `GET|PUT|DELETE(?sku_id) /reseller/product-settings/:productId`, `POST …/preview`.
- Dashboard and orders: `GET /reseller/dashboard`, `GET /reseller/orders`, `GET /reseller/orders/stats`, `GET /reseller/orders/:no`.
- Finance: `GET /reseller/balance-accounts`, `GET /reseller/ledger-entries`, `GET|POST /reseller/withdraws`.

---

**Client-side storage keys to keep for compatibility:**
- localStorage: `user_token`, `user_profile`, `cart_items`, `locale`, `dujiao_theme`, `dj-storefront-template`, `dj_affiliate_attribution`, `dj_affiliate_visitor_key`, `announcement_dismiss`, `api_secret_viewed`
- sessionStorage (with a localStorage fallback): `guest_order_auth`
- sessionStorage only: `announcement_closed`
