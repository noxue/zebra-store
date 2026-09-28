/**
 * API request/response types. Field names mirror the backend DTOs exactly.
 * Amounts are two-decimal strings ("12.30"); localized fields are
 * `{ "zh-CN", "zh-TW", "en-US" }` objects (any key may be missing).
 */

export type Locale = 'zh-CN' | 'zh-TW' | 'en-US'
export type LocalizedText = Partial<Record<Locale, string>>
export type JsonObject = Record<string, unknown>

export interface CaptchaPayload {
  captcha_id?: string
  captcha_code?: string
  turnstile_token?: string
}

// ------------------------------------------------------------ site config
export type CaptchaScene = 'login' | 'register_send_code' | 'reset_send_code' | 'guest_create_order' | 'gift_card_redeem'

export interface CaptchaConfig {
  provider: 'none' | 'image' | 'turnstile' | string
  scenes?: Partial<Record<CaptchaScene, boolean>>
  turnstile?: { site_key?: string }
}

export interface NavCustomItem {
  id?: number
  title?: LocalizedText
  name?: LocalizedText
  url?: string
  link_type?: string
  target?: string
  icon?: string
  sort_order?: number
  enabled?: boolean
}

export interface NavConfig {
  builtin?: Record<string, boolean>
  custom_items?: NavCustomItem[]
}

export interface FooterLink {
  name?: LocalizedText
  title?: LocalizedText
  url: string
}

export interface AnnouncementConfig {
  enabled?: boolean
  type?: string
  title?: LocalizedText
  content?: LocalizedText
  version?: string | number
}

export interface AboutConfig {
  hero?: { title?: LocalizedText; subtitle?: LocalizedText }
  introduction?: LocalizedText
  services?: { title?: LocalizedText; items?: Array<LocalizedText | string> }
  contact?: { title?: LocalizedText; text?: LocalizedText }
}

export interface SeoConfig {
  title?: LocalizedText
  keywords?: LocalizedText
  description?: LocalizedText
  default_og_image?: string
}

export interface CustomScript {
  name?: string
  enabled?: boolean
  position?: 'head' | 'body_end' | string
  code?: string
}

export interface ThemeEffects {
  sakura?: boolean
  sparkle?: boolean
}

/** Optional anime theme block (new backend only). */
export interface ThemeConfig {
  primary_color?: string
  secondary_color?: string
  accent_color?: string
  background_image?: string
  mascot_image?: string
  login_background?: string
  effects?: ThemeEffects
  default_mode?: 'light' | 'dark' | 'system' | string
}

export interface ConfigPaymentChannel {
  id: number
  name: string
  channel_type: string
  provider_type: string
  interaction_mode: string
  min_amount?: string
  max_amount?: string
  hide_amount_out_range?: boolean
  icon?: string
  fee_rate?: string
  fixed_fee?: string
  fee_policy?: string
}

export interface SiteConfig {
  brand?: {
    site_name?: string
    site_logo?: string
    site_icon?: string
    site_description?: LocalizedText
    site_url?: string
  }
  theme?: ThemeConfig
  currency?: string
  languages?: string[]
  template_mode?: 'card' | 'list' | string
  storefront_template?: string
  nav_config?: NavConfig
  footer_links?: FooterLink[]
  contact?: { telegram?: string; whatsapp?: string }
  support?: { telegram?: string; whatsapp?: string; email?: string; support_url?: string }
  announcement?: AnnouncementConfig | null
  about?: AboutConfig
  legal?: { terms?: LocalizedText; privacy?: LocalizedText }
  captcha?: CaptchaConfig
  telegram_auth?: { enabled?: boolean; bot_username?: string; mode?: string; mini_app_url?: string }
  google_auth?: { enabled?: boolean; client_id?: string }
  registration_enabled?: boolean
  email_verification_enabled?: boolean
  email_domain_allowlist_enabled?: boolean
  allowed_email_domains?: string[]
  smtp_enabled?: boolean
  wallet_only_payment?: boolean
  wallet_recharge_channel_ids?: number[]
  payment_channels?: ConfigPaymentChannel[]
  affiliate?: {
    enabled?: boolean
    commission_rate?: number
    confirm_days?: number
    min_withdraw_amount?: number | string
    withdraw_channels?: string[]
  }
  seo?: SeoConfig
  scripts?: CustomScript[]
  tenant?: { mode?: string; host?: string }
  server_time?: number
  app_version?: string
}

