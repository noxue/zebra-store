import type { IconComponent } from '@/components/ui/cn'
import {
  BadgePercent,
  Bell,
  Bot,
  Boxes,
  ClipboardCheck,
  CreditCard,
  Crown,
  Download,
  FileText,
  FolderTree,
  Gift,
  History,
  ImageIcon,
  Images,
  KeyRound,
  LayoutDashboard,
  Link,
  ListOrdered,
  Lock,
  Newspaper,
  Package,
  RefreshCw,
  ReceiptText,
  ScrollText,
  Send,
  Settings,
  ShieldCheck,
  ShoppingBag,
  SlidersHorizontal,
  Ticket,
  Truck,
  UserRound,
  Users,
  Wallet,
  WalletCards,
  Wifi,
} from 'lucide-vue-next'

export interface NavItem {
  labelKey: string
  to: string
  icon: IconComponent
  permission?: string
}

export interface NavGroup {
  id: string
  labelKey: string
  icon: IconComponent
  items: NavItem[]
}

export const DASHBOARD_ITEM: NavItem = { labelKey: 'admin.navItems.dashboard', to: '/', icon: LayoutDashboard }

/** Sidebar groups — identical order, paths and permissions to the original AdminLayout.vue. */
export const NAV_GROUPS: NavGroup[] = [
  {
    id: 'products',
    labelKey: 'admin.navGroups.productManagement',
    icon: Package,
    items: [
      { labelKey: 'admin.navItems.productCategories', to: '/categories', icon: FolderTree, permission: 'GET:/admin/categories' },
      { labelKey: 'admin.navItems.productList', to: '/products', icon: Boxes, permission: 'GET:/admin/products' },
      { labelKey: 'admin.navItems.cardSecrets', to: '/card-secrets', icon: KeyRound, permission: 'GET:/admin/card-secrets' },
      { labelKey: 'admin.navItems.cardSecretImports', to: '/card-secret-imports', icon: KeyRound, permission: 'GET:/admin/card-secrets' },
      { labelKey: 'admin.navItems.cardSecretExports', to: '/card-secret-exports', icon: Download, permission: 'GET:/admin/card-secrets' },
    ],
  },
  {
    id: 'orders',
    labelKey: 'admin.navGroups.orderManagement',
    icon: ShoppingBag,
    items: [
      { labelKey: 'admin.navItems.orderList', to: '/orders', icon: ListOrdered, permission: 'GET:/admin/orders' },
      { labelKey: 'admin.navItems.orderRiskControl', to: '/order-risk-control', icon: ShieldCheck, permission: 'GET:/admin/settings' },
      { labelKey: 'admin.navItems.orderRefunds', to: '/order-refunds', icon: ReceiptText, permission: 'GET:/admin/order-refunds' },
    ],
  },
  {
    id: 'payments',
    labelKey: 'admin.navGroups.paymentManagement',
    icon: CreditCard,
    items: [
      { labelKey: 'admin.navItems.paymentChannels', to: '/payment-channels', icon: WalletCards, permission: 'GET:/admin/payment-channels' },
      { labelKey: 'admin.navItems.payments', to: '/payments', icon: ReceiptText, permission: 'GET:/admin/payments' },
      { labelKey: 'admin.navItems.callbackRoutes', to: '/callback-routes', icon: Link, permission: 'GET:/admin/settings' },
    ],
  },
  {
    id: 'users',
    labelKey: 'admin.navGroups.userManagement',
    icon: Users,
    items: [
      { labelKey: 'admin.navItems.userList', to: '/users', icon: UserRound, permission: 'GET:/admin/users' },
      { labelKey: 'admin.navItems.walletManagement', to: '/wallet-recharges', icon: Wallet, permission: 'GET:/admin/wallet/recharges' },
      { labelKey: 'admin.navItems.walletConfig', to: '/wallet-config', icon: Wallet, permission: 'GET:/admin/settings' },
      { labelKey: 'admin.navItems.userLoginLogs', to: '/user-login-logs', icon: History, permission: 'GET:/admin/user-login-logs' },
      { labelKey: 'admin.navItems.memberLevels', to: '/member-levels', icon: Crown, permission: 'GET:/admin/member-levels' },
    ],
  },
  {
    id: 'articles',
    labelKey: 'admin.navGroups.articleManagement',
    icon: Newspaper,
    items: [
      { labelKey: 'admin.navItems.articleList', to: '/posts/blog', icon: FileText, permission: 'GET:/admin/posts' },
      { labelKey: 'admin.navItems.postCategories', to: '/posts/categories', icon: FolderTree, permission: 'GET:/admin/post-categories' },
    ],
  },
  {
    id: 'content',
    labelKey: 'admin.navGroups.contentManagement',
    icon: FileText,
    items: [
      { labelKey: 'admin.navItems.banners', to: '/banners', icon: Images, permission: 'GET:/admin/banners' },
      { labelKey: 'admin.navItems.media', to: '/media', icon: ImageIcon, permission: 'GET:/admin/media' },
      { labelKey: 'admin.navItems.announcementList', to: '/posts/notice', icon: Bell, permission: 'GET:/admin/posts' },
    ],
  },
  {
    id: 'marketing',
    labelKey: 'admin.navGroups.marketingManagement',
    icon: BadgePercent,
    items: [
      { labelKey: 'admin.navItems.coupons', to: '/coupons', icon: Ticket, permission: 'GET:/admin/coupons' },
      { labelKey: 'admin.navItems.promotions', to: '/promotions', icon: BadgePercent, permission: 'GET:/admin/promotions' },
      { labelKey: 'admin.navItems.wholesalePrices', to: '/wholesale-prices', icon: BadgePercent, permission: 'GET:/admin/products' },
      { labelKey: 'admin.navItems.giftCards', to: '/gift-cards', icon: Gift, permission: 'GET:/admin/gift-cards' },
    ],
  },
  {
    id: 'affiliate',
    labelKey: 'admin.navGroups.affiliateManagement',
    icon: BadgePercent,
    items: [
      { labelKey: 'admin.navItems.affiliatesSettings', to: '/affiliates/settings', icon: SlidersHorizontal, permission: 'GET:/admin/settings/affiliate' },
      { labelKey: 'admin.navItems.affiliatesUsers', to: '/affiliates/users', icon: Users, permission: 'GET:/admin/affiliates/users' },
      { labelKey: 'admin.navItems.affiliatesCommissions', to: '/affiliates/commissions', icon: ReceiptText, permission: 'GET:/admin/affiliates/commissions' },
      { labelKey: 'admin.navItems.affiliatesWithdraws', to: '/affiliates/withdraws', icon: WalletCards, permission: 'GET:/admin/affiliates/withdraws' },
    ],
  },
  {
    id: 'reseller',
    labelKey: 'admin.navGroups.resellerManagement',
    icon: Users,
    items: [
      { labelKey: 'admin.navItems.resellerOperations', to: '/resellers/operations', icon: LayoutDashboard, permission: 'GET:/admin/resellers/operations/overview' },
      { labelKey: 'admin.navItems.resellerProfiles', to: '/resellers/profiles', icon: Users, permission: 'GET:/admin/resellers/profiles' },
      { labelKey: 'admin.navItems.resellerDomains', to: '/resellers/domains', icon: Link, permission: 'GET:/admin/resellers/domains' },
      { labelKey: 'admin.navItems.resellerSiteConfigs', to: '/resellers/site-configs', icon: Settings, permission: 'GET:/admin/resellers/site-configs' },
      { labelKey: 'admin.navItems.resellerProductSettings', to: '/resellers/product-settings', icon: SlidersHorizontal, permission: 'GET:/admin/resellers/product-settings' },
      { labelKey: 'admin.navItems.resellerLedgerEntries', to: '/resellers/ledger-entries', icon: ReceiptText, permission: 'GET:/admin/resellers/ledger-entries' },
      { labelKey: 'admin.navItems.resellerBalanceAccounts', to: '/resellers/balance-accounts', icon: Wallet, permission: 'GET:/admin/resellers/balance-accounts' },
      { labelKey: 'admin.navItems.resellerWithdraws', to: '/resellers/withdraws', icon: WalletCards, permission: 'GET:/admin/resellers/withdraws' },
    ],
  },
  {
    id: 'integration',
    labelKey: 'admin.navGroups.integrationManagement',
    icon: Link,
    items: [
      { labelKey: 'admin.navItems.siteConnections', to: '/site-connections', icon: Link, permission: 'GET:/admin/site-connections' },
      { labelKey: 'admin.navItems.productMappings', to: '/product-mappings', icon: Boxes, permission: 'GET:/admin/product-mappings' },
      { labelKey: 'admin.navItems.cardConverters', to: '/card-converters', icon: RefreshCw, permission: 'GET:/admin/card-converters' },
      { labelKey: 'admin.navItems.procurementOrders', to: '/procurement-orders', icon: Truck, permission: 'GET:/admin/procurement-orders' },
      { labelKey: 'admin.navItems.reconciliation', to: '/reconciliation', icon: ClipboardCheck, permission: 'GET:/admin/reconciliation/jobs' },
      { labelKey: 'admin.navItems.apiCredentials', to: '/api-credentials', icon: KeyRound, permission: 'GET:/admin/api-credentials' },
    ],
  },
  {
    id: 'telegramBot',
    labelKey: 'admin.navGroups.telegramBot',
    icon: Bot,
    items: [
      { labelKey: 'admin.navItems.telegramBotOverview', to: '/telegram-bot', icon: Bot, permission: 'GET:/admin/settings/telegram-bot' },
      { labelKey: 'admin.navItems.telegramBotSettings', to: '/telegram-bot/settings', icon: SlidersHorizontal, permission: 'GET:/admin/settings/telegram-bot' },
      { labelKey: 'admin.navItems.telegramBotHelpCenter', to: '/telegram-bot/help-center', icon: ScrollText, permission: 'GET:/admin/settings/telegram-bot' },
      { labelKey: 'admin.navItems.telegramBotMenuSettings', to: '/telegram-bot/menu', icon: ListOrdered, permission: 'GET:/admin/settings/telegram-bot' },
      { labelKey: 'admin.navItems.telegramBotStatus', to: '/telegram-bot/status', icon: Wifi, permission: 'GET:/admin/settings/telegram-bot' },
      { labelKey: 'admin.navItems.telegramBotChannelClients', to: '/telegram-bot/channel-clients', icon: KeyRound, permission: 'GET:/admin/channel-clients' },
      { labelKey: 'admin.navItems.telegramBotBroadcasts', to: '/telegram-bot/broadcasts', icon: Send, permission: 'GET:/admin/telegram-bot/broadcasts' },
    ],
  },
  {
    id: 'system',
    labelKey: 'admin.navGroups.systemSettings',
    icon: Settings,
    items: [
      { labelKey: 'admin.navItems.siteSettings', to: '/settings', icon: SlidersHorizontal, permission: 'GET:/admin/settings' },
      { labelKey: 'admin.navItems.notificationCenter', to: '/settings/notifications', icon: Bell, permission: 'GET:/admin/settings/notification-center' },
      { labelKey: 'admin.navItems.authz', to: '/authz', icon: ShieldCheck, permission: 'GET:/admin/authz/roles' },
      { labelKey: 'admin.navItems.authzAudit', to: '/authz-audit-logs', icon: ScrollText, permission: 'GET:/admin/authz/audit-logs' },
      { labelKey: 'admin.navItems.security', to: '/security', icon: Lock },
    ],
  },
]

