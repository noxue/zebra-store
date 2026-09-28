// Admin API endpoint functions shared by the management views.
// Every function resolves to the response envelope `ApiResponse<T>`; read `.data` / `.pagination`.
import { api, type QueryParams } from './client'
import type {
  AdminAdjustWalletPayload,
  AdminAffiliateCommission,
  AdminAffiliateSetting,
  AdminAffiliateUser,
  AdminAffiliateWithdraw,
  AdminApiCredential,
  AdminAuthzAdmin,
  AdminAuthzAuditLog,
  AdminAuthzMeResponse,
  AdminAuthzPolicy,
  AdminAuthzRole,
  AdminBanner,
  AdminBatchCardSecretDeletePayload,
  AdminBatchCardSecretStatusPayload,
  AdminBatchGiftCardStatusPayload,
  AdminCardSecret,
  AdminCardSecretBatch,
  AdminCategory,
  AdminCoupon,
  AdminDashboardInventoryAlert,
  AdminExportAvailableCardSecretsPayload,
  AdminExportCardSecretsPayload,
  AdminExportGiftCardsPayload,
  AdminFulfillment,
  AdminGatewaySecurityTestResult,
  AdminGenerateGiftCardsPayload,
  AdminGiftCard,
  AdminLoginRequest,
  AdminLoginResponse,
  AdminManualRefundPayload,
  AdminMemberLevel,
  AdminMemberLevelPrice,
  AdminNotificationLog,
  AdminOrder,
  AdminOrderRefund,
  AdminPayment,
  AdminPaymentChannel,
  AdminPermissionCatalogItem,
  AdminPost,
  AdminProcurementOrder,
  AdminProduct,
  AdminProductMapping,
  AdminPromotion,
  AdminReconciliationItem,
  AdminReconciliationJob,
  AdminRefundToWalletPayload,
  AdminResellerBalanceAccount,
  AdminResellerDomain,
  AdminResellerLedgerEntry,
  AdminResellerOperationsFinance,
  AdminResellerOperationsOverview,
  AdminResellerProductSetting,
  AdminResellerProductSettingDetail,
  AdminResellerProductSettingPreviewData,
  AdminResellerProductSettingUpdatePayload,
  AdminResellerProfile,
  AdminResellerProfileApprovePayload,
  AdminResellerProfileDetail,
  AdminResellerProfileUpdatePayload,
  AdminResellerReasonPayload,
  AdminResellerSiteConfig,
  AdminResellerSiteConfigPayload,
  AdminResellerSystemDomainPayload,
  AdminResellerWithdraw,
  AdminSiteConnection,
  SiteConnectionCodeParseResult,
  SiteConnectionHandshakeRequest,
  SiteConnectionHandshakeResult,
  SiteConnectionProtocolDef,
  AdminTelegramBotRuntimeStatus,
  AdminTelegramBroadcast,
  AdminTelegramBroadcastUser,
  AdminUpdateGiftCardPayload,
  AdminUpdateRefundPaymentFeePayload,
  AdminUser,
  AdminUserLoginLog,
  AdminWalletAccount,
  AdminWalletRecharge,
  AdminWalletTransaction,
  AdRenderResponse,
  BatchResult,
  ComplianceAcknowledgePayload,
  ComplianceStatus,
  DashboardOverview,
  DashboardRankings,
  DashboardTrends,
  EnableTwoFAResponse,
  ImageCaptchaResponse,
  JsonObject,
  AdminLoginPasswordResponse,
  MediaListResponse,
  PublicConfig,
  SetupTwoFAResponse,
  TwoFAStatus,
  UploadResponse,
  AuthzCreateAdminRequest,
  AuthzUpdateAdminRequest,
  Verify2FAPayload,
  AdminMedia,
} from './types'

export type * from './types'

type Id = number | string

