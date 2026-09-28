import { createRouter, createWebHistory } from 'vue-router'
import { useAdminAuthStore } from '@/stores/auth'
import { ADMIN_BASE } from '@/utils/adminBase'
import { routes } from './routes'

const router = createRouter({
  history: createWebHistory(ADMIN_BASE || '/'),
  routes,
  scrollBehavior: () => ({ top: 0 }),
})

router.beforeEach(async (to) => {
  const auth = useAdminAuthStore()
  const requiresAuth = to.matched.some((r) => r.meta.requiresAuth)

  if (requiresAuth && !auth.token) return { path: '/login' }
  if (to.path === '/login' && auth.token) return { path: '/' }

  if (requiresAuth && auth.token && !auth.permissionsLoaded) {
    try {
      await auth.loadAuthz()
    } catch {
      auth.logout()
      return { path: '/login' }
    }
  }

  const permission = typeof to.meta.permission === 'string' ? to.meta.permission : ''
  if (permission && !auth.hasPermission(permission) && to.path !== '/forbidden') {
    return { path: '/forbidden', query: { from: to.fullPath } }
  }

  // Compliance gate: non-super admins are redirected; super admins see a blocking dialog in the page.
  if (to.meta.compliance) {
    const { useComplianceStore } = await import('@/stores/compliance')
    const compliance = useComplianceStore()
    if (!compliance.loaded) {
      try {
        await compliance.fetchStatus()
      } catch {
        /* do not block navigation; the page guard handles it */
      }
    }
    if (!compliance.acknowledged && !auth.isSuper) return { name: 'compliance-required' }
  }
  return true
})

export default router
