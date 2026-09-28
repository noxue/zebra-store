// Admin API wire types; amounts are decimal strings on the wire.
export interface Pagination {
  page: number
  page_size: number
  total: number
  total_page: number
}

export interface ApiResponse<T = unknown> {
  status_code: number
  msg: string
  data: T
  pagination?: Pagination
}

/** Money as returned by the backend: two-decimal string (older fields may be numbers). */
export type Money = string | number

// --- Localized JSON (multi-language content) ---
export type LocalizedText = Record<string, string>

// --- Category ---
export interface AdminCategory {
  id: number
  parent_id: number
  slug: string
  name: LocalizedText
  icon: string
  sort_order: number
  is_active: boolean
  created_at: string
}

// --- Product ---
export interface AdminProductSKU {
  id: number
  product_id: number
  sku_code: string
  spec_values: Record<string, string>
  price_amount: Money
  cost_price_amount: Money
  manual_stock_total: number
  manual_stock_locked: number
  manual_stock_sold: number
  auto_stock_available: number
  auto_stock_total: number
  is_active: boolean
  sort_order: number
  created_at: string
  updated_at: string
}

export interface AdminWholesalePrice {
  sku_id?: number
  sku_code?: string
  min_quantity: number
  unit_price: Money | string
}

export interface AdminProduct {
  id: number
  category_id: number
  slug: string
  seo_meta: Record<string, LocalizedText> | null
  title: LocalizedText
  description: LocalizedText | null
  content: LocalizedText | null
  instructions?: LocalizedText | null
  price_amount: Money
  cost_price_amount: Money
  wholesale_prices?: AdminWholesalePrice[]
  images: string[] | null
  tags: string[] | null
  purchase_type: string
  min_purchase_quantity: number
  max_purchase_quantity: number
  stock_display_mode: string
  fulfillment_type: string
  manual_form_schema: Record<string, unknown> | null
  manual_stock_total: number
  manual_stock_locked: number
  manual_stock_sold: number
  payment_channel_ids: string
  is_affiliate_enabled: boolean
  auto_stock_available: number
  auto_stock_total: number
  auto_stock_locked: number
  auto_stock_sold: number
  is_mapped: boolean
  is_active: boolean
  sort_order: number
  created_at: string
  updated_at: string
  category?: AdminCategory
  skus?: AdminProductSKU[]
}

export interface AdminNotificationLog {
  id: number
  event_type: string
  biz_type: string
  biz_id: number
  channel: string
  recipient: string
  locale: string
  title: string
  body: string
  status: string
  error_message: string
  is_test: boolean
  variables?: Record<string, unknown>
  created_at: string
}

// --- Order ---
export interface AdminOrderItem {
  id: number
  order_id: number
  product_id: number
  sku_id: number
  title: LocalizedText
  sku_snapshot?: {
    sku_id?: number
    sku_code?: string
    spec_values?: unknown
    image?: string
    [key: string]: unknown
  }
  quantity: number
  original_unit_price: Money
  unit_price: Money
  cost_price: Money
  original_total_price: Money
  total_price: Money
  fulfillment_type: string
  created_at: string
  promotion_id?: number
  promotion_name?: string
  tags?: string[]
  manual_form_schema_snapshot?: Record<string, unknown>
  manual_form_submission?: Record<string, unknown>
  coupon_discount_amount?: Money
  promotion_discount_amount?: Money
  member_discount_amount?: Money
  wholesale_discount_amount?: Money
}

export interface AdminFulfillment {
  id: number
  order_id: number
  type: string
  status: string
  content: string
  created_at: string
  updated_at: string
  payload?: string
  payload_line_count?: number
  delivery_data?: Record<string, unknown>
}

export interface AdminOrder {
  id: number
  order_no: string
  parent_id?: number
  user_id?: number
  guest_email?: string
  guest_locale?: string
  status: string
  currency: string
  original_amount: Money
  discount_amount: Money
  promotion_discount_amount: Money
  wholesale_discount_amount?: Money
  member_discount_amount?: Money
  total_amount: Money
  wallet_paid_amount: Money
  online_paid_amount: Money
  refunded_amount: Money
  coupon_id?: number
  coupon_code?: string
  promotion_id?: number
  affiliate_profile_id?: number
  affiliate_code?: string
  client_ip?: string
  expires_at?: string
  paid_at?: string
  canceled_at?: string
  created_at: string
  updated_at: string
  items?: AdminOrderItem[]
  fulfillment?: AdminFulfillment
  children?: AdminOrder[]
  payments?: AdminPayment[]
  user_display_name?: string
  user_email?: string
}

export interface AdminUserOAuthIdentity {
  id: number
  provider: string
  provider_user_id: string
  username?: string
  avatar_url?: string
  auth_at?: string
  created_at: string
}