export const adminAPI = {
  // ---- auth / 2FA / compliance ----
  login: (data: AdminLoginRequest) => api.post<AdminLoginResponse>('/admin/login', data),
  verify2FA: (data: Verify2FAPayload) => api.post<AdminLoginPasswordResponse>('/admin/login/verify-2fa', data),
  getComplianceStatus: () => api.get<ComplianceStatus>('/admin/compliance/status'),
  acknowledgeCompliance: (data: ComplianceAcknowledgePayload) => api.post<ComplianceStatus>('/admin/compliance/acknowledge', data),
  get2FAStatus: () => api.get<TwoFAStatus>('/admin/2fa/status'),
  setup2FA: () => api.post<SetupTwoFAResponse>('/admin/2fa/setup', {}),
  enable2FA: (data: { code: string }) => api.post<EnableTwoFAResponse>('/admin/2fa/enable', data),
  disable2FA: (data: { code?: string; recovery_code?: string }) => api.post<JsonObject>('/admin/2fa/disable', data),
  regenerateRecoveryCodes: (data: { code: string }) =>
    api.post<{ recovery_codes: string[] }>('/admin/2fa/recovery-codes/regenerate', data),
  resetAdmin2FA: (id: Id) => api.post<JsonObject>(`/admin/authz/admins/${id}/2fa/reset`, {}),
  updatePassword: (data: { old_password: string; new_password: string }) => api.put<JsonObject>('/admin/password', data),

  // ---- RBAC ----
  getAuthzMe: () => api.get<AdminAuthzMeResponse>('/admin/authz/me'),
  listAuthzRoles: () => api.get<AdminAuthzRole[] | string[]>('/admin/authz/roles', { params: { include_metadata: true } }),
  listAuthzAdmins: () => api.get<AdminAuthzAdmin[]>('/admin/authz/admins'),
  createAuthzAdmin: (data: AuthzCreateAdminRequest) => api.post<AdminAuthzAdmin>('/admin/authz/admins', data),
  updateAuthzAdmin: (id: Id, data: AuthzUpdateAdminRequest) => api.put<AdminAuthzAdmin>(`/admin/authz/admins/${id}`, data),
  deleteAuthzAdmin: (id: Id) => api.delete<JsonObject>(`/admin/authz/admins/${id}`),
  listAuthzAuditLogs: (params?: QueryParams) => api.get<AdminAuthzAuditLog[]>('/admin/authz/audit-logs', { params }),
  listAuthzPermissionCatalog: () => api.get<AdminPermissionCatalogItem[]>('/admin/authz/permissions/catalog'),
  createAuthzRole: (data: { role: string }) => api.post<JsonObject>('/admin/authz/roles', data),
  deleteAuthzRole: (role: string) => api.delete<JsonObject>(`/admin/authz/roles/${encodeURIComponent(role)}`),
  getAuthzRolePolicies: (role: string) => api.get<AdminAuthzPolicy[]>(`/admin/authz/roles/${encodeURIComponent(role)}/policies`),
  grantAuthzPolicy: (data: { role: string; object: string; action: string }) => api.post<JsonObject>('/admin/authz/policies', data),
  revokeAuthzPolicy: (data: { role: string; object: string; action: string }) =>
    api.delete<JsonObject>('/admin/authz/policies', { data }),
  getAuthzAdminRoles: (id: Id) => api.get<string[]>(`/admin/authz/admins/${id}/roles`),
  setAuthzAdminRoles: (id: Id, data: { roles: string[] }) => api.put<JsonObject>(`/admin/authz/admins/${id}/roles`, data),

  // ---- upload / media ----
  upload: (file: File | Blob, scene = 'common', filename?: string, opts?: { silent?: boolean }) => {
    const payload = new FormData()
    if (filename) payload.append('file', file, filename)
    else payload.append('file', file)
    payload.append('scene', scene)
    return api.post<UploadResponse>('/admin/upload', payload, opts)
  },
  getMedia: (params?: QueryParams) => api.get<MediaListResponse>('/admin/media', { params }),
  updateMedia: (id: Id, data: { name: string }) => api.put<AdminMedia>(`/admin/media/${id}`, data),
  deleteMedia: (id: Id) => api.delete<JsonObject>(`/admin/media/${id}`),
  batchDeleteMedia: (ids: number[]) => api.post<BatchResult>('/admin/media/batch-delete', { ids }),

  // ---- catalog ----
  getProducts: (params?: QueryParams) => api.get<AdminProduct[]>('/admin/products', { params }),
  getProduct: (id: Id) => api.get<AdminProduct>(`/admin/products/${id}`),
  createProduct: (data: JsonObject) => api.post<AdminProduct>('/admin/products', data),
  updateProduct: (id: Id, data: JsonObject) => api.put<AdminProduct>(`/admin/products/${id}`, data),
  patchProduct: (id: Id, data: { is_active?: boolean; sort_order?: number; category_id?: number }) =>
    api.patch<AdminProduct>(`/admin/products/${id}`, data),
  updateProductWholesalePrices: (id: Id, data: { wholesale_prices: Array<{ sku_id?: number; sku_code?: string; min_quantity: number; unit_price: number | string }> }) =>
    api.patch<AdminProduct>(`/admin/products/${id}/wholesale-prices`, data),
  deleteProduct: (id: Id) => api.delete<JsonObject>(`/admin/products/${id}`),
  batchUpdateProductStatus: (ids: number[], isActive: boolean) =>
    api.post<BatchResult>('/admin/products/batch-status', { ids, is_active: isActive }),
  batchUpdateProductCategory: (ids: number[], categoryId: number) =>
    api.post<BatchResult>('/admin/products/batch-category', { ids, category_id: categoryId }),
  batchDeleteProducts: (ids: number[]) => api.post<BatchResult>('/admin/products/batch-delete', { ids }),
  getCategories: (params?: QueryParams) => api.get<AdminCategory[]>('/admin/categories', { params }),
  createCategory: (data: JsonObject) => api.post<AdminCategory>('/admin/categories', data),
  updateCategory: (id: Id, data: JsonObject) => api.put<AdminCategory>(`/admin/categories/${id}`, data),
  patchCategoryActive: (id: Id, isActive: boolean) => api.patch<AdminCategory>(`/admin/categories/${id}/active`, { is_active: isActive }),
  deleteCategory: (id: Id) => api.delete<JsonObject>(`/admin/categories/${id}`),
  getPosts: (params?: QueryParams) => api.get<AdminPost[]>('/admin/posts', { params }),
  getPost: (id: Id) => api.get<AdminPost>(`/admin/posts/${id}`),
  createPost: (data: JsonObject) => api.post<AdminPost>('/admin/posts', data),
  updatePost: (id: Id, data: JsonObject) => api.put<AdminPost>(`/admin/posts/${id}`, data),
  deletePost: (id: Id) => api.delete<JsonObject>(`/admin/posts/${id}`),
  getPostRelatedProducts: (id: Id) => api.get<AdminProduct[]>(`/admin/posts/${id}/products`),
  getPostCategories: (params?: QueryParams) => api.get<AdminCategory[]>('/admin/post-categories', { params }),
  createPostCategory: (data: JsonObject) => api.post<AdminCategory>('/admin/post-categories', data),
  updatePostCategory: (id: Id, data: JsonObject) => api.put<AdminCategory>(`/admin/post-categories/${id}`, data),
  deletePostCategory: (id: Id) => api.delete<JsonObject>(`/admin/post-categories/${id}`),
  patchPostCategoryStatus: (id: Id, isActive: boolean) =>
    api.patch<AdminCategory>(`/admin/post-categories/${id}/status`, { is_active: isActive }),
  getBanners: (params?: QueryParams) => api.get<AdminBanner[]>('/admin/banners', { params }),
  getBanner: (id: Id) => api.get<AdminBanner>(`/admin/banners/${id}`),
  createBanner: (data: JsonObject) => api.post<AdminBanner>('/admin/banners', data),
  updateBanner: (id: Id, data: JsonObject) => api.put<AdminBanner>(`/admin/banners/${id}`, data),
  deleteBanner: (id: Id) => api.delete<JsonObject>(`/admin/banners/${id}`),

  // ---- settings ----
  getSettings: <T = JsonObject>(params: { key: string }) => api.get<T>('/admin/settings', { params }),
  updateSettings: <T = JsonObject>(data: { key: string; value: unknown }) => api.put<T>('/admin/settings', data),
  getHomeAnnouncement: () => api.get<JsonObject>('/admin/settings', { params: { key: 'home_announcement' } }),
  updateHomeAnnouncement: (value: JsonObject) => api.put<JsonObject>('/admin/settings', { key: 'home_announcement', value }),
  getSMTPSettings: () => api.get<JsonObject>('/admin/settings/smtp'),
  updateSMTPSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/smtp', data),
  testSMTPSettings: (data: JsonObject) => api.post<JsonObject>('/admin/settings/smtp/test', data),
  getCaptchaSettings: () => api.get<JsonObject>('/admin/settings/captcha'),
  updateCaptchaSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/captcha', data),
  getTelegramAuthSettings: () => api.get<JsonObject>('/admin/settings/telegram-auth'),
  updateTelegramAuthSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/telegram-auth', data),
  getGoogleAuthSettings: () => api.get<JsonObject>('/admin/settings/google-auth'),
  updateGoogleAuthSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/google-auth', data),
  getOrderEmailTemplateSettings: () => api.get<JsonObject>('/admin/settings/order-email-template'),
  updateOrderEmailTemplateSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/order-email-template', data),
  resetOrderEmailTemplateSettings: () => api.post<JsonObject>('/admin/settings/order-email-template/reset'),
  getNotificationCenterSettings: () => api.get<JsonObject>('/admin/settings/notification-center'),
  updateNotificationCenterSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/notification-center', data),
  listNotificationLogs: (params?: QueryParams) => api.get<AdminNotificationLog[]>('/admin/settings/notification-center/logs', { params }),
  testNotificationCenterSettings: (data: JsonObject) => api.post<JsonObject>('/admin/settings/notification-center/test', data),
  getAffiliateSettings: () => api.get<AdminAffiliateSetting>('/admin/settings/affiliate'),
  updateAffiliateSettings: (data: AdminAffiliateSetting) => api.put<AdminAffiliateSetting>('/admin/settings/affiliate', data),

  // ---- public / system ----
  getPublicConfig: () => api.get<PublicConfig>('/public/config'),
  getImageCaptcha: () => api.get<ImageCaptchaResponse>('/public/captcha/image'),
  checkSystemUpdate: (params?: { owner?: string; repo?: string }) => api.get<JsonObject>('/admin/system/version/check', { params }),
  getUpdateCapability: () => api.get<JsonObject>('/admin/system/update/capability'),
  getUpdateStatus: () => api.get<JsonObject>('/admin/system/update/status'),
  startSystemUpdate: () => api.post<JsonObject>('/admin/system/update/start'),
  rollbackSystemUpdate: (data?: { force?: boolean }) => api.post<JsonObject>('/admin/system/update/rollback', data ?? {}),
  restartSystemService: () => api.post<JsonObject>('/admin/system/restart'),

  // ---- dashboard ----
  getDashboardOverview: (params?: QueryParams) => api.get<DashboardOverview>('/admin/dashboard/overview', { params }),
  getDashboardTrends: (params?: QueryParams) => api.get<DashboardTrends>('/admin/dashboard/trends', { params }),
  getDashboardRankings: (params?: QueryParams) => api.get<DashboardRankings>('/admin/dashboard/rankings', { params }),
  getDashboardInventoryAlerts: () => api.get<AdminDashboardInventoryAlert[]>('/admin/dashboard/inventory-alerts'),

  // ---- orders / payments ----
  getOrders: (params?: QueryParams) => api.get<AdminOrder[]>('/admin/orders', { params }),
  getOrder: (id: Id) => api.get<AdminOrder>(`/admin/orders/${id}`),
  updateOrderStatus: (id: Id, data: { status: string }) => api.patch<AdminOrder>(`/admin/orders/${id}`, data),
  createFulfillment: (data: JsonObject) => api.post<AdminFulfillment>('/admin/fulfillments', data),
  downloadFulfillment: (orderId: Id) => api.getBlob(`/admin/orders/${orderId}/fulfillment/download`),
  refundOrderToWallet: (id: Id, data: AdminRefundToWalletPayload) => api.post<JsonObject>(`/admin/orders/${id}/refund-to-wallet`, data),
  manualRefundOrder: (id: Id, data: AdminManualRefundPayload) => api.post<JsonObject>(`/admin/orders/${id}/manual-refund`, data),
  getOrderRefunds: (params?: QueryParams) => api.get<AdminOrderRefund[]>('/admin/order-refunds', { params }),
  getOrderRefund: (id: Id) => api.get<AdminOrderRefund>(`/admin/order-refunds/${id}`),
  updateOrderRefundPaymentFee: (id: Id, data: AdminUpdateRefundPaymentFeePayload) =>
    api.patch<AdminOrderRefund>(`/admin/order-refunds/${id}/payment-fee`, data),
  getPayments: (params?: QueryParams) => api.get<AdminPayment[]>('/admin/payments', { params }),
  getPayment: (id: Id) => api.get<AdminPayment>(`/admin/payments/${id}`),
  exportPayments: (params?: QueryParams) => api.getBlob('/admin/payments/export', { params }),
  getPaymentChannels: (params?: QueryParams) => api.get<AdminPaymentChannel[]>('/admin/payment-channels', { params }),
  getPaymentChannel: (id: Id) => api.get<AdminPaymentChannel>(`/admin/payment-channels/${id}`),
  createPaymentChannel: (data: JsonObject) => api.post<AdminPaymentChannel>('/admin/payment-channels', data),
  updatePaymentChannel: (id: Id, data: JsonObject) => api.put<AdminPaymentChannel>(`/admin/payment-channels/${id}`, data),
  deletePaymentChannel: (id: Id) => api.delete<JsonObject>(`/admin/payment-channels/${id}`),
  testWechatPayPublicKey: (id: Id) =>
    api.post<AdminGatewaySecurityTestResult>(`/admin/payment-channels/${id}/wechatpay-public-key-test`, {}),

  // ---- users / wallet ----
  getUsers: (params?: QueryParams) => api.get<AdminUser[]>('/admin/users', { params }),
  getUser: (id: Id) => api.get<AdminUser>(`/admin/users/${id}`),
  updateUser: (id: Id, data: JsonObject) => api.put<AdminUser>(`/admin/users/${id}`, data),
  batchUpdateUserStatus: (data: { user_ids: number[]; status: string }) => api.put<BatchResult>('/admin/users/batch-status', data),
  getUserLoginLogs: (params?: QueryParams) => api.get<AdminUserLoginLog[]>('/admin/user-login-logs', { params }),
  getUserWallet: (id: Id) => api.get<{ account?: AdminWalletAccount; [key: string]: unknown }>(`/admin/users/${id}/wallet`),
  getUserWalletTransactions: (id: Id, params?: QueryParams) =>
    api.get<AdminWalletTransaction[]>(`/admin/users/${id}/wallet/transactions`, { params }),
  adjustUserWallet: (id: Id, data: AdminAdjustWalletPayload) => api.post<JsonObject>(`/admin/users/${id}/wallet/adjust`, data),
  unbindUserTelegram: (id: Id) => api.delete<JsonObject>(`/admin/users/${id}/oauth/telegram`),
  unbindUserGoogle: (id: Id) => api.delete<JsonObject>(`/admin/users/${id}/oauth/google`),
  resetUser2FA: (id: Id) => api.delete<JsonObject>(`/admin/users/${id}/2fa`),
  getUserCouponUsages: (id: Id, params?: QueryParams) => api.get<JsonObject[]>(`/admin/users/${id}/coupon-usages`, { params }),
  setUserMemberLevel: (userId: Id, memberLevelId: number) =>
    api.put<JsonObject>(`/admin/users/${userId}/member-level`, { member_level_id: memberLevelId }),
  getWalletRecharges: (params?: QueryParams) => api.get<AdminWalletRecharge[]>('/admin/wallet/recharges', { params }),

  // ---- affiliate ----
  getAffiliateUsers: (params?: QueryParams) => api.get<AdminAffiliateUser[]>('/admin/affiliates/users', { params }),
  updateAffiliateUserStatus: (id: Id, data: { status: string }) => api.patch<JsonObject>(`/admin/affiliates/users/${id}/status`, data),
  batchUpdateAffiliateUserStatus: (data: { profile_ids: number[]; status: string }) =>
    api.patch<BatchResult>('/admin/affiliates/users/batch-status', data),
  getAffiliateCommissions: (params?: QueryParams) => api.get<AdminAffiliateCommission[]>('/admin/affiliates/commissions', { params }),
  getAffiliateWithdraws: (params?: QueryParams) => api.get<AdminAffiliateWithdraw[]>('/admin/affiliates/withdraws', { params }),
  rejectAffiliateWithdraw: (id: Id, data: { reason?: string }) => api.post<JsonObject>(`/admin/affiliates/withdraws/${id}/reject`, data),
  payAffiliateWithdraw: (id: Id) => api.post<JsonObject>(`/admin/affiliates/withdraws/${id}/pay`, {}),

  // ---- resellers ----
  getResellerOperationsOverview: (params?: QueryParams) =>
    api.get<AdminResellerOperationsOverview>('/admin/resellers/operations/overview', { params }),
  getResellerOperationsFinance: (params?: QueryParams) =>
    api.get<AdminResellerOperationsFinance>('/admin/resellers/operations/finance', { params }),
  getResellerLedgerEntries: (params?: QueryParams) => api.get<AdminResellerLedgerEntry[]>('/admin/resellers/ledger-entries', { params }),
  getResellerBalanceAccounts: (params?: QueryParams) =>
    api.get<AdminResellerBalanceAccount[]>('/admin/resellers/balance-accounts', { params }),
  getResellerWithdraws: (params?: QueryParams) => api.get<AdminResellerWithdraw[]>('/admin/resellers/withdraws', { params }),
  rejectResellerWithdraw: (id: Id, data: { reason?: string }) => api.post<JsonObject>(`/admin/resellers/withdraws/${id}/reject`, data),
  payResellerWithdraw: (id: Id) => api.post<JsonObject>(`/admin/resellers/withdraws/${id}/pay`, {}),
  getResellerProfiles: (params?: QueryParams) => api.get<AdminResellerProfile[]>('/admin/resellers/profiles', { params }),
  getResellerProfile: (id: Id) => api.get<AdminResellerProfileDetail>(`/admin/resellers/profiles/${id}`),
  updateResellerProfile: (id: Id, data: AdminResellerProfileUpdatePayload) =>
    api.put<AdminResellerProfile>(`/admin/resellers/profiles/${id}`, data),
  assignResellerSystemDomain: (id: Id, data: AdminResellerSystemDomainPayload) =>
    api.put<AdminResellerDomain>(`/admin/resellers/profiles/${id}/system-domain`, data),
  approveResellerProfile: (id: Id, data: AdminResellerProfileApprovePayload) =>
    api.post<AdminResellerProfile>(`/admin/resellers/profiles/${id}/approve`, data),
  rejectResellerProfile: (id: Id, data: AdminResellerReasonPayload) =>
    api.post<AdminResellerProfile>(`/admin/resellers/profiles/${id}/reject`, data),
  disableResellerProfile: (id: Id, data: AdminResellerReasonPayload) =>
    api.post<AdminResellerProfile>(`/admin/resellers/profiles/${id}/disable`, data),
  restoreResellerProfile: (id: Id) => api.post<AdminResellerProfile>(`/admin/resellers/profiles/${id}/restore`, {}),
  getResellerDomains: (params?: QueryParams) => api.get<AdminResellerDomain[]>('/admin/resellers/domains', { params }),
  approveResellerDomain: (id: Id) => api.post<AdminResellerDomain>(`/admin/resellers/domains/${id}/approve`, {}),
  disableResellerDomain: (id: Id) => api.post<AdminResellerDomain>(`/admin/resellers/domains/${id}/disable`, {}),
  setPrimaryResellerDomain: (id: Id) => api.post<AdminResellerDomain>(`/admin/resellers/domains/${id}/set-primary`, {}),
  getResellerSiteConfigs: (params?: QueryParams) => api.get<AdminResellerSiteConfig[]>('/admin/resellers/site-configs', { params }),
  getResellerSiteConfig: (resellerId: Id) => api.get<AdminResellerSiteConfig>(`/admin/resellers/site-configs/${resellerId}`),
  updateResellerSiteConfig: (resellerId: Id, data: AdminResellerSiteConfigPayload) =>
    api.put<AdminResellerSiteConfig>(`/admin/resellers/site-configs/${resellerId}`, data),
  resetResellerSiteConfig: (resellerId: Id) => api.post<JsonObject>(`/admin/resellers/site-configs/${resellerId}/reset`, {}),
  getResellerProductSettings: (params?: QueryParams) =>
    api.get<AdminResellerProductSetting[]>('/admin/resellers/product-settings', { params }),
  getResellerProductSetting: (resellerId: Id, productId: Id) =>
    api.get<AdminResellerProductSettingDetail>(`/admin/resellers/product-settings/${resellerId}/${productId}`),
  updateResellerProductSettings: (resellerId: Id, productId: Id, data: AdminResellerProductSettingUpdatePayload) =>
    api.put<AdminResellerProductSettingDetail>(`/admin/resellers/product-settings/${resellerId}/${productId}`, data),
  previewResellerProductSettings: (resellerId: Id, productId: Id, data: AdminResellerProductSettingUpdatePayload) =>
    api.post<AdminResellerProductSettingPreviewData>(`/admin/resellers/product-settings/${resellerId}/${productId}/preview`, data),
  resetResellerProductSetting: (resellerId: Id, productId: Id, skuId = 0) =>
    api.delete<JsonObject>(`/admin/resellers/product-settings/${resellerId}/${productId}`, { params: { sku_id: skuId } }),

  // ---- marketing ----
  getCoupons: (params?: QueryParams) => api.get<AdminCoupon[]>('/admin/coupons', { params }),
  createCoupon: (data: JsonObject) => api.post<AdminCoupon>('/admin/coupons', data),
  updateCoupon: (id: Id, data: JsonObject) => api.put<AdminCoupon>(`/admin/coupons/${id}`, data),
  deleteCoupon: (id: Id) => api.delete<JsonObject>(`/admin/coupons/${id}`),
  getPromotions: (params?: QueryParams) => api.get<AdminPromotion[]>('/admin/promotions', { params }),
  createPromotion: (data: JsonObject) => api.post<AdminPromotion>('/admin/promotions', data),
  updatePromotion: (id: Id, data: JsonObject) => api.put<AdminPromotion>(`/admin/promotions/${id}`, data),
  deletePromotion: (id: Id) => api.delete<JsonObject>(`/admin/promotions/${id}`),
  generateGiftCards: (data: AdminGenerateGiftCardsPayload) => api.post<JsonObject>('/admin/gift-cards/generate', data),
  getGiftCards: (params?: QueryParams) => api.get<AdminGiftCard[]>('/admin/gift-cards', { params }),
  updateGiftCard: (id: Id, data: AdminUpdateGiftCardPayload) => api.put<AdminGiftCard>(`/admin/gift-cards/${id}`, data),
  deleteGiftCard: (id: Id) => api.delete<JsonObject>(`/admin/gift-cards/${id}`),
  batchUpdateGiftCardStatus: (data: AdminBatchGiftCardStatusPayload) => api.patch<BatchResult>('/admin/gift-cards/batch-status', data),
  exportGiftCards: (data: AdminExportGiftCardsPayload) => api.postBlob('/admin/gift-cards/export', data),
  getMemberLevels: (params?: QueryParams) => api.get<AdminMemberLevel[]>('/admin/member-levels', { params }),
  createMemberLevel: (data: JsonObject) => api.post<AdminMemberLevel>('/admin/member-levels', data),
  updateMemberLevel: (id: Id, data: JsonObject) => api.put<AdminMemberLevel>(`/admin/member-levels/${id}`, data),
  deleteMemberLevel: (id: Id) => api.delete<JsonObject>(`/admin/member-levels/${id}`),
  backfillMemberLevels: () => api.post<JsonObject>('/admin/member-levels/backfill'),
  getMemberLevelPrices: (productId: Id) => api.get<AdminMemberLevelPrice[]>('/admin/member-level-prices', { params: { product_id: productId } }),
  batchUpsertMemberLevelPrices: (data: { prices: Array<Partial<AdminMemberLevelPrice>> }) =>
    api.post<JsonObject>('/admin/member-level-prices/batch', data),
  deleteMemberLevelPrice: (id: Id) => api.delete<JsonObject>(`/admin/member-level-prices/${id}`),

  // ---- card secrets ----
  createCardSecretBatch: (data: {
    product_id: number
    sku_id?: number
    name?: string
    secrets: string[]
    batch_no?: string
    note?: string
    deduplicate?: boolean
  }) => api.post<JsonObject>('/admin/card-secrets/batch', data),
  importCardSecretCSV: (formData: FormData) => api.post<JsonObject>('/admin/card-secrets/import', formData),
  getCardSecrets: (params?: QueryParams) => api.get<AdminCardSecret[]>('/admin/card-secrets', { params }),
  updateCardSecret: (id: Id, data: { secret?: string; status?: string }) => api.put<AdminCardSecret>(`/admin/card-secrets/${id}`, data),
  batchUpdateCardSecretStatus: (data: AdminBatchCardSecretStatusPayload) => api.patch<BatchResult>('/admin/card-secrets/batch-status', data),
  batchDeleteCardSecrets: (data: AdminBatchCardSecretDeletePayload) => api.post<BatchResult>('/admin/card-secrets/batch-delete', data),
  exportCardSecrets: (data: AdminExportCardSecretsPayload) => api.postBlob('/admin/card-secrets/export', data),
  exportAvailableCardSecrets: (data: AdminExportAvailableCardSecretsPayload) => api.postBlob('/admin/card-secrets/export-available', data),
  getCardSecretStats: (params?: QueryParams) =>
    api.get<{ available: number; reserved: number; used: number; total: number }>('/admin/card-secrets/stats', { params }),
  getCardSecretBatches: (params?: QueryParams) => api.get<AdminCardSecretBatch[]>('/admin/card-secrets/batches', { params }),
  getCardSecretTemplate: () => api.get<JsonObject>('/admin/card-secrets/template'),

  // ---- integration ----
  getSiteConnections: (params?: QueryParams) => api.get<AdminSiteConnection[]>('/admin/site-connections', { params }),
  getSiteConnection: (id: Id) => api.get<AdminSiteConnection>(`/admin/site-connections/${id}`),
  createSiteConnection: (data: JsonObject) => api.post<AdminSiteConnection>('/admin/site-connections', data),
  updateSiteConnection: (id: Id, data: JsonObject) => api.put<AdminSiteConnection>(`/admin/site-connections/${id}`, data),
  deleteSiteConnection: (id: Id) => api.delete<JsonObject>(`/admin/site-connections/${id}`),
  pingSiteConnection: (id: Id) => api.post<JsonObject>(`/admin/site-connections/${id}/ping`),
  /** Registered supplier adapters (protocol id, localized name/description, dynamic fields, capabilities). */
  getSiteConnectionProtocols: () => api.get<SiteConnectionProtocolDef[]>('/admin/site-connections/protocols', { silent: true }),
  /** Decode a `zsc1_…` connection code (errors are handled by the caller: silent). */
  parseSiteConnectionCode: (code: string) =>
    api.post<SiteConnectionCodeParseResult>('/admin/site-connections/parse-code', { code }, { silent: true }),
  /** Probe an upstream with the given credentials without saving (silent; caller renders the error). */
  handshakeSiteConnection: (data: SiteConnectionHandshakeRequest) =>
    api.post<SiteConnectionHandshakeResult>('/admin/site-connections/handshake', data, { silent: true, timeout: 30_000 }),
  updateSiteConnectionStatus: (id: Id, data: { is_active?: boolean; status?: string }) =>
    api.put<JsonObject>(`/admin/site-connections/${id}/status`, data),
  reapplyConnectionMarkup: (id: Id) => api.post<JsonObject>(`/admin/site-connections/${id}/reapply-markup`),
  getProductMappings: (params?: QueryParams) => api.get<AdminProductMapping[]>('/admin/product-mappings', { params }),
  getProductMapping: (id: Id) => api.get<AdminProductMapping>(`/admin/product-mappings/${id}`),
  importUpstreamProduct: (data: JsonObject) => api.post<JsonObject>('/admin/product-mappings/import', data),
  batchImportUpstreamProducts: (data: JsonObject) => api.post<JsonObject>('/admin/product-mappings/batch-import', data),
  batchImportByCategory: (data: JsonObject) => api.post<JsonObject>('/admin/product-mappings/batch-import-by-category', data),
  syncProductMapping: (id: Id) => api.post<JsonObject>(`/admin/product-mappings/${id}/sync`),
  updateProductMappingStatus: (id: Id, data: { is_active: boolean }) => api.put<JsonObject>(`/admin/product-mappings/${id}/status`, data),
  deleteProductMapping: (id: Id) => api.delete<JsonObject>(`/admin/product-mappings/${id}`),
  batchSyncProductMappings: (ids: number[]) => api.post<BatchResult>('/admin/product-mappings/batch-sync', { ids }),
  batchUpdateProductMappingStatus: (ids: number[], isActive: boolean) =>
    api.post<BatchResult>('/admin/product-mappings/batch-status', { ids, is_active: isActive }),
  batchDeleteProductMappings: (ids: number[]) => api.post<BatchResult>('/admin/product-mappings/batch-delete', { ids }),
  getUpstreamProducts: (params?: QueryParams) => api.get<JsonObject>('/admin/upstream-products', { params }),
  getUpstreamCategories: (params: { connection_id: string | number }) => api.get<JsonObject>('/admin/upstream-categories', { params }),
  getProcurementOrders: (params?: QueryParams) => api.get<AdminProcurementOrder[]>('/admin/procurement-orders', { params }),
  getProcurementOrderStats: (params?: QueryParams) => api.get<Record<string, number>>('/admin/procurement-orders/stats', { params }),
  getProcurementOrder: (id: Id) => api.get<AdminProcurementOrder>(`/admin/procurement-orders/${id}`),
  downloadProcurementUpstreamPayload: (id: Id) => api.getBlob(`/admin/procurement-orders/${id}/upstream-payload/download`),
  retryProcurementOrder: (id: Id) => api.post<JsonObject>(`/admin/procurement-orders/${id}/retry`),
  cancelProcurementOrder: (id: Id) => api.post<JsonObject>(`/admin/procurement-orders/${id}/cancel`),
  runReconciliation: (data: { connection_id: number; type: string; time_range_start: string; time_range_end: string }) =>
    api.post<AdminReconciliationJob>('/admin/reconciliation/run', data),
  getReconciliationJobs: (params?: QueryParams) => api.get<AdminReconciliationJob[]>('/admin/reconciliation/jobs', { params }),
  getReconciliationJob: (id: Id, params?: QueryParams) =>
    api.get<{ job?: AdminReconciliationJob; items?: AdminReconciliationItem[]; [key: string]: unknown }>(`/admin/reconciliation/jobs/${id}`, {
      params,
    }),
  resolveReconciliationItem: (id: Id, data: { resolution?: string; remark?: string }) =>
    api.put<JsonObject>(`/admin/reconciliation/items/${id}/resolve`, data),
  getApiCredentials: (params?: QueryParams) => api.get<AdminApiCredential[]>('/admin/api-credentials', { params }),
  getApiCredential: (id: Id) => api.get<AdminApiCredential>(`/admin/api-credentials/${id}`),
  approveApiCredential: (id: Id) => api.post<JsonObject>(`/admin/api-credentials/${id}/approve`),
  rejectApiCredential: (id: Id, data: { reason: string }) => api.post<JsonObject>(`/admin/api-credentials/${id}/reject`, data),
  updateApiCredentialStatus: (id: Id, data: { is_active: boolean }) => api.put<JsonObject>(`/admin/api-credentials/${id}/status`, data),
  deleteApiCredential: (id: Id) => api.delete<JsonObject>(`/admin/api-credentials/${id}`),

  // ---- telegram ----
  getChannelClients: () => api.get<JsonObject[]>('/admin/channel-clients'),
  createChannelClient: (data: { name: string; channel_type: string; description?: string; bot_token?: string; callback_url?: string }) =>
    api.post<JsonObject>('/admin/channel-clients', data),
  getChannelClient: (id: Id) => api.get<JsonObject>(`/admin/channel-clients/${id}`),
  updateChannelClient: (id: Id, data: { name?: string; description?: string; bot_token?: string; callback_url?: string }) =>
    api.put<JsonObject>(`/admin/channel-clients/${id}`, data),
  updateChannelClientStatus: (id: Id, data: { status: number }) => api.put<JsonObject>(`/admin/channel-clients/${id}/status`, data),
  resetChannelClientSecret: (id: Id) => api.post<JsonObject>(`/admin/channel-clients/${id}/reset-secret`),
  deleteChannelClient: (id: Id) => api.delete<JsonObject>(`/admin/channel-clients/${id}`),
  getTelegramBotSettings: () => api.get<JsonObject>('/admin/settings/telegram-bot'),
  updateTelegramBotSettings: (data: JsonObject) => api.put<JsonObject>('/admin/settings/telegram-bot', data),
  getTelegramBotRuntimeStatus: () => api.get<AdminTelegramBotRuntimeStatus>('/admin/settings/telegram-bot/runtime-status'),
  getTelegramBroadcasts: (params?: QueryParams) => api.get<AdminTelegramBroadcast[]>('/admin/telegram-bot/broadcasts', { params }),
  getTelegramBroadcast: (id: Id) => api.get<AdminTelegramBroadcast>(`/admin/telegram-bot/broadcasts/${id}`),
  createTelegramBroadcast: (data: {
    title: string
    recipient_type: string
    user_ids?: number[]
    filters?: JsonObject
    attachment_url?: string
    attachment_name?: string
    message_html: string
  }) => api.post<AdminTelegramBroadcast>('/admin/telegram-bot/broadcasts', data),
  deleteTelegramBroadcast: (id: Id) => api.delete<JsonObject>(`/admin/telegram-bot/broadcasts/${id}`),
  getTelegramBroadcastUsers: (params?: QueryParams) => api.get<AdminTelegramBroadcastUser[]>('/admin/telegram-bot/users', { params }),

  // ---- ads ----
  renderAdSlot: (slotCode: string, params?: Record<string, string>) =>
    api.get<AdRenderResponse>(`/admin/ads/render/${slotCode}`, { params, silent: true }),
  reportAdImpression: (data: { tenant: string; client: string; slot_code: string; items: Array<{ ad_id: number; impression_token: string }> }) =>
    api.post<JsonObject>('/admin/ads/impression', data, { silent: true }),
}

export type AdminAPI = typeof adminAPI