// ------------------------------------------------------------ catalog
export interface Category {
  id: number
  parent_id?: number | null
  slug: string
  name: LocalizedText
  icon?: string | null
  sort_order?: number
}

export interface PromotionRule {
  id?: number
  name?: string
  type: 'percent' | 'fixed' | 'special_price' | string
  value: string
  min_amount: string
}

export interface WholesalePrice {
  sku_id?: number
  sku_code?: string
  min_quantity: number
  unit_price: string
}

export interface MemberPrice {
  member_level_id: number
  sku_id?: number
  price_amount: string
}

export type ManualFormFieldType = 'text' | 'textarea' | 'select' | 'radio' | 'checkbox' | 'number' | 'email' | 'phone'

export interface ManualFormFieldOption {
  value: string
  label?: LocalizedText | string
}

export interface ManualFormField {
  key: string
  type: ManualFormFieldType | string
  required?: boolean
  label?: LocalizedText
  placeholder?: LocalizedText
  regex?: string
  min?: number
  max?: number
  max_len?: number
  options?: Array<ManualFormFieldOption | string>
  /** Message for a value the pattern refuses (supplier widget `error`). */
  error_message?: LocalizedText
}

export interface ManualFormSchema {
  fields?: ManualFormField[]
}

export type StockStatus = 'unlimited' | 'in_stock' | 'low_stock' | 'out_of_stock'

export interface ProductSku {
  id: number
  sku_code: string
  spec_values?: Record<string, unknown> | null
  price_amount: string
  promotion_price_amount?: string
  is_active: boolean
  stock_status?: StockStatus | string
  stock_display?: string
  stock_display_mode?: string
  stock_range_min?: number
  stock_range_max?: number
  stock_quantity_hidden?: boolean
  manual_stock_total?: number
  manual_stock_locked?: number
  manual_stock_sold?: number
  auto_stock_available?: number
  upstream_stock?: number
  is_sold_out?: boolean
}

export interface RelatedPost {
  id: number
  slug: string
  type?: string
  title: LocalizedText
  summary?: LocalizedText
  thumbnail?: string
  published_at?: string
}

export interface Product {
  id: number
  category_id?: number
  slug: string
  title: LocalizedText
  description?: LocalizedText | null
  content?: LocalizedText | null
  images?: string[] | null
  tags?: string[] | null
  category?: Category | null
  price_amount: string
  promotion_id?: number
  promotion_name?: string
  promotion_type?: string
  promotion_price_amount?: string
  promotion_rules?: PromotionRule[] | null
  wholesale_prices?: WholesalePrice[] | null
  member_prices?: MemberPrice[] | null
  purchase_type: 'guest' | 'member' | string
  fulfillment_type: 'auto' | 'manual' | 'upstream' | string
  stock_status?: StockStatus | string
  stock_display?: string
  stock_display_mode?: string
  stock_quantity_hidden?: boolean
  is_sold_out?: boolean
  manual_stock_available?: number
  auto_stock_available?: number
  min_purchase_quantity?: number
  max_purchase_quantity?: number
  manual_form_schema?: ManualFormSchema | null
  payment_channel_ids?: number[] | null
  skus?: ProductSku[] | null
  related_posts?: RelatedPost[] | null
}

export interface ProductListParams {
  page?: number
  page_size?: number
  category_id?: number
  search?: string
}

export type PostType = 'blog' | 'notice'

export interface Post {
  id: number
  slug: string
  type: PostType | string
  title: LocalizedText
  summary?: LocalizedText | null
  content?: LocalizedText | null
  thumbnail?: string
  published_at?: string
  related_products?: Product[] | null
}