// --- CardSecret ---
export interface AdminCardSecretBatch {
  id: number
  product_id: number
  sku_id: number
  name: string
  batch_no?: string
  source?: string
  note?: string
  total_count: number
  available_count: number
  reserved_count: number
  used_count: number
  created_at: string
}

export interface AdminCardSecret {
  id: number
  product_id: number
  sku_id: number
  batch_id?: number
  secret: string
  status: string
  order_id?: number
  reserved_at?: string
  used_at?: string
  created_at: string
  updated_at: string
  batch?: AdminCardSecretBatch
}

// --- Coupon ---
export interface AdminCoupon {
  id: number
  code: string
  type: string
  value: Money
  min_amount: Money
  max_discount: Money
  usage_limit: number
  used_count: number
  per_user_limit: number
  disabled_wholesale_price?: boolean
  per_item_discount?: boolean
  payment_roles?: string[]
  member_levels?: number[]
  scope_type: string
  scope_ref_ids: number[] | string
  starts_at?: string
  ends_at?: string
  is_active: boolean
  created_at: string
  updated_at: string
}

// --- GiftCard ---
export interface AdminGiftCard {
  id: number
  batch_id?: number
  name: string
  code: string
  amount: Money | string
  currency: string
  status: string
  expires_at?: string
  redeemed_at?: string
  redeemed_user_id?: number
  wallet_txn_id?: number
  is_expired?: boolean
  redeemed_user?: { id?: number; display_name?: string; email?: string }
  batch?: AdminGiftCardBatch
  created_at: string
  updated_at: string
}

export interface AdminGiftCardBatch {
  id: number
  name: string
  batch_no?: string
  total_count: number
  amount: Money
  currency: string
  created_at: string
}

// --- MemberLevel ---
export interface AdminMemberLevel {
  id: number
  name: LocalizedText
  slug: string
  icon: string
  discount_rate: Money
  recharge_threshold: Money
  spend_threshold: Money
  is_default: boolean
  sort_order: number
  is_active: boolean
  created_at: string
  updated_at: string
}

export interface AdminMemberLevelPrice {
  id: number
  member_level_id: number
  product_id: number
  sku_id: number
  price_amount: Money
  created_at: string
  updated_at: string
}

// --- Promotion ---
export interface AdminPromotion {
  id: number
  name: string
  scope_type: string
  scope_ref_id: number
  type: string
  value: Money
  min_amount: Money
  starts_at?: string
  ends_at?: string
  is_active: boolean
  created_at: string
  updated_at: string
}

// --- Banner ---
// --- Media ---
export interface AdminMedia {
  id: number
  name: string
  filename: string
  path: string
  mime_type: string
  size: number
  scene: string
  width: number
  height: number
  created_at: string
}

export interface AdminBanner {
  id: number
  name: string
  position: string
  title: LocalizedText
  subtitle: LocalizedText
  image: string
  mobile_image: string
  link_type: string
  link_value: string
  open_in_new_tab: boolean
  is_active: boolean
  start_at?: string
  end_at?: string
  sort_order: number
  created_at: string
  updated_at: string
}

export interface AdminTelegramBotRuntimeStatus {
  connected: boolean
  last_seen_at?: string
  bot_version?: string
  webhook_status?: string
  machine_code?: string
  license_status?: string
  license_expires_at?: string
  warnings?: string[]
  config_version: number
  last_config_sync_at?: string
}

export interface AdminTelegramBroadcast {
  id: number
  title: string
  recipient_type: string
  filters?: Record<string, unknown>
  recipient_count: number
  success_count: number
  failed_count: number
  status: string
  message_html: string
  attachment_url?: string
  attachment_name?: string
  started_at?: string
  completed_at?: string
  last_error?: string
  created_at: string
  updated_at: string
}

export interface AdminTelegramBroadcastUser {
  user_id: number
  display_name: string
  user_email: string
  telegram_username: string
  telegram_user_id: string
  bound_at: string
  user_created_at: string
}

// --- Post ---
export interface AdminPost {
  id: number
  slug: string
  type: string
  title: LocalizedText
  summary: LocalizedText
  content: LocalizedText
  thumbnail: string
  is_published: boolean
  published_at?: string
  created_at: string
  category_id?: number | null
  category?: { id: number; name: LocalizedText } | null
}

// --- PaymentChannel ---
export interface AdminPaymentChannel {
  id: number
  name: string
  provider_type: string
  channel_type: string
  interaction_mode: string
  fee_rate: Money | string
  fixed_fee?: number | string
  min_amount?: Money | string
  max_amount?: Money | string
  hide_amount_out_range?: boolean
  payment_types?: string[]
  payment_roles?: string[]
  member_levels?: number[]
  config_json: Record<string, unknown>
  icon: string
  is_active: boolean
  sort_order: number
  created_at: string
  updated_at: string
}

export interface AdminGatewaySecurityTestResult {
  verification_mode: string
  response_serial: string
  request_signature_accepted: boolean
  response_signature_valid: boolean
  echo_message_matched: boolean
}

