import type { RouteRecordRaw } from 'vue-router'

declare module 'vue-router' {
  interface RouteMeta {
    requiresAuth?: boolean
    /** `METHOD:/admin/path` required to open the route */
    permission?: string
    /** Payment/finance route gated by the compliance acknowledgement */
    compliance?: boolean
  }
}

export const adminChildRoutes: RouteRecordRaw[] = [
  { path: '', name: 'dashboard-home', component: () => import('@/views/Dashboard') },
  { path: 'forbidden', name: 'forbidden', component: () => import('@/views/Forbidden') },
  { path: 'compliance-required', name: 'compliance-required', component: () => import('@/views/ComplianceRequired') },
  // catalog
  { path: 'products', name: 'products', component: () => import('@/views/catalog/Products'), meta: { permission: 'GET:/admin/products' } },
  { path: 'categories', name: 'categories', component: () => import('@/views/catalog/Categories'), meta: { permission: 'GET:/admin/categories' } },
  { path: 'card-secrets', name: 'card-secrets', component: () => import('@/views/catalog/CardSecrets'), meta: { permission: 'GET:/admin/card-secrets' } },
  { path: 'card-secret-imports', name: 'card-secret-imports', component: () => import('@/views/catalog/CardSecretImports'), meta: { permission: 'GET:/admin/card-secrets' } },
  { path: 'card-secret-exports', name: 'card-secret-exports', component: () => import('@/views/catalog/CardSecretExports'), meta: { permission: 'GET:/admin/card-secrets' } },
  { path: 'gift-cards', name: 'gift-cards', component: () => import('@/views/marketing/GiftCards'), meta: { permission: 'GET:/admin/gift-cards' } },
  { path: 'wholesale-prices', name: 'wholesale-prices', component: () => import('@/views/marketing/WholesalePrices'), meta: { permission: 'GET:/admin/products' } },
  // orders
  { path: 'orders', name: 'orders', component: () => import('@/views/orders/Orders'), meta: { permission: 'GET:/admin/orders' } },
  { path: 'order-risk-control', name: 'order-risk-control', component: () => import('@/views/orders/OrderRiskControl'), meta: { permission: 'GET:/admin/settings' } },
  { path: 'order-refunds', name: 'order-refunds', component: () => import('@/views/orders/OrderRefunds'), meta: { permission: 'GET:/admin/order-refunds' } },
  // payments
  { path: 'payments', name: 'payments', component: () => import('@/views/payments/Payments'), meta: { permission: 'GET:/admin/payments', compliance: true } },
  { path: 'payment-channels', name: 'payment-channels', component: () => import('@/views/payments/PaymentChannels'), meta: { permission: 'GET:/admin/payment-channels', compliance: true } },
  { path: 'callback-routes', name: 'callback-routes', component: () => import('@/views/payments/CallbackRoutes'), meta: { permission: 'GET:/admin/settings' } },
  // users
  { path: 'users', name: 'users', component: () => import('@/views/users/Users'), meta: { permission: 'GET:/admin/users' } },
  { path: 'users/:id', name: 'user-detail', component: () => import('@/views/users/UserDetail'), meta: { permission: 'GET:/admin/users/:id' } },
  { path: 'user-login-logs', name: 'user-login-logs', component: () => import('@/views/users/UserLoginLogs'), meta: { permission: 'GET:/admin/user-login-logs' } },
  { path: 'wallet-recharges', name: 'wallet-recharges', component: () => import('@/views/users/WalletRecharges'), meta: { permission: 'GET:/admin/wallet/recharges', compliance: true } },
  { path: 'wallet-config', name: 'wallet-config', component: () => import('@/views/users/Wallet'), meta: { permission: 'GET:/admin/settings', compliance: true } },
  { path: 'member-levels', name: 'member-levels', component: () => import('@/views/users/MemberLevels'), meta: { permission: 'GET:/admin/member-levels' } },
  // content
  { path: 'posts/categories', name: 'postCategories', component: () => import('@/views/content/PostCategories'), meta: { permission: 'GET:/admin/post-categories' } },
  { path: 'posts', redirect: '/posts/blog' },
  { path: 'posts/:type(blog|notice)', name: 'posts', component: () => import('@/views/content/Posts'), meta: { permission: 'GET:/admin/posts' } },
  { path: 'banners', name: 'banners', component: () => import('@/views/content/Banners'), meta: { permission: 'GET:/admin/banners' } },
  { path: 'media', name: 'media', component: () => import('@/views/content/Media'), meta: { permission: 'GET:/admin/media' } },
  // marketing
  { path: 'coupons', name: 'coupons', component: () => import('@/views/marketing/Coupons'), meta: { permission: 'GET:/admin/coupons' } },
  { path: 'promotions', name: 'promotions', component: () => import('@/views/marketing/Promotions'), meta: { permission: 'GET:/admin/promotions' } },
  // affiliate
  { path: 'affiliates/settings', name: 'affiliates-settings', component: () => import('@/views/affiliate/AffiliateSettings'), meta: { permission: 'GET:/admin/settings/affiliate' } },
  { path: 'affiliates/users', name: 'affiliates-users', component: () => import('@/views/affiliate/AffiliateUsers'), meta: { permission: 'GET:/admin/affiliates/users' } },
  { path: 'affiliates/commissions', name: 'affiliates-commissions', component: () => import('@/views/affiliate/AffiliateCommissions'), meta: { permission: 'GET:/admin/affiliates/commissions', compliance: true } },
  { path: 'affiliates/withdraws', name: 'affiliates-withdraws', component: () => import('@/views/affiliate/AffiliateWithdraws'), meta: { permission: 'GET:/admin/affiliates/withdraws', compliance: true } },
  // resellers
  { path: 'resellers/operations', name: 'resellers-operations', component: () => import('@/views/reseller/ResellerOperationsDashboard'), meta: { permission: 'GET:/admin/resellers/operations/overview', compliance: true } },
  { path: 'resellers/profiles', name: 'resellers-profiles', component: () => import('@/views/reseller/ResellerProfiles'), meta: { permission: 'GET:/admin/resellers/profiles' } },
  { path: 'resellers/profiles/:id', name: 'resellers-profile-detail', component: () => import('@/views/reseller/ResellerProfileDetail'), meta: { permission: 'GET:/admin/resellers/profiles/:id' } },
  { path: 'resellers/domains', name: 'resellers-domains', component: () => import('@/views/reseller/ResellerDomains'), meta: { permission: 'GET:/admin/resellers/domains' } },
  { path: 'resellers/site-configs', name: 'resellers-site-configs', component: () => import('@/views/reseller/ResellerSiteConfigs'), meta: { permission: 'GET:/admin/resellers/site-configs' } },
  { path: 'resellers/product-settings', name: 'resellers-product-settings', component: () => import('@/views/reseller/ResellerProductSettings'), meta: { permission: 'GET:/admin/resellers/product-settings' } },
  { path: 'resellers/ledger-entries', name: 'resellers-ledger-entries', component: () => import('@/views/reseller/ResellerLedgerEntries'), meta: { permission: 'GET:/admin/resellers/ledger-entries', compliance: true } },
  { path: 'resellers/balance-accounts', name: 'resellers-balance-accounts', component: () => import('@/views/reseller/ResellerBalanceAccounts'), meta: { permission: 'GET:/admin/resellers/balance-accounts', compliance: true } },
  { path: 'resellers/withdraws', name: 'resellers-withdraws', component: () => import('@/views/reseller/ResellerWithdraws'), meta: { permission: 'GET:/admin/resellers/withdraws', compliance: true } },
  // integration
  { path: 'site-connections', name: 'site-connections', component: () => import('@/views/integration/SiteConnections'), meta: { permission: 'GET:/admin/site-connections' } },
  { path: 'product-mappings', name: 'product-mappings', component: () => import('@/views/integration/ProductMappings'), meta: { permission: 'GET:/admin/product-mappings' } },
  { path: 'card-converters', name: 'card-converters', component: () => import('@/views/integration/CardConverters'), meta: { permission: 'GET:/admin/card-converters' } },
  { path: 'procurement-orders', name: 'procurement-orders', component: () => import('@/views/integration/ProcurementOrders'), meta: { permission: 'GET:/admin/procurement-orders' } },
  { path: 'reconciliation', name: 'reconciliation', component: () => import('@/views/integration/Reconciliation'), meta: { permission: 'GET:/admin/reconciliation/jobs', compliance: true } },
  { path: 'api-credentials', name: 'api-credentials', component: () => import('@/views/integration/ApiCredentials'), meta: { permission: 'GET:/admin/api-credentials' } },
  // telegram bot
  { path: 'telegram-bot', name: 'telegram-bot', component: () => import('@/views/telegram/TelegramBot'), meta: { permission: 'GET:/admin/settings/telegram-bot' } },
  { path: 'telegram-bot/settings', name: 'telegram-bot-settings', component: () => import('@/views/telegram/TelegramBotSettings'), meta: { permission: 'GET:/admin/settings/telegram-bot' } },
  { path: 'telegram-bot/help-center', name: 'telegram-bot-help-center', component: () => import('@/views/telegram/TelegramBotHelpCenter'), meta: { permission: 'GET:/admin/settings/telegram-bot' } },
  { path: 'telegram-bot/menu', name: 'telegram-bot-menu-settings', component: () => import('@/views/telegram/TelegramBotMenuSettings'), meta: { permission: 'GET:/admin/settings/telegram-bot' } },
  { path: 'telegram-bot/status', name: 'telegram-bot-status', component: () => import('@/views/telegram/TelegramBotStatus'), meta: { permission: 'GET:/admin/settings/telegram-bot' } },
  { path: 'telegram-bot/channel-clients', name: 'telegram-bot-channel-clients', component: () => import('@/views/telegram/TelegramBotChannelClients'), meta: { permission: 'GET:/admin/channel-clients' } },
  { path: 'telegram-bot/broadcasts', name: 'telegram-bot-broadcasts', component: () => import('@/views/telegram/TelegramBotBroadcasts'), meta: { permission: 'GET:/admin/telegram-bot/broadcasts' } },
  { path: 'telegram-bot/broadcasts/create', name: 'telegram-bot-broadcast-create', component: () => import('@/views/telegram/TelegramBotBroadcastCreate'), meta: { permission: 'GET:/admin/telegram-bot/broadcasts' } },
  { path: 'telegram-bot/broadcasts/:id', name: 'telegram-bot-broadcast-detail', component: () => import('@/views/telegram/TelegramBotBroadcastDetail'), meta: { permission: 'GET:/admin/telegram-bot/broadcasts' } },
  // system
  { path: 'settings', name: 'settings', component: () => import('@/views/system/Settings'), meta: { permission: 'GET:/admin/settings' } },
  { path: 'settings/notifications', name: 'notifications', component: () => import('@/views/system/Notifications'), meta: { permission: 'GET:/admin/settings/notification-center' } },
  { path: 'authz', name: 'authz', component: () => import('@/views/system/Authz'), meta: { permission: 'GET:/admin/authz/roles' } },
  { path: 'authz-audit-logs', name: 'authz-audit-logs', component: () => import('@/views/system/AuthzAuditLogs'), meta: { permission: 'GET:/admin/authz/audit-logs' } },
  { path: 'security', name: 'security', component: () => import('@/views/system/Security') },
]

export const routes: RouteRecordRaw[] = [
  { path: '/login', name: 'login', component: () => import('@/views/Login') },
  { path: '/', component: () => import('@/layouts/AdminLayout'), meta: { requiresAuth: true }, children: adminChildRoutes },
  { path: '/:pathMatch(.*)*', redirect: '/' },
]