export interface ResolvedNavGroup extends Omit<NavGroup, 'items' | 'labelKey'> {
  label: string
  items: Array<NavItem & { label: string }>
}

/** Filter by permission, translate, then apply the nav search keyword (group or item label). */
export function resolveNav(
  groups: NavGroup[],
  t: (key: string) => string,
  can: (perm?: string) => boolean,
  keyword: string,
): ResolvedNavGroup[] {
  const k = keyword.trim().toLowerCase()
  const out: ResolvedNavGroup[] = []
  for (const g of groups) {
    const label = t(g.labelKey)
    const items = g.items.filter((i) => can(i.permission)).map((i) => ({ ...i, label: t(i.labelKey) }))
    if (!items.length) continue
    const matched = !k || label.toLowerCase().includes(k) ? items : items.filter((i) => i.label.toLowerCase().includes(k))
    if (matched.length) out.push({ id: g.id, icon: g.icon, label, items: matched })
  }
  return out
}

/** Active check with "most specific item wins" (e.g. /posts/categories must not light /posts/blog). */
export function isNavItemActive(to: string, currentPath: string, allPaths: string[]): boolean {
  if (to === '/') return currentPath === '/'
  if (currentPath === to) return true
  if (!currentPath.startsWith(`${to}/`)) return false
  return !allPaths.some((p) => p !== to && (currentPath === p || (p.length > to.length && currentPath.startsWith(`${p}/`))))
}