export interface PostListParams {
  type?: PostType
  page?: number
  page_size?: number
  search?: string
}

export interface Banner {
  id: number
  position?: string
  title?: LocalizedText
  subtitle?: LocalizedText
  image: string
  mobile_image?: string
  link_type?: 'none' | 'internal' | 'external' | string
  link_value?: string
  open_in_new_tab?: boolean
}

export interface PublicMemberLevel {
  id: number
  name: LocalizedText
  slug: string
  icon: string
  discount_rate: number
  recharge_threshold: number
  spend_threshold: number
  is_default: boolean
  sort_order: number
}

export interface CaptchaImage {
  captcha_id: string
  image_base64: string
}

// ------------------------------------------------------------ auth / user
export interface UserProfileData {
  id: number
  email: string
  nickname: string
  email_verified_at?: string | null
  locale?: string
  member_level_id?: number
  total_recharged?: number | string
  total_spent?: number | string
  email_change_mode?: 'bind_only' | 'change_with_old_and_new'
  password_change_mode?: 'set_without_old' | 'change_with_old'
}

export interface AuthUser {
  id: number
  email: string
  nickname?: string
  email_verified_at?: string | null
  locale?: string
}

export interface LoginResult {
  token?: string
  expires_at?: string
  user?: AuthUser
  requires_totp?: boolean
  challenge_token?: string
  challenge_expires_at?: string
}

export interface LoginPayload {
  email: string
  password: string
  remember_me?: boolean
  captcha_payload?: CaptchaPayload
}

export interface RegisterPayload {
  email: string
  password: string
  code: string
  agreement_accepted: boolean
}

export interface SendVerifyCodePayload {
  email: string
  purpose: 'register' | 'reset'
  captcha_payload?: CaptchaPayload
}

export interface ForgotPasswordPayload {
  email: string
  code: string
  new_password: string
}

export interface OidcStartResult {
  url?: string
  auth_url?: string
  authorize_url?: string
  state?: string
}

export interface UpdateUserProfilePayload {
  nickname?: string
  locale?: string
}

export interface UserLoginLogItem {
  id: number
  user_id?: number
  email: string
  status: string
  fail_reason?: string
  client_ip?: string
  user_agent?: string
  login_source?: string
  created_at?: string
}

export interface SendChangeEmailCodePayload {
  kind: 'old' | 'new'
  new_email?: string
}

export interface ChangeEmailPayload {
  new_email: string
  old_code?: string
  new_code: string
}

export interface ChangeUserPasswordPayload {
  old_password?: string
  new_password: string
}

export interface TelegramAuthPayload {
  id: number
  first_name?: string
  last_name?: string
  username?: string
  photo_url?: string
  auth_date: number
  hash: string
}

export interface TelegramMiniAppAuthPayload {
  init_data: string
}

export interface GoogleCredentialPayload {
  credential: string
}

export interface TelegramBindingData {
  bound: boolean
  provider?: string
  provider_user_id?: string
  username?: string
  avatar_url?: string
  auth_at?: string | null
  updated_at?: string | null
  can_unbind?: boolean
}

export interface GoogleBindingData extends TelegramBindingData {
  email?: string
  display_name?: string
}

export interface TotpStatus {
  enabled: boolean
  enabled_at?: string | null
  recovery_codes_remaining?: number
  recovery_codes_total?: number
}

export interface TotpSetupResult {
  secret: string
  otpauth_url?: string
  qr_code?: string
  issuer?: string
  account?: string
}

export interface TotpEnableResult {
  recovery_codes?: string[]
}

// ------------------------------------------------------------ orders
export interface OrderItemInput {
  product_id: number
  sku_id?: number
  quantity: number
  fulfillment_type?: string
}

export type ManualFormData = Record<string, Record<string, unknown>>

export interface OrderPreviewPayload {
  coupon_code?: string
  affiliate_code?: string
  affiliate_visitor_key?: string
  items: OrderItemInput[]
  manual_form_data?: ManualFormData
}

