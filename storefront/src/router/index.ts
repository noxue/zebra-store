import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'
import { captureAffiliateFromRoute } from '@/utils/affiliate'
import { installNavigationFeedback } from './navigation'

declare module 'vue-router' {
  interface RouteMeta {
    requiresUserAuth?: boolean
    userGuest?: boolean
    resellerConsole?: boolean
  }
}

const Home = () => import('@/views/Home')
const Products = () => import('@/views/Products')
const PersonalCenter = () => import('@/views/PersonalCenter')

/** In list template mode `/products` and `/categories/:slug` render Home (same as original). */
const productsOrHome = () => (useAppStore().isListMode ? Home() : Products())

const personal = (path: string, name: string, section: string): RouteRecordRaw => ({
  path,
  name,
  component: PersonalCenter,
  props: { section },
  meta: { requiresUserAuth: true },
})

export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: Home },
  { path: '/products', name: 'products', component: productsOrHome },
  { path: '/categories/:slug', name: 'category-products', component: productsOrHome },
  { path: '/products/:slug', name: 'product-detail', component: () => import('@/views/ProductDetail') },
  { path: '/cart', name: 'cart', component: () => import('@/views/Cart') },
  { path: '/checkout', name: 'checkout', component: () => import('@/views/Checkout') },
  { path: '/pay', name: 'payment', component: () => import('@/views/Payment') },
  personal('/me', 'personal-center', 'overview'),
  personal('/me/profile', 'personal-center-profile', 'profile'),
  personal('/me/security', 'personal-center-security', 'security'),
  personal('/me/orders', 'personal-center-orders', 'orders'),
  personal('/me/wallet', 'personal-center-wallet', 'wallet'),
  personal('/me/gift-cards', 'personal-center-gift-cards', 'giftCard'),
  personal('/me/api', 'personal-center-api', 'api'),
  personal('/me/affiliate', 'personal-center-affiliate', 'affiliate'),
  { path: '/me/reseller', name: 'personal-center-reseller', redirect: '/reseller', meta: { requiresUserAuth: true, resellerConsole: true } },
  {
    path: '/reseller',
    component: () => import('@/views/reseller/ResellerLayout'),
    meta: { requiresUserAuth: true, resellerConsole: true },
    children: [
      { path: '', name: 'reseller-dashboard', component: () => import('@/views/reseller/ResellerDashboard') },
      { path: 'apply', name: 'reseller-apply', component: () => import('@/views/reseller/ResellerApply') },
      { path: 'domains', name: 'reseller-domains', component: () => import('@/views/reseller/ResellerDomains') },
      { path: 'site', name: 'reseller-site', component: () => import('@/views/reseller/ResellerSite') },
      { path: 'products', name: 'reseller-products', component: () => import('@/views/reseller/ResellerProducts') },
      { path: 'orders', name: 'reseller-orders', component: () => import('@/views/reseller/ResellerOrders') },
      { path: 'orders/:order_no', name: 'reseller-order-detail', component: () => import('@/views/reseller/ResellerOrderDetail') },
      { path: 'finance', name: 'reseller-finance', component: () => import('@/views/reseller/ResellerFinance') },
      { path: 'ledger', name: 'reseller-ledger', component: () => import('@/views/reseller/ResellerLedger') },
      { path: 'withdraws', name: 'reseller-withdraws', component: () => import('@/views/reseller/ResellerWithdraws') },
    ],
  },
  { path: '/orders/:order_no', name: 'order-detail', component: () => import('@/views/OrderDetail'), meta: { requiresUserAuth: true } },
  {
    path: '/recharge-orders/:recharge_no',
    name: 'recharge-order-detail',
    component: () => import('@/views/RechargeOrderDetail'),
    meta: { requiresUserAuth: true },
  },
  { path: '/guest/orders', name: 'guest-orders', component: () => import('@/views/GuestOrders') },
  { path: '/guest/orders/:order_no', name: 'guest-order-detail', component: () => import('@/views/GuestOrderDetail') },
  { path: '/blog', name: 'blog', component: () => import('@/views/Blog') },
  { path: '/blog/:slug', name: 'blog-detail', component: () => import('@/views/BlogDetail') },
  { path: '/notice', name: 'notice', component: () => import('@/views/Notice') },
  { path: '/about', name: 'about', component: () => import('@/views/About') },
  { path: '/terms', name: 'terms', component: () => import('@/views/Legal'), props: { type: 'terms' } },
  { path: '/privacy', name: 'privacy', component: () => import('@/views/Legal'), props: { type: 'privacy' } },
  { path: '/auth/login', name: 'user-login', component: () => import('@/views/auth/Login'), meta: { userGuest: true } },
  { path: '/auth/register', name: 'user-register', component: () => import('@/views/auth/Register'), meta: { userGuest: true } },
  { path: '/auth/forgot', name: 'user-forgot', component: () => import('@/views/auth/Forgot'), meta: { userGuest: true } },
  { path: '/auth/telegram/callback', name: 'user-telegram-callback', component: () => import('@/views/auth/TelegramCallback') },
  { path: '/auth/google/callback', name: 'user-google-callback', component: () => import('@/views/auth/GoogleCallback') },
  { path: '/:pathMatch(.*)*', name: 'not-found', component: () => import('@/views/NotFound') },
]

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  scrollBehavior(to, _from, saved) {
    if (saved) return saved
    if (to.hash) return { el: to.hash, top: 80 }
    return { top: 0 }
  },
  routes,
})

installNavigationFeedback(router)

router.beforeEach(async (to) => {
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  void captureAffiliateFromRoute(to)
  if (!appStore.config) await appStore.loadConfig()
  if (to.meta.requiresUserAuth) {
    if (!auth.isAuthenticated) return `/auth/login?redirect=${encodeURIComponent(to.fullPath)}`
    if (to.meta.resellerConsole && !appStore.canAccessResellerConsole) return '/me/orders'
    return true
  }
  if (to.meta.userGuest && auth.isAuthenticated) return '/me/orders'
  return true
})

export default router
