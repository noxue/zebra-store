import { computed, defineComponent, onMounted, ref } from 'vue'
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { ArrowLeft, Banknote, ClipboardCheck, FileText, Globe, LayoutGrid, LogOut, Settings, ShoppingBag, Tag, Upload, UserRound } from 'lucide-vue-next'
import { provideResellerProfile } from '@/composables/reseller/useResellerProfile'
import { usePageTitle } from '@/composables/usePageTitle'
import { useUserAuthStore } from '@/stores/userAuth'
import { LanguageSwitcher } from '@/components/layout/LanguageSwitcher'
import { SiteLogo } from '@/components/layout/SiteLogo'
import { ThemeToggle } from '@/components/layout/ThemeToggle'
import { ResellerInactiveCard, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { Button, cn } from '@/components/ui'
import { canRenderResellerConsoleModule } from '@/utils/reseller/console'

interface NavDef {
  to: string
  label: string
  icon: LucideIcon
}

const groupDefs: Array<{ key: string; title: string; items: NavDef[] }> = [
  {
    key: 'operations',
    title: 'resellerConsole.navGroups.operations',
    items: [
      {
        to: '/reseller',
        label: 'resellerConsole.nav.dashboard',
        icon: LayoutGrid,
      },
      {
        to: '/reseller/orders',
        label: 'resellerConsole.nav.orders',
        icon: ShoppingBag,
      },
      {
        to: '/reseller/finance',
        label: 'resellerConsole.nav.finance',
        icon: Banknote,
      },
      {
        to: '/reseller/ledger',
        label: 'resellerConsole.nav.ledger',
        icon: FileText,
      },
      {
        to: '/reseller/withdraws',
        label: 'resellerConsole.nav.withdraws',
        icon: Upload,
      },
    ],
  },
  {
    key: 'config',
    title: 'resellerConsole.navGroups.config',
    items: [
      {
        to: '/reseller/domains',
        label: 'resellerConsole.nav.domains',
        icon: Globe,
      },
      {
        to: '/reseller/site',
        label: 'resellerConsole.nav.site',
        icon: Settings,
      },
      {
        to: '/reseller/products',
        label: 'resellerConsole.nav.products',
        icon: Tag,
      },
    ],
  },
  {
    key: 'account',
    title: 'resellerConsole.navGroups.account',
    items: [
      {
        to: '/reseller/apply',
        label: 'resellerConsole.nav.apply',
        icon: ClipboardCheck,
      },
    ],
  },
]

/** Reseller console shell: glass topbar, grouped sidebar (desktop), scrollable chips (mobile). */
export default defineComponent({
  name: 'ResellerLayout',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const router = useRouter()
    const auth = useUserAuthStore()
    const logout = () => {
      auth.logout()
      void router.push('/auth/login')
    }
    const profile = provideResellerProfile()
    const ready = ref(false)

    const isActive = (path: string) => (path === '/reseller' ? route.path === path : route.path === path || route.path.startsWith(`${path}/`))
    const current = computed(() => groupDefs.flatMap((g) => g.items).find((i) => isActive(i.to)))
    const title = computed(() => (current.value ? t(current.value.label) : t('resellerConsole.title')))
    usePageTitle(() => `${title.value} · ${t('resellerConsole.title')}`)
    const canRender = computed(() => canRenderResellerConsoleModule(route.path, profile.state.value))

    onMounted(async () => {
      await profile.load()
      ready.value = true
    })

    const navLink = (item: NavDef, compact = false) => {
      const Icon = item.icon
      const active = isActive(item.to)
      return (
        <RouterLink
          key={item.to}
          to={item.to}
          class={cn(
            'relative flex items-center gap-2.5 rounded-full font-bold transition',
            compact ? 'h-9 shrink-0 px-3.5 text-xs' : 'px-4 py-2.5 text-sm',
            active ? 'zs-gradient-bg text-on-primary shadow-zs' : 'text-muted hover:bg-primary-soft hover:text-primary-text',
          )}
        >
          <Icon class={compact ? 'size-4' : 'size-[18px]'} />
          <span class="truncate">{t(item.label)}</span>
        </RouterLink>
      )
    }

    return () => (
      <div class="flex min-h-screen flex-col">
        <header class="sticky top-0 z-40 px-2 pt-2 sm:px-4">
          <div class="zs-glass mx-auto flex h-14 max-w-7xl items-center justify-between gap-3 rounded-full px-3 shadow-zs sm:px-5">
            <div class="flex min-w-0 items-center gap-3">
              <SiteLogo compact />
              <span class="hidden rounded-full bg-primary-soft px-3 py-1 text-xs font-bold text-primary-text sm:inline">{t('zsReseller.consoleBadge')}</span>
              <span class="zs-title truncate text-base text-fg">{title.value}</span>
            </div>
            <div class="flex items-center gap-0.5">
              <Button to="/" variant="ghost" size="sm">
                <ArrowLeft class="size-4" />
                <span class="hidden sm:inline">{t('resellerConsole.nav.backStore')}</span>
              </Button>
              <ThemeToggle />
              <div class="hidden sm:block">
                <LanguageSwitcher />
              </div>
              <Button to="/me" variant="ghost" size="sm" aria-label={t('navbar.personalCenter')}>
                <UserRound class="size-4" />
              </Button>
              {auth.isAuthenticated && (
                <div class="hidden lg:block">
                  <Button variant="ghost" size="sm" class="text-danger-text" onClick={logout}>
                    <LogOut class="size-4" />
                    {t('navbar.logout')}
                  </Button>
                </div>
              )}
            </div>
          </div>
          <nav class="zs-scroll-x mx-auto mt-2 flex max-w-7xl gap-1.5 px-1 lg:hidden" aria-label={t('zsReseller.menu')}>
            {groupDefs.flatMap((g) => g.items).map((item) => navLink(item, true))}
          </nav>
        </header>

        <div class="mx-auto flex w-full max-w-7xl flex-1 gap-6 px-3 py-5 sm:px-4 lg:py-6">
          <aside class="hidden w-60 shrink-0 lg:block">
            <div class="zs-card sticky top-24 space-y-5 p-3">
              {groupDefs.map((group) => (
                <div key={group.key}>
                  <p class="px-4 pb-1.5 text-[11px] font-bold tracking-[0.14em] text-muted">{t(group.title)}</p>
                  <div class="space-y-1">{group.items.map((item) => navLink(item))}</div>
                </div>
              ))}
            </div>
          </aside>
          <main class="min-w-0 flex-1 pb-8">
            {!ready.value ? (
              <ResellerPageState loading title={t('resellerConsole.common.loading')} />
            ) : canRender.value ? (
              <RouterView />
            ) : (
              <ResellerInactiveCard canApply={profile.state.value.canApply} opened={profile.state.value.opened} />
            )}
          </main>
        </div>
      </div>
    )
  },
})