// --- Payment ---
export interface AdminPayment {
  id: number
  payment_no: string
  order_id: number
  channel_id: number
  provider_type: string
  channel_type: string
  display_channel_type?: string
  interaction_mode: string
  amount: Money
  payable_amount: Money
  fee_rate: Money | string
  fixed_fee?: number | string
  fee_amount: Money
  fee_policy?: 'none' | 'merchant_absorbed' | 'customer_surcharge' | 'legacy_customer_surcharge'
  exception_code?: string
  superseded_at?: string
  superseded_by_payment_id?: number
  currency: string
  status: string
  provider_trade_no?: string
  paid_at?: string
  expired_at?: string
  created_at: string
  updated_at: string
  order_no?: string
  channel_name?: string
  recharge_no?: string
  recharge_user_id?: number
  recharge_status?: string
  provider_ref?: string
  pay_url?: string
  qr_code?: string
  provider_payload?: Record<string, unknown>
}

// --- OrderRefund ---
export interface AdminOrderRefund {
  id: number
  user_id: number
  guest_email?: string
  guest_locale?: string
  order_id: number
  order_no?: string
  type: string
  refund_type_label?: string
  amount: string
  payment_fee_refunded: boolean
  payment_fee_refunded_amount: string
  currency: string
  remark?: string
  items?: AdminOrderItem[]
  user_email?: string
  user_display_name?: string
  created_at: string
  updated_at: string
}

// --- User ---
export interface AdminUser {
  id: number
  email: string
  display_name: string
  nickname?: string
  locale: string
  status: string
  member_level_id?: number
  total_recharged?: number | string
  total_spent?: number | string
  wallet_balance?: number | string
  admin_note?: string
  email_verified_at?: string
  email_verified?: boolean
  last_login_at?: string
  created_at: string
  updated_at: string
  oauth_identities?: AdminUserOAuthIdentity[]
  [key: string]: unknown
}

// --- UserLoginLog ---
export interface AdminUserLoginLog {
  id: number
  user_id: number
  email: string
  client_ip: string
  user_agent: string
  login_source?: string
  status: string
  fail_reason?: string
  created_at: string
}

// --- SiteConnection ---
export interface AdminSiteConnection {
  id: number
  name: string
  type: string
  base_url: string
  api_key: string
  api_secret?: string
  protocol?: string
  /** Adapter-specific configuration (keys come from the protocol's `fields`). */
  config?: Record<string, unknown> | null
  /** Adapter-specific configuration as stored by the backend (`secret` fields come back empty). */
  extra?: Record<string, unknown> | null
  callback_url?: string
  retry_max?: number
  retry_intervals?: string
  status?: string
  is_active: boolean
  last_ping_at?: string
  last_ping_status?: string
  exchange_rate?: Money
  price_markup_percent?: number
  price_rounding_mode?: string
  auto_sync_price?: boolean
  /** Capabilities negotiated at handshake; legacy protocols return an empty list. */
  features?: string[]
  /** Settlement currency of the supplier site. */
  supplier_currency?: string
  sync_mode?: SiteConnectionSyncMode
  last_change_seq?: number | null
  webhook_status?: string
  created_at: string
  updated_at: string
}

export type SiteConnectionSyncMode = 'incremental' | 'full'

export type SiteConnectionFieldKind = 'text' | 'secret' | 'url' | 'select'

export interface SiteConnectionFieldOption {
  value: string
  label: LocalizedText
}

/** One adapter-specific connection field, rendered dynamically in the connection form. */
export interface SiteConnectionProtocolField {
  key: string
  label: LocalizedText
  kind: SiteConnectionFieldKind
  required: boolean
  placeholder?: LocalizedText | null
  options?: SiteConnectionFieldOption[] | null
}

/** GET /admin/site-connections/protocols — one supplier adapter. */
export interface SiteConnectionProtocolDef {
  id: string
  name: LocalizedText
  description: LocalizedText
  fields: SiteConnectionProtocolField[]
  capabilities: string[]
  supports_connection_code: boolean
}

/** Adapter-specific values (`{field.key: value}`) sent with create/update/handshake. */
export type SiteConnectionConfig = Record<string, string>

/** POST /admin/site-connections/parse-code */
export interface SiteConnectionCodeParseResult {
  name: string
  base_url: string
  api_key: string
  api_secret: string
  protocol: string
  config?: Record<string, unknown> | null
}

export interface SiteConnectionHandshakeRequest {
  base_url: string
  api_key: string
  api_secret: string
  protocol: string
  config: SiteConnectionConfig
}

/** POST /admin/site-connections/handshake */
export interface SiteConnectionHandshakeResult {
  ok: boolean
  protocol: string
  version: string
  site: { name: string; url: string; currency: string }
  features: string[]
  limits: Record<string, unknown>
  account: { balance: Money; currency: string }
  suggested_exchange_rate: string | null
  suggested_callback_url: string | null
  error?: string
}

