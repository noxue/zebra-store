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

  return true
})

export default router