export interface GuestOrderPreviewPayload extends OrderPreviewPayload {
  email: string
  order_password: string
}

export interface CreateAndPayPayload extends OrderPreviewPayload {
  channel_id?: number
  use_balance?: boolean
}

export interface GuestCreateAndPayPayload extends CreateAndPayPayload {
  email: string
  order_password: string
  captcha_payload?: CaptchaPayload
}

export interface SkuSnapshot {
  image?: string
  sku_code?: string
  sku_id?: number
  spec_values?: Record<string, unknown> | null
}

export interface OrderItem {
  id?: number
  product_id?: number
  sku_id?: number
  title: LocalizedText
  sku_snapshot?: SkuSnapshot | null
  tags?: string[] | null
  quantity: number
  original_unit_price?: string
  unit_price: string
  original_total_price?: string
  total_price: string
  coupon_discount_amount?: string
  member_discount_amount?: string
  promotion_discount_amount?: string
  wholesale_discount_amount?: string
  fulfillment_type?: string
  manual_form_schema_snapshot?: ManualFormSchema | null
  manual_form_submission?: Record<string, unknown> | null
  instructions?: LocalizedText | string | null
  slug?: string
  product_slug?: string
}

export interface OrderPreview {
  currency: string
  original_amount: string
  discount_amount: string
  promotion_discount_amount: string
  wholesale_discount_amount?: string
  member_discount_amount?: string
  total_amount: string
  items: OrderItem[]
  payment_channels?: PaymentChannel[]
}

export interface Fulfillment {
  type?: string
  status?: string
  payload?: string
  payload_line_count?: number
  payload_truncated?: boolean
  delivered_at?: string | null
  logistics?: Record<string, unknown> | null
  delivery_data?: Record<string, unknown> | null
}

export interface RefundRecord {
  id?: number
  amount: string
  currency?: string
  remark?: string
  created_at?: string
}

export type OrderStatus =
  | 'pending_payment'
  | 'paid'
  | 'fulfilling'
  | 'partially_delivered'
  | 'partially_refunded'
  | 'delivered'
  | 'completed'
  | 'expired'
  | 'canceled'
  | 'refunded'

export interface Order {
  id?: number
  order_no: string
  parent_order_no?: string
  guest_email?: string
  guest_locale?: string
  status: OrderStatus | string
  currency: string
  original_amount?: string
  discount_amount?: string
  promotion_discount_amount?: string
  wholesale_discount_amount?: string
  member_discount_amount?: string
  total_amount: string
  wallet_paid_amount?: string
  online_paid_amount?: string
  refunded_amount?: string
  created_at: string
  paid_at?: string | null
  expires_at?: string | null
  canceled_at?: string | null
  items?: OrderItem[]
  fulfillment?: Fulfillment | null
  children?: Order[] | null
  refund_records?: RefundRecord[] | null
}

export interface OrderListParams {
  page?: number
  page_size?: number
  status?: string
  order_no?: string
}

export interface StatusStats {
  total?: number
  by_status: Record<string, number>
}

export interface CreateOrderResult {
  order_no?: string
  order?: Order
  order_paid?: boolean
  payment_id?: number
  [key: string]: unknown
}

// ------------------------------------------------------------ payments
export interface PaymentChannel {
  id: number
  name: string
  icon?: string
  channel_type: string
  provider_type: string
  interaction_mode: string
  fee_policy?: string
  fee_rate?: string
  fixed_fee?: string
  min_amount?: string
  max_amount?: string
  hide_amount_out_range?: boolean
}

export interface PaymentChannelsPayload {
  amount: string
  items?: OrderItemInput[]
}

export interface CreatePaymentPayload {
  order_no: string
  channel_id?: number
  use_balance?: boolean
}

export interface PaymentCreateResult {
  order_paid?: boolean
  wallet_paid_amount?: string
  online_pay_amount?: string
  payable_amount?: string
  /** Actual currency of payable_amount (may differ from order currency). */
  currency?: string
  fee_amount?: string
  fee_policy?: string
  payment_id?: number
  order_no?: string
  channel_id?: number
  provider_type?: string
  channel_type?: string
  interaction_mode?: string
  pay_url?: string
  qr_code?: string
  wallet_address?: string
  chain_amount?: string
  chain?: string
  token_id?: string
  expires_at?: string | null
  status?: string
}