// --- ProductMapping ---
export type UpstreamProductStatus = 'active' | 'inactive' | 'deleted'

export interface AdminProductMapping {
  id: number
  connection_id: number
  local_product_id: number
  upstream_product_id: number
  upstream_sku_id: number
  upstream_product_name: string
  upstream_sku_name: string
  upstream_price: Money
  upstream_currency: string
  is_active: boolean
  upstream_status?: UpstreamProductStatus
  last_sync_at?: string
  created_at: string
  updated_at: string
  connection_name?: string
  local_product_title?: string
  last_synced_at?: string
  product?: { id: number; title?: LocalizedText; skus?: AdminProductSKU[] }
  [key: string]: unknown
}

// --- ProcurementOrder ---
export interface AdminProcurementOrder {
  id: number
  connection_id: number
  local_order_id: number
  local_order_no: string
  upstream_order_no?: string
  status: string
  upstream_amount: Money
  upstream_currency: string
  local_sell_amount: Money
  local_sell_currency: string
  error_message?: string
  retry_count: number
  next_retry_at?: string
  trace_id: string
  upstream_payload?: string
  upstream_payload_line_count?: number
  created_at: string
  updated_at: string
  connection_name?: string
  connection?: { id: number; name: string; type?: string; base_url?: string }
  upstream_refund_records?: Array<Record<string, unknown>>
  upstream_refunded_amount?: string
  [key: string]: unknown
}

// --- Reconciliation ---
export interface AdminReconciliationJob {
  id: number
  connection_id: number
  type: string
  status: string
  time_range_start: string
  time_range_end: string
  total_items: number
  matched_items: number
  mismatched_items: number
  mismatched_count?: number
  total_count?: number
  matched_count?: number
  missing_items: number
  created_at: string
  updated_at: string
  started_at?: string
  finished_at?: string
  result_json?: string
  connection_name?: string
  connection?: { name?: string; [key: string]: unknown }
  [key: string]: unknown
}

export interface AdminReconciliationItem {
  id: number
  job_id: number
  type: string
  local_order_no?: string
  upstream_order_no?: string
  local_amount?: Money
  upstream_amount?: Money
  local_status?: string
  upstream_status?: string
  mismatch_type?: string
  status: string
  resolution?: string
  resolved?: boolean
  remark?: string
  resolved_by?: string
  resolved_at?: string
  created_at: string
}

// --- ApiCredential ---
export interface AdminApiCredential {
  id: number
  user_id: number
  name: string
  api_key: string
  status: string
  is_active: boolean
  permissions: string[]
  ip_whitelist: string[]
  rate_limit: number
  last_used_at?: string
  approved_at?: string
  reject_reason?: string
  created_at: string
  updated_at: string
  user_email?: string
  user?: { id: number; email: string; display_name?: string }
}

// --- Dashboard ---
export interface AdminDashboardOverview {
  total_orders: number
  total_revenue: Money
  total_users: number
  total_products: number
  today_orders: number
  today_revenue: Money
  today_users: number
  pending_orders: number
}

export interface AdminDashboardTrendPoint {
  date: string
  orders: number
  revenue: number
  users: number
}

export interface AdminDashboardRanking {
  product_id: number
  product_title: string
  total_orders: number
  total_revenue: Money
}

export interface AdminDashboardInventoryAlert {
  product_id: number
  sku_id?: number
  product_title: Record<string, string>
  sku_code?: string
  sku_spec_values?: Record<string, string>
  fulfillment_type: string
  alert_type: string
  available_stock: number
}

// --- Ad Proxy ---
export interface AdRenderSlotDTO {
  code: string
  scene: string
  layout: string
  render_mode: string
  max_items: number
}

export interface AdRenderItemDTO {
  id: number
  advertiser_name: string
  title: string
  subtitle: string
  cta_label: string
  badge: string
  image: string
  mobile_image: string
  icon: string
  link_type: string
  open_in_new_tab: boolean
  theme: string
  dismissible: boolean
  click_url: string
  impression_token: string
}

export interface AdRenderResponse {
  slot: AdRenderSlotDTO
  items: AdRenderItemDTO[]
}

// --- Affiliate ---
export interface AdminAffiliateUser {
  id: number
  user_id: number
  code: string
  status: string
  total_earnings: Money
  pending_earnings: Money
  confirmed_earnings: Money
  withdrawn_earnings: Money
  total_referrals: number
  created_at: string
  updated_at: string
  user_email?: string
  user_display_name?: string
}

