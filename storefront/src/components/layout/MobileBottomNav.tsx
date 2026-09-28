import { computed, defineComponent } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Home, LayoutGrid, ShoppingCart, User } from 'lucide-vue-next'
import { useAppStore } from '@/stores/app'
import { useCartStore } from '@/stores/cart'
import { useUserAuthStore } from '@/stores/userAuth'
import { cn } from '@/components/ui'

/** Bottom tab bar for < lg screens: Home / Products / Cart / Me. */
export const MobileBottomNav = defineComponent({
  name: 'MobileBottomNav',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const appStore = useAppStore()
    const cart = useCartStore()
    const auth = useUserAuthStore()
    const items = computed(() => {
      const list = [{ key: 'home', to: '/', label: t('bottomNav.home'), icon: Home, active: route.path === '/' }]
      if (!appStore.isListMode) {
        list.push({ key: 'products', to: '/products', label: t('bottomNav.products'), icon: LayoutGrid, active: route.path.startsWith('/products') || route.path.startsWith('/categories') })
      }
      list.push({ key: 'cart', to: '/cart', label: t('bottomNav.cart'), icon: ShoppingCart, active: route.path === '/cart' })
      list.push({ key: 'me', to: auth.isAuthenticated ? '/me' : '/auth/login', label: t('bottomNav.me'), icon: User, active: route.path.startsWith('/me') || route.path.startsWith('/auth') })
      return list
    })
    return () => (
      <nav class="fixed inset-x-0 bottom-0 z-40 px-3 pb-[max(0.5rem,env(safe-area-inset-bottom))] lg:hidden">
        <div class="zs-glass mx-auto flex h-14 max-w-md items-center justify-around rounded-full shadow-zs-lg">
          {items.value.map((item) => {
            const Icon = item.icon
            return (
              <RouterLink key={item.key} to={item.to} class={cn('relative flex flex-1 flex-col items-center gap-0.5 text-[11px] font-bold transition', item.active ? 'text-primary-text' : 'text-muted')}>
                <span class={cn('flex h-7 w-10 items-center justify-center rounded-full transition', item.active && 'bg-primary-soft')}>
                  <Icon class="size-[18px]" />
                </span>
                {item.label}
                {item.key === 'cart' && cart.totalItems > 0 && (
                  <span class="zs-gradient-bg absolute -top-1 left-1/2 ml-2 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] text-on-primary zs-num">
                    {cart.totalItems > 99 ? '99+' : cart.totalItems}
                  </span>
                )}
              </RouterLink>
            )
          })}
        </div>
      </nav>
    )
  },
})