export interface PaymentCaptureResult {
  status?: string
  order_status?: string
  payment?: PaymentCreateResult
  [key: string]: unknown
}

// ------------------------------------------------------------ wallet
export interface WalletAccountData {
  balance: string
}

export interface WalletTransactionData {
  id: number
  type: string
  direction: string
  amount: string
  balance_after: string
  remark: string
  created_at: string
}

export interface WalletRechargePayload {
  amount: string
  channel_id: number
  currency?: string
  remark?: string
}

export interface WalletRechargeOrderData {
  id: number
  recharge_no: string
  amount: string
  payable_amount: string
  fee_amount: string
  currency: string
  status: string
  remark: string
  paid_at?: string | null
  created_at: string
  payment?: PaymentCreateResult | null
  payment_id?: number
  channel_id?: number
}

export interface WalletRechargeResult extends PaymentCreateResult {
  recharge?: WalletRechargeOrderData
  recharge_no?: string
  recharge_status?: string
  account?: WalletAccountData
}

export interface GiftCardData {
  id: number
  name: string
  code: string
  amount: string
  currency: string
  status: string
  redeemed_at?: string
}

export interface GiftCardRedeemResult {
  gift_card: GiftCardData
  wallet: WalletAccountData
  transaction: WalletTransactionData
  wallet_delta: string
}

// ------------------------------------------------------------ affiliate
export interface AffiliateDashboardData {
  opened: boolean
  affiliate_code: string
  promotion_path: string
  click_count: number
  valid_order_count: number
  conversion_rate: number
  pending_commission: string
  available_commission: string
  withdrawn_commission: string
}

export interface AffiliateCommissionData {
  id: number
  order_no?: string
  commission_type: string
  base_amount?: string
  rate_percent?: string
  commission_amount: string
  status: string
  confirm_at?: string
  available_at?: string
  created_at: string
}

export interface AffiliateWithdrawData {
  id: number
  amount: string
  channel: string
  account: string
  status: string
  reject_reason?: string
  created_at: string
}

export interface AffiliateWithdrawApplyPayload {
  amount: string
  channel: string
  account: string
}

export interface AffiliateClickPayload {
  affiliate_code: string
  visitor_key?: string
  landing_path?: string
  referrer?: string
}

// ------------------------------------------------------------ api credential
export interface ApiCredentialData {
  status: 'none' | 'pending' | 'approved' | 'rejected' | string
  id?: number
  api_key?: string
  api_secret_tail?: string
  api_secret_masked?: string
  is_active?: boolean
  reject_reason?: string
  created_at?: string
  approved_at?: string
  /** A rotated secret is waiting: old and new secrets are both valid until `rotation_expires_at`. */
  rotation_pending?: boolean
  rotation_expires_at?: string | null
  /** Integration protocols this site serves for the credential. */
  protocols?: string[]
}

export interface ApiCredentialRegenerateResult {
  api_key?: string
  api_secret: string
}

/** POST /api-credential/connection-code — the code embeds a freshly rotated secret (shown once). */
export interface ApiConnectionCodeResult {
  code: string
  rotation_expires_at: string | null
}

/** POST /api-credential/rotate — new secret (shown once); the old one stays valid until expiry. */
export interface ApiCredentialRotateResult {
  api_secret: string
  rotation_expires_at: string | null
}

/** A provider-compat protocol served for PHP shop systems (`acg-faka` → /shared/, `mcy-open-api` → /plugin/open-api/). */
export interface ApiCompatProtocol {
  id: 'acg-faka' | 'mcy-open-api' | string
  /** Switched on by the site (`integration.acg_faka_compat` / `integration.mcy_compat`). */
  enabled: boolean
  path: string
}