export interface AdminAffiliateCommission {
  id: number
  affiliate_profile_id: number
  order_id: number
  order_no: string
  amount: Money
  rate: Money
  base_amount?: Money
  rate_percent?: number
  commission_amount?: Money
  status: string
  confirmed_at?: string
  confirm_at?: string
  available_at?: string
  created_at: string
  updated_at: string
  affiliate_code?: string
  user_email?: string
  affiliate_profile?: { id: number; user_id: number; code: string; user_email?: string; user_display_name?: string; user?: { id: number; email: string; display_name?: string } }
  order?: { id: number; order_no: string; total_amount?: Money }
}

export interface AdminAffiliateWithdraw {
  id: number
  affiliate_profile_id: number
  amount: Money
  channel: string
  account_info: string
  account?: string
  status: string
  reject_reason?: string
  paid_at?: string
  processed_at?: string
  processor?: { username?: string; [key: string]: unknown } | string
  created_at: string
  updated_at: string
  affiliate_code?: string
  user_email?: string
  affiliate_profile?: { id: number; user_id: number; code: string; user_email?: string; user_display_name?: string; user?: { id: number; email: string; display_name?: string } }
}

export interface AdminResellerUser {
  id: number
  email?: string
  display_name?: string
  username?: string
}

export interface AdminResellerProfileRef {
  id: number
  user_id: number
  status?: string
  settlement_status?: string
  user?: AdminResellerUser
}

export interface AdminResellerProfile {
  id: number
  user_id: number
  status: string
  apply_reason?: string
  reject_reason?: string
  default_markup_percent: string
  max_markup_percent: string
  settlement_status: string
  reviewed_by?: number
  reviewed_at?: string
  created_at: string
  updated_at: string
  user?: AdminResellerUser
}

export interface AdminResellerDomain {
  id: number
  reseller_id: number
  domain: string
  type: string
  verification_token?: string
  verification_status: string
  status: string
  is_primary: boolean
  verified_at?: string
  created_at: string
  updated_at: string
  profile?: AdminResellerProfileRef
}

export interface AdminResellerLocalizedText {
  'zh-CN'?: string
  'zh-TW'?: string
  'en-US'?: string
  [key: string]: string | undefined
}

export interface AdminResellerSiteConfigPayload {
  site_name?: string
  logo?: string
  favicon?: string
  announcement?: {
    enabled?: boolean
    type?: string
    title?: AdminResellerLocalizedText
    content?: AdminResellerLocalizedText
  }
  support?: {
    telegram?: string
    whatsapp?: string
    email?: string
    support_url?: string
  }
  seo?: {
    title?: AdminResellerLocalizedText
    keywords?: AdminResellerLocalizedText
    description?: AdminResellerLocalizedText
    default_og_image?: string
  }
  footer_links?: Array<{
    name?: AdminResellerLocalizedText
    url?: string
  }>
  nav_config?: {
    builtin?: Record<string, boolean>
    custom_items?: Array<{
      name?: AdminResellerLocalizedText
      url?: string
      enabled?: boolean
    }>
  }
}

export interface AdminResellerSiteConfig extends Required<Pick<AdminResellerSiteConfigPayload, 'announcement' | 'support' | 'seo' | 'nav_config'>> {
  id: number
  reseller_id: number
  site_name: string
  logo: string
  favicon: string
  footer_links: Array<{
    name?: AdminResellerLocalizedText
    url?: string
  }>
  profile?: AdminResellerProfileRef
  created_at: string
  updated_at: string
}

export interface AdminResellerProductSettingProduct {
  id: number
  slug: string
  title: LocalizedText
  price_amount: string | number
  is_active: boolean
}

export interface AdminResellerProductSetting {
  id: number
  reseller_id: number
  product_id: number
  sku_id: number
  is_listed: boolean
  pricing_mode: string
  markup_percent: string | number
  fixed_markup_amount: string | number
  fixed_price_amount: string | number
  sort_order: number
  created_at: string
  updated_at: string
  profile?: AdminResellerProfileRef
  product?: AdminResellerProductSettingProduct
}

export interface AdminResellerProductSettingRule {
  id?: number
  product_id: number
  sku_id: number
  is_listed: boolean
  pricing_mode: string
  markup_percent: string | number
  fixed_markup_amount: string | number
  fixed_price_amount: string | number
  effective_price_amount?: string | number
  rule_source?: string
  sort_order: number
  updated_at?: string
}

export interface AdminResellerProductSettingSKU {
  id: number
  sku_code: string
  spec_values: Record<string, string>
  base_price_amount: string | number
  is_active: boolean
  setting?: AdminResellerProductSettingRule
  effective_price_amount?: string | number
}

export interface AdminResellerProductSettingDetail {
  product: AdminResellerProductSettingProduct
  product_setting?: AdminResellerProductSettingRule
  skus: AdminResellerProductSettingSKU[]
}

export interface AdminResellerProductSettingPayloadItem {
  sku_id: number
  is_listed: boolean
  pricing_mode: string
  markup_percent: string
  fixed_markup_amount: string
  fixed_price_amount: string
  sort_order: number
}

