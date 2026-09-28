import { defineComponent, onBeforeUnmount, onMounted, ref, Teleport, Transition, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ClipboardList, LogIn, LogOut, Menu, ShoppingCart, User, X } from 'lucide-vue-next'
import { useNavConfig, type NavItem } from '@/composables/useNavConfig'
import { useCartStore } from '@/stores/cart'
import { useUserAuthStore } from '@/stores/userAuth'
import { cn } from '@/components/ui'
import { LanguageSwitcher } from './LanguageSwitcher'
import { SiteLogo } from './SiteLogo'
import { ThemeToggle } from './ThemeToggle'

/** Fixed glass header that shrinks on scroll. */
export const Navbar = defineComponent({
  name: 'Navbar',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const router = useRouter()
    const cart = useCartStore()
    const auth = useUserAuthStore()
    const { primaryNavItems } = useNavConfig()
    const scrolled = ref(false)
    const drawer = ref(false)
    const bump = ref(false)

    const onScroll = () => {
      scrolled.value = window.scrollY > 12
    }
    onMounted(() => {
      onScroll()
      window.addEventListener('scroll', onScroll, { passive: true })
    })
    onBeforeUnmount(() => window.removeEventListener('scroll', onScroll))
    watch(
      () => cart.totalItems,
      (n, old) => {
        if (n > old) {
          bump.value = false
          requestAnimationFrame(() => {
            bump.value = true
          })
        }
      },
    )
    watch(
      () => route.fullPath,
      () => {
        drawer.value = false
      },
    )

    const isActive = (item: NavItem) => {
      if (item.type !== 'route') return false
      if (item.path === '/') return route.path === '/'
      if (item.path === '/products') return route.path.startsWith('/products') || route.path.startsWith('/categories')
      return route.path.startsWith(item.path)
    }

    const logout = () => {
      auth.logout()
      void router.push('/auth/login')
    }

    const navLink = (item: NavItem, mobile = false) => {
      const Icon = item.icon
      const cls = cn(
        'flex items-center gap-2 rounded-full font-bold transition',
        mobile ? 'px-4 py-3 text-base' : 'h-10 px-3.5 text-sm',
        isActive(item) ? 'bg-primary-soft text-primary-text' : 'text-muted hover:bg-surface-muted hover:text-fg',
      )
      const inner = [<Icon class="size-4" />, <span>{item.label}</span>]
      return item.type === 'link' ? (
        <a key={item.key} href={item.path} target={item.target} rel="noopener noreferrer" class={cls}>
          {inner}
        </a>
      ) : (
        <RouterLink key={item.key} to={item.path} class={cls}>
          {inner}
        </RouterLink>
      )
    }

    const iconBtn = 'flex h-10 items-center gap-1.5 rounded-full px-3 text-sm font-bold text-muted transition hover:bg-primary-soft hover:text-primary-text'

    return () => (
      <header class="sticky top-0 z-40 w-full px-2 pt-2 sm:px-4">
        <div
          class={cn(
            'zs-glass mx-auto flex max-w-7xl items-center justify-between gap-3 rounded-full px-3 transition-all duration-300 sm:px-5',
            scrolled.value ? 'h-14 shadow-zs' : 'h-16',
          )}
        >
          <SiteLogo />
          <nav class="hidden items-center gap-1 lg:flex">{primaryNavItems.value.map((item) => navLink(item))}</nav>
          <div class="flex items-center gap-0.5">
            <RouterLink to="/cart" class={cn(iconBtn, 'relative')} aria-label={t('navbar.cart')}>
              <span class="relative">
                <ShoppingCart class={cn('size-[18px]', bump.value && 'zs-wiggle')} />
                {cart.totalItems > 0 && (
                  <span class="zs-gradient-bg absolute -top-2.5 -right-2.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] leading-none text-on-primary zs-num ring-2 ring-surface-solid">
                    {cart.totalItems > 99 ? '99+' : cart.totalItems}
                  </span>
                )}
              </span>
              <span class="hidden xl:inline">{t('navbar.cart')}</span>
            </RouterLink>
            {auth.isAuthenticated ? (
              <>
                <RouterLink to="/me" class={cn(iconBtn, 'hidden sm:flex', route.path.startsWith('/me') && 'bg-primary-soft text-primary-text')}>
                  <User class="size-[18px]" />
                  <span class="hidden xl:inline">{t('navbar.personalCenter')}</span>
                </RouterLink>
                <button type="button" class={cn(iconBtn, 'hidden text-danger-text sm:flex')} onClick={logout}>
                  <LogOut class="size-[18px]" />
                  <span class="hidden xl:inline">{t('navbar.logout')}</span>
                </button>
              </>
            ) : (
              <>
                <RouterLink to="/guest/orders" class={cn(iconBtn, 'hidden sm:flex')}>
                  <ClipboardList class="size-[18px]" />
                  <span class="hidden xl:inline">{t('navbar.guestOrders')}</span>
                </RouterLink>
                <RouterLink to="/auth/login" class={cn(iconBtn, 'hidden sm:flex')}>
                  <LogIn class="size-[18px]" />
                  <span class="hidden xl:inline">{t('navbar.login')}</span>
                </RouterLink>
              </>
            )}
            <ThemeToggle />
            <div class="hidden sm:block">
              <LanguageSwitcher />
            </div>
            <button type="button" class={cn(iconBtn, 'lg:hidden')} aria-label={t('zs.menu')} onClick={() => (drawer.value = true)}>
              <Menu class="size-5" />
            </button>
          </div>
        </div>

        <Teleport to="body">
          <Transition name="zs-fade">
            {drawer.value && (
              <div class="fixed inset-0 z-[60] bg-[var(--zs-overlay)] backdrop-blur-sm lg:hidden" onClick={() => (drawer.value = false)}>
                <aside
                  class="zs-pop absolute right-3 top-3 bottom-3 flex w-[min(20rem,85vw)] flex-col gap-2 overflow-y-auto rounded-zs-lg border border-line bg-surface-solid p-4 shadow-zs-lg"
                  onClick={(e: MouseEvent) => e.stopPropagation()}
                >
                  <div class="mb-2 flex items-center justify-between">
                    <SiteLogo />
                    <button type="button" aria-label={t('zs.closeMenu')} class="flex size-9 items-center justify-center rounded-full hover:bg-primary-soft" onClick={() => (drawer.value = false)}>
                      <X class="size-5" />
                    </button>
                  </div>
                  {primaryNavItems.value.map((item) => navLink(item, true))}
                  <div class="zs-divider my-2" />
                  {auth.isAuthenticated ? (
                    <>
                      <RouterLink to="/me" class="flex items-center gap-2 rounded-full px-4 py-3 font-bold text-fg hover:bg-surface-muted">
                        <User class="size-4" />
                        {t('navbar.personalCenter')}
                      </RouterLink>
                      <button type="button" class="flex items-center gap-2 rounded-full px-4 py-3 text-left font-bold text-danger-text hover:bg-danger-soft" onClick={logout}>
                        <LogOut class="size-4" />
                        {t('navbar.logout')}
                      </button>
                    </>
                  ) : (
                    <>
                      <RouterLink to="/guest/orders" class="flex items-center gap-2 rounded-full px-4 py-3 font-bold text-fg hover:bg-surface-muted">
                        <ClipboardList class="size-4" />
                        {t('navbar.guestOrders')}
                      </RouterLink>
                      <RouterLink to="/auth/login" class="flex items-center gap-2 rounded-full px-4 py-3 font-bold text-fg hover:bg-surface-muted">
                        <LogIn class="size-4" />
                        {t('navbar.login')}
                      </RouterLink>
                    </>
                  )}
                  <div class="mt-auto flex items-center justify-between pt-4">
                    <span class="text-sm text-muted">{t('zs.language')}</span>
                    <LanguageSwitcher />
                  </div>
                </aside>
              </div>
            )}
          </Transition>
        </Teleport>
      </header>
    )
  },
})