/** GET|PUT /api-credential/compat, POST /api-credential/compat/issue — 异次元 / 萌次元 downstream credentials. */
export interface ApiCompatKeyData {
  /** 商户ID / API-ID to enter downstream (the user id). */
  app_id: string
  /** Empty until issued (or after a re-approval invalidated it). */
  app_key: string
  is_active: boolean
  /** Comma separated IPs / CIDRs; empty = any address. */
  ip_allowlist: string
  last_used_at: string | null
  /** Site address to enter as the upstream domain. */
  site_url: string
  protocols: ApiCompatProtocol[]
}

export interface ApiCompatKeyUpdate {
  is_active?: boolean
  ip_allowlist?: string
}

// ------------------------------------------------------------ reseller
export interface ResellerProfileSummaryData {
  id: number
  status: string
  settlement_status: string
  created_at: string
}

export interface ResellerManagementProfileData {
  id: number
  status: string
  apply_reason?: string
  reject_reason?: string
  default_markup_percent: string
  max_markup_percent: string
  settlement_status: string
  reviewed_at?: string
  created_at: string
  updated_at: string
}

export interface ResellerDomainData {
  id: number
  domain: string
  type: string
  verification_token?: string
  verification_status: string
  status: string
  is_primary: boolean
  verified_at?: string
  created_at: string
  updated_at: string
}

export interface ResellerManagementSnapshotData {
  opened: boolean
  can_apply: boolean
  profile?: ResellerManagementProfileData
  domains: ResellerDomainData[]
}

export interface ResellerApplyPayload {
  reason?: string
}

export interface ResellerCustomDomainPayload {
  domain: string
}

export type ResellerLocalizedText = Record<Locale, string>

export interface ResellerSiteConfigPayload {
  site_name: string
  logo?: string
  favicon?: string
  announcement?: {
    enabled: boolean
    type: string
    title: ResellerLocalizedText
    content: ResellerLocalizedText
  }
  support?: {
    telegram?: string
    whatsapp?: string
    email?: string
    support_url?: string
  }
  seo?: {
    title: ResellerLocalizedText
    keywords: ResellerLocalizedText
    description: ResellerLocalizedText
    default_og_image?: string
  }
  footer_links?: Array<{ name: ResellerLocalizedText; url: string }>
  nav_config?: {
    builtin: Record<string, boolean>
    custom_items: Array<{ name: ResellerLocalizedText; url: string }>
  }
}

export interface ResellerSiteConfigData extends ResellerSiteConfigPayload {
  id: number
  updated_at: string
}

export interface ResellerSiteConfigSnapshotData {
  opened: boolean
  can_edit: boolean
  config?: ResellerSiteConfigData
}

export interface ResellerUploadResult {
  url: string
  path?: string
}

export type ResellerPricingMode = 'inherit' | 'markup_percent' | 'fixed_markup' | 'fixed_price'

export interface ResellerProductSettingData {
  id?: number
  product_id: number
  sku_id: number
  is_listed: boolean
  pricing_mode: ResellerPricingMode | string
  markup_percent: string
  fixed_markup_amount: string
  fixed_price_amount: string
  effective_price_amount?: string
  rule_source?: string
  sort_order: number
  updated_at?: string
}

export interface ResellerProductSettingSKUData {
  id: number
  sku_code: string
  spec_values: Record<string, unknown>
  base_price_amount: string
  is_active: boolean
  setting?: ResellerProductSettingData
  effective_price_amount?: string
}

export interface ResellerProductSettingProductData {
  id: number
  slug: string
  title: LocalizedText
  price_amount: string
  is_active: boolean
}

/**
 * Product setting detail. `GET /reseller/product-settings` returns a page of
 * these same objects (original `NewResellerProductSettingListResp`).
 */
export interface ResellerProductSettingDetailData {
  product: ResellerProductSettingProductData
  product_setting?: ResellerProductSettingData
  skus: ResellerProductSettingSKUData[]
}