export interface AdminResellerProductSettingUpdatePayload {
  settings: AdminResellerProductSettingPayloadItem[]
}

export interface AdminResellerProductSettingPreviewItem {
  sku_id: number
  is_listed: boolean
  base_price_amount: string
  effective_price_amount: string
  valid: boolean
  error_code?: string
}

export interface AdminResellerProductSettingPreviewData {
  items: AdminResellerProductSettingPreviewItem[]
}

export interface AdminResellerProfileApprovePayload {
  default_markup_percent?: string
  max_markup_percent?: string
}

export interface AdminResellerProfileUpdatePayload {
  default_markup_percent?: string
  max_markup_percent?: string
  settlement_status?: string
  reason?: string
}

export interface AdminResellerSystemDomainPayload {
  subdomain?: string
  domain?: string
}

export interface AdminResellerReasonPayload {
  reason?: string
}

export interface AdminResellerProfileDetailOrder {
  order_no: string
  status: string
  currency: string
  total_amount: string | number
  base_amount: string | number
  profit_amount: string | number
  profit_status: string
  domain: string
  buyer_label: string
  items_count: number
  created_at: string
  paid_at?: string
}

export interface AdminResellerProfileDetailBalance {
  id: number
  currency: string
  status: string
  available_amount: string | number
  locked_amount: string | number
  negative_amount: string | number
  updated_at: string
}

export interface AdminResellerProfileDetailLedger {
  id: number
  order_id?: number
  type: string
  amount: string | number
  currency: string
  status: string
  available_at?: string
  withdraw_request_id?: number
  created_at: string
}

export interface AdminResellerProfileDetailWithdraw {
  id: number
  amount: string | number
  currency: string
  channel: string
  account: string
  status: string
  reject_reason?: string
  processed_at?: string
  created_at: string
}

export interface AdminResellerProfileDetail {
  profile: AdminResellerProfile
  domains: AdminResellerDomain[]
  site_config?: AdminResellerSiteConfig
  product_summary: {
    configured_products: number
    hidden_products: number
    sku_overrides: number
    pricing_overrides: number
  }
  finance_summary: {
    balances: AdminResellerProfileDetailBalance[]
    recent_ledger_count: number
    recent_withdraw_count: number
  }
  recent_orders: AdminResellerProfileDetailOrder[]
  recent_ledger_entries: AdminResellerProfileDetailLedger[]
  recent_withdraws: AdminResellerProfileDetailWithdraw[]
}

export interface AdminResellerOrderRef {
  id: number
  order_no: string
}

export interface AdminResellerLedgerEntry {
  id: number
  reseller_id: number
  order_id?: number
  type: string
  amount: Money | string
  currency: string
  idempotency_key: string
  metadata_json?: Record<string, unknown>
  status: string
  available_at?: string
  withdraw_request_id?: number
  remark?: string
  created_at: string
  updated_at: string
  profile?: AdminResellerProfileRef
  order?: AdminResellerOrderRef
}

export interface AdminResellerBalanceAccount {
  id: number
  reseller_id: number
  currency: string
  status: string
  available_amount_cache: number | string
  locked_amount_cache: number | string
  negative_amount_cache: number | string
  last_ledger_entry_id: number
  risk_note?: string
  created_at: string
  updated_at: string
  profile?: AdminResellerProfileRef
}

export interface AdminResellerWithdraw {
  id: number
  reseller_id: number
  amount: Money | string
  currency: string
  channel: string
  account: string
  status: string
  reject_reason?: string
  processed_by?: number
  processed_at?: string
  processor?: { id?: number; username?: string; [key: string]: unknown } | string
  created_at: string
  updated_at: string
  profile?: AdminResellerProfileRef
}

export interface AdminResellerOperationsAlert {
  type: string
  level: string
  value: Money
}

export interface AdminResellerOperationsOverview {
  range: string
  from: string
  to: string
  timezone: string
  lifecycle: {
    profiles_total: number
    profiles_pending_review: number
    profiles_active: number
    profiles_rejected: number
    profiles_disabled: number
    profiles_settlement_frozen: number
    domains_total: number
    domains_pending_review: number
    domains_active: number
    domains_disabled: number
    domains_pending_verification: number
    domains_verified: number
    custom_domains: number
    subdomains: number
    site_configs_total: number
    active_profiles_without_site_config: number
  }
  orders: {
    orders_total: number
    paid_orders: number
    completed_orders: number
    refunded_orders: number
    self_dealing_blocked_orders: number
    active_resellers_with_orders: number
    average_paid_orders_per_active_reseller: string
  }
  top_resellers: Array<{
    reseller_id: number
    user_id: number
    email?: string
    display_name?: string
    orders_total: number
    paid_orders: number
    active_domains: number
    site_configured: boolean
    last_order_at?: string
  }>
  alerts: AdminResellerOperationsAlert[]
}

export interface AdminResellerOperationsFinance {
  range: string
  from: string
  to: string
  timezone: string
  period_currency_rows: Array<{
    currency: string
    orders_total: number
    paid_orders: number
    gmv_paid: string
    profit_earned: string
    refund_deducted: string
    withdraw_paid: string
  }>
  current_currency_rows: Array<{
    currency: string
    available_balance: string
    locked_balance: string
    negative_balance: string
    pending_withdraw_count: number
    pending_withdraw_amount: string
    negative_balance_accounts: number
    frozen_balance_accounts: number
  }>
}

// ---- Admin auth / RBAC / wallet / misc (from original api/admin.ts) ----
export interface CaptchaPayload {
  captcha_id?: string
  captcha_code?: string
  turnstile_token?: string
}

export interface AdminLoginRequest {
  username: string
  password: string
  captcha_payload?: CaptchaPayload
}

export interface AdminLoginPasswordResponse {
  requires_totp: false
  token: string
  user: {
    id: number
    username: string
  }
  expires_at: string
}

export interface AdminLoginChallengeResponse {
  requires_totp: true
  challenge_token: string
  challenge_expires_at: string
}

export type AdminLoginResponse = AdminLoginPasswordResponse | AdminLoginChallengeResponse

export interface TwoFAStatus {
  enabled: boolean
  enabled_at?: string
  recovery_codes_remaining: number
  recovery_codes_total: number
}

export interface SetupTwoFAResponse {
  secret: string
  otpauth_url: string
  expires_at: string
}

export interface EnableTwoFAResponse {
  enabled_at: string
  recovery_codes: string[]
}

export interface Verify2FAPayload {
  challenge_token: string
  code?: string
  recovery_code?: string
}

export interface AdminAuthzPolicy {
  subject: string
  object: string
  action: string
}

export interface AdminAuthzRole {
  role: string
  immutable: boolean
}

export interface AdminAuthzMeResponse {
  admin_id: number
  is_super: boolean
  roles: string[]
  policies: AdminAuthzPolicy[]
}

export interface AdminAuthzAdmin {
  id: number
  username: string
  is_super: boolean
  last_login_at?: string
  created_at?: string
  roles?: string[]
  totp_enabled?: boolean
  totp_enabled_at?: string
}

export interface AuthzCreateAdminRequest {
  username: string
  password: string
  is_super?: boolean
}

export interface AuthzUpdateAdminRequest {
  username?: string
  password?: string
  is_super?: boolean
}

export interface AdminAuthzAuditLog {
  id: number
  operator_admin_id: number
  operator_username: string
  target_admin_id?: number
  target_username: string
  action: string
  role: string
  object: string
  method: string
  request_id: string
  detail: Record<string, unknown>
  created_at: string
}

export interface AdminPermissionCatalogItem {
  module: string
  method: string
  object: string
  permission: string
}

export interface AdminWalletAccount {
  id: number
  user_id: number
  balance: string
  created_at: string
  updated_at: string
}

export interface AdminWalletTransaction {
  id: number
  user_id: number
  order_id?: number | null
  type: string
  direction: string
  amount: string
  balance_before: string
  balance_after: string
  currency: string
  reference: string
  remark: string
  created_at: string
  updated_at: string
}

export interface AdminWalletRechargeUser {
  id: number
  email: string
  display_name: string
}

export interface AdminWalletRecharge {
  id: number
  recharge_no: string
  user_id: number
  payment_id: number
  channel_id: number
  provider_type: string
  channel_type: string
  interaction_mode: string
  amount: string
  payable_amount: string
  fee_rate: string
  fee_amount: string
  currency: string
  status: string
  remark?: string
  paid_at?: string
  created_at: string
  updated_at: string
  channel_name?: string
  payment_status?: string
  user?: AdminWalletRechargeUser
}

export interface AdminAdjustWalletPayload {
  amount: string
  operation?: 'add' | 'subtract'
  currency?: string
  remark: string
}

export interface AdminRefundToWalletPayload {
  amount: string
  remark?: string
}

export interface AdminManualRefundPayload {
  amount: string
  remark?: string
  payment_fee_refunded?: boolean
}

export interface AdminUpdateRefundPaymentFeePayload {
  payment_fee_refunded: boolean
}

export interface AdminBatchCardSecretStatusPayload {
  ids?: number[]
  batch_id?: number
  filter?: AdminCardSecretQueryPayload
  status: 'available' | 'reserved' | 'used'
}

export interface AdminBatchCardSecretDeletePayload {
  ids?: number[]
  batch_id?: number
  filter?: AdminCardSecretQueryPayload
}

export interface AdminExportCardSecretsPayload {
  ids?: number[]
  batch_id?: number
  filter?: AdminCardSecretQueryPayload
  format: 'txt' | 'csv'
}