/** Query filters accepted by `GET /reseller/product-settings`. */
export interface ResellerProductSettingListParams extends PageParams {
  keyword?: string
  category_id?: number
  /** `configured` | `unconfigured` */
  configured?: 'configured' | 'unconfigured'
  /** `listed` | `hidden` */
  listed?: 'listed' | 'hidden'
}

export interface ResellerProductSettingPayloadItem {
  sku_id: number
  is_listed: boolean
  pricing_mode: ResellerPricingMode | string
  markup_percent: string
  fixed_markup_amount: string
  fixed_price_amount: string
  sort_order: number
}

export interface ResellerProductSettingUpdatePayload {
  settings: ResellerProductSettingPayloadItem[]
}

export interface ResellerProductSettingPreviewItem {
  sku_id: number
  is_listed: boolean
  base_price_amount: string
  effective_price_amount: string
  valid: boolean
  error_code?: string
}

export interface ResellerProductSettingPreviewData {
  items: ResellerProductSettingPreviewItem[]
}

export interface ResellerBalanceData {
  id: number
  currency: string
  status: string
  available_amount: string
  locked_amount: string
  negative_amount: string
  updated_at: string
}

export interface ResellerLedgerData {
  id: number
  order_id?: number
  type: string
  amount: string
  currency: string
  status: string
  available_at?: string
  withdraw_request_id?: number
  created_at: string
}

export interface ResellerWithdrawData {
  id: number
  amount: string
  currency: string
  channel: string
  account: string
  status: string
  reject_reason?: string
  processed_at?: string
  created_at: string
}

export interface ResellerDashboardData {
  opened: boolean
  profile?: ResellerProfileSummaryData
  balances?: ResellerBalanceData[]
  withdraw_enabled: boolean
  withdraw_disabled_reason?: string
}

export interface ResellerOrderListParams {
  page?: number
  page_size?: number
  status?: string
  order_no?: string
  created_from?: string
  created_to?: string
  paid_from?: string
  paid_to?: string
}

export type ResellerOrderStatsParams = ResellerOrderListParams

export interface ResellerOrderData {
  order_no: string
  status: string
  currency: string
  total_amount: string
  base_amount: string
  profit_amount: string
  profit_status: 'credited' | 'pending' | 'unavailable' | string
  domain: string
  buyer_label: string
  items_count: number
  created_at: string
  paid_at?: string | null
}

export interface ResellerOrderItemData {
  title: LocalizedText
  sku_snapshot: Record<string, unknown>
  quantity: number
  unit_price: string
  total_price: string
  base_unit_amount?: string
  reseller_unit_amount?: string
  base_total_amount?: string
  reseller_total_amount?: string
  profit_amount?: string
}

export interface ResellerOrderDetailData extends ResellerOrderData {
  items: ResellerOrderItemData[]
}

export interface ResellerOrderStatsData {
  total: number
  by_status: Record<string, number>
  by_currency: Record<string, number>
}

export interface ResellerWithdrawApplyPayload {
  amount: string
  currency: string
  channel: string
  account: string
}

export interface PageParams {
  page?: number
  page_size?: number
}

// @section shop-extra (Home/Products/ProductDetail/Cart additions go below)

// @section order-extra (Checkout/Payment/Order detail additions go below)

/** Payment channel lookup for an existing order (Payment page). */
export interface OrderPaymentChannelsPayload extends PaymentChannelsPayload {
  order_no?: string
}

/** Extra order fields read by the payment page. */
export interface Order {
  allowed_payment_channel_ids?: number[] | null
}

export interface RechargePaymentData extends PaymentCreateResult {
  id?: number
}

/** `GET /wallet/recharges/:no` / capture response (recharge + payment). */
export interface RechargeDetailData extends PaymentCreateResult {
  recharge?: WalletRechargeOrderData | null
  payment?: RechargePaymentData | null
}

// @section content-extra (Blog/About/Auth additions go below)

// @section personal-extra (Personal center additions go below)
/** 2FA enable may rotate the session token. */
export interface TotpEnableWithTokenResult extends TotpEnableResult {
  token?: string
}

// @section reseller-extra (Reseller console additions go below)