export interface AdminExportAvailableCardSecretsPayload {
  product_id: number
  sku_id?: number
  batch_id?: number
  limit: number
  format: 'txt' | 'csv'
  delete_after_export?: boolean
}

export interface AdminCardSecretQueryPayload {
  product_id?: number
  sku_id?: number
  batch_id?: number
  status?: string
  secret?: string
  batch_no?: string
}

export type AdminGiftCardStatus = 'active' | 'redeemed' | 'disabled'

export interface AdminGenerateGiftCardsPayload {
  name: string
  quantity: number
  amount: string
  expires_at?: string
}

export interface AdminUpdateGiftCardPayload {
  name?: string
  status?: Exclude<AdminGiftCardStatus, 'redeemed'>
  expires_at?: string
}

export interface AdminBatchGiftCardStatusPayload {
  ids: number[]
  status: 'active' | 'disabled'
}

export interface AdminExportGiftCardsPayload {
  ids: number[]
  format: 'txt' | 'csv'
}

export interface AdminAffiliateSetting {
  enabled: boolean
  commission_rate: number
  confirm_days: number
  min_withdraw_amount: number
  withdraw_channels: string[]
}

export interface ComplianceStatus {
  acknowledged: boolean
  acknowledged_at?: string
  acknowledged_by_admin_id?: number
  acknowledged_by_username?: string
  version?: string
}

export interface ComplianceAcknowledgePayload {
  segment1: string
  segment2: string
  segment3: string
}

// ---- Dashboard (from original views/Dashboard.vue) ----
export interface DashboardAlertItem {
  type: string
  level: string
  value: number
}

export interface DashboardFunnel {
  orders_created: number
  payments_created: number
  payments_success: number
  orders_paid: number
  orders_completed: number
  payment_conversion_rate: string
  completion_rate: string
}

export interface DashboardKpi {
  orders_total: number
  paid_orders: number
  completed_orders: number
  pending_payment_orders: number
  processing_orders: number
  gmv_paid: string
  total_cost: string
  payment_fee: string
  total_profit: string
  profit_margin: string
  payments_total: number
  payments_success: number
  payments_failed: number
  payment_success_rate: string
  new_users: number
  active_products: number
  out_of_stock_products: number
  low_stock_products: number
  out_of_stock_skus: number
  low_stock_skus: number
  auto_available_secrets: number
  manual_available_units: number
  total_user_balance: string
}

export interface DashboardOverview {
  range: string
  from: string
  to: string
  timezone: string
  currency?: string
  kpi: DashboardKpi
  funnel: DashboardFunnel
  alerts: DashboardAlertItem[]
}

export interface DashboardTrendPoint {
  date: string
  orders_total: number
  orders_paid: number
  payments_success: number
  payments_failed: number
  gmv_paid: string
  profit: string
}

export interface DashboardTrends {
  range: string
  from: string
  to: string
  timezone: string
  points: DashboardTrendPoint[]
}

export interface DashboardProductRanking {
  product_id: number
  sku_id?: number
  sku_code?: string
  sku_spec_values?: Record<string, unknown>
  title: string
  paid_orders: number
  quantity: number
  paid_amount: string
  total_cost: string
  profit: string
}

export interface DashboardChannelRanking {
  channel_id: number
  channel_name: string
  provider_type: string
  channel_type: string
  success_count: number
  failed_count: number
  success_amount: string
  success_rate: string
}

export interface DashboardRankings {
  range: string
  from: string
  to: string
  timezone: string
  top_products: DashboardProductRanking[]
  top_channels: DashboardChannelRanking[]
}

// ---- Public config (subset used by admin) ----
export interface ThemeEffects {
  sakura?: boolean
  sparkle?: boolean
}

/** Zebra Store addition: site_config.theme (see docs/DESIGN.md) */
export interface SiteTheme {
  primary_color?: string
  secondary_color?: string
  accent_color?: string
  background_image?: string
  mascot_image?: string
  login_background?: string
  effects?: ThemeEffects
  default_mode?: 'system' | 'light' | 'dark'
}

export interface PublicConfig {
  app_version?: string
  currency?: string
  languages?: string[]
  brand?: {
    site_name?: string
    site_url?: string
    site_icon?: string
    site_logo?: string
    site_description?: LocalizedText
  }
  captcha?: {
    provider?: string
    scenes?: Record<string, boolean>
    turnstile?: { site_key?: string }
  }
  theme?: SiteTheme
  [key: string]: unknown
}

export interface ImageCaptchaResponse {
  captcha_id: string
  image_base64: string
}

export interface MediaListResponse {
  items: AdminMedia[]
  total: number
}

export interface UploadResponse {
  url: string
  filename?: string
  size?: number
  [key: string]: unknown
}

export interface BatchResult {
  success_count?: number
  [key: string]: unknown
}

/** Generic JSON object used for settings payloads whose shape lives in the view composable. */
export type JsonObject = Record<string, unknown>
