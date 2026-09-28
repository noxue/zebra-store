import { computed, defineComponent, h, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue'
import { RouterLink, RouterView, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ChevronDown, ChevronsLeft, ChevronsRight, Home, LogOut, Menu, Moon, RefreshCw, Search, Sun } from 'lucide-vue-next'
import { useAdminAuthStore } from '@/stores/auth'
import { useAppStore } from '@/stores/app'
import { localeSelectOptions, setLocale, type AppLocale } from '@/i18n'
import { adminUrl } from '@/utils/adminBase'
import { Button, Select, Sheet, cn, type IconComponent } from '@/components/ui'
import { SystemUpdateDialog } from '@/components/SystemUpdateDialog'
import { ComplianceGuard } from '@/components/compliance/ComplianceGuard'
import { DASHBOARD_ITEM, NAV_GROUPS, isNavItemActive, resolveNav } from './nav'

const GROUP_KEY = 'admin_nav_group_expanded'
const COLLAPSED_KEY = 'admin_sidebar_collapsed'
const AUTO_COLLAPSE_WIDTH = 1280

const readGroups = (): Record<string, boolean> => {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(GROUP_KEY) || '{}')
    if (!parsed || typeof parsed !== 'object') return {}
    return Object.fromEntries(Object.entries(parsed).filter(([, v]) => typeof v === 'boolean')) as Record<string, boolean>
  } catch {
    return {}
  }
}

export default defineComponent({
  name: 'AdminLayout',
  setup() {
    const { t, locale } = useI18n()
    const route = useRoute()
    const auth = useAdminAuthStore()
    const app = useAppStore()
    const search = ref('')
    const expanded = ref<Record<string, boolean>>(readGroups())
    const mobileOpen = ref(false)
    const collapsed = ref(false)
    const userToggled = ref(false)
    const updateOpen = ref(false)

    const groups = computed(() => resolveNav(NAV_GROUPS, (k) => t(k), (p) => auth.hasPermission(p), search.value))
    const allPaths = computed(() => NAV_GROUPS.flatMap((g) => g.items.map((i) => i.to)))
    const showDashboard = computed(() => {
      const k = search.value.trim().toLowerCase()
      return !k || t(DASHBOARD_ITEM.labelKey).toLowerCase().includes(k)
    })
    const brandName = computed(() => app.siteName || t('admin.brand'))

    watch(expanded, (v) => localStorage.setItem(GROUP_KEY, JSON.stringify(v)), { deep: true })
    watch(
      () => route.path,
      () => (mobileOpen.value = false),
    )

    const isExpanded = (id: string) => !!search.value.trim() || expanded.value[id] !== false
    const toggleGroup = (id: string) => (expanded.value = { ...expanded.value, [id]: !isExpanded(id) })

    const toggleSidebar = () => {
      collapsed.value = !collapsed.value
      userToggled.value = true
      localStorage.setItem(COLLAPSED_KEY, String(collapsed.value))
    }
    const onResize = () => {
      if (!userToggled.value) collapsed.value = window.innerWidth < AUTO_COLLAPSE_WIDTH
    }
    const logout = () => {
      auth.logout()
      window.location.href = adminUrl('/login')
    }

    onMounted(() => {
      const saved = localStorage.getItem(COLLAPSED_KEY)
      if (saved !== null) {
        collapsed.value = saved === 'true'
        userToggled.value = true
      } else {
        collapsed.value = window.innerWidth < AUTO_COLLAPSE_WIDTH
      }
      window.addEventListener('resize', onResize)
      void app.loadConfig()
    })
    onBeforeUnmount(() => window.removeEventListener('resize', onResize))

    const navLink = (to: string, label: string, icon: IconComponent, compact: boolean, nested: boolean) => {
      const Icon = icon
      const active = isNavItemActive(to, route.path, allPaths.value)
      return (
        <RouterLink
          key={to}
          to={to}
          title={compact ? label : undefined}
          class={cn(
            'group relative flex items-center gap-2.5 rounded-[12px] text-sm transition-all',
            compact ? 'h-10 w-10 justify-center' : cn('h-9 px-3', nested && 'ml-4'),
            active ? 'bg-primary-soft font-semibold text-primary shadow-zs-sm' : 'text-fg/75 hover:bg-primary-soft/60 hover:text-primary',
          )}
        >
          {active && <span class="zs-gradient-bg absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-full" />}
          <Icon class="h-4 w-4 shrink-0" />
          {!compact && <span class="truncate">{label}</span>}
        </RouterLink>
      )
    }

    const renderNav = (compact: boolean) => (
      <nav class={cn('flex-1 space-y-1 overflow-y-auto pb-4', compact ? 'px-3' : 'px-4')}>
        {showDashboard.value && navLink(DASHBOARD_ITEM.to, t(DASHBOARD_ITEM.labelKey), DASHBOARD_ITEM.icon, compact, false)}
        {groups.value.map((g) => {
          const GroupIcon = g.icon
          if (compact) {
            return (
              <div key={g.id} class="space-y-1 border-t border-line/60 pt-1">
                {g.items.map((i) => navLink(i.to, `${g.label} / ${i.label}`, i.icon, true, false))}
              </div>
            )
          }
          const open = isExpanded(g.id)
          return (
            <div key={g.id} class="pt-1">
              <button
                type="button"
                onClick={() => toggleGroup(g.id)}
                class="flex h-9 w-full items-center gap-2.5 rounded-[12px] px-3 text-sm font-medium text-fg transition-colors hover:bg-primary-soft/50"
              >
                <GroupIcon class="h-4 w-4 text-secondary" />
                <span class="flex-1 truncate text-left">{g.label}</span>
                <ChevronDown class={cn('h-4 w-4 text-muted transition-transform', !open && '-rotate-90')} />
              </button>
              {open && <div class="mt-0.5 space-y-0.5">{g.items.map((i) => navLink(i.to, i.label, i.icon, false, true))}</div>}
            </div>
          )
        })}
      </nav>
    )

    const brand = (compact: boolean) => (
      <RouterLink to="/" class={cn('flex items-center gap-3 py-5', compact ? 'justify-center px-2' : 'px-5')}>
        <img src={app.siteLogo || '/favicon.svg'} alt="" class="h-10 w-10 shrink-0 rounded-[14px] object-cover shadow-zs-sm" />
        {!compact && (
          <div class="min-w-0">
            <p class="zs-display zs-gradient-text truncate text-base leading-tight">{brandName.value}</p>
            <p class="truncate text-[11px] text-muted">{t('admin.layout.controlRoom')}</p>
          </div>
        )}
      </RouterLink>
    )

    const sidebarBody = (compact: boolean) => (
      <div class="flex h-full flex-col">
        {brand(compact)}
        {!compact && (
          <div class="px-4 pb-3">
            <div class="relative">
              <Search class="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-muted" />
              <input
                value={search.value}
                onInput={(e: Event) => (search.value = (e.target as HTMLInputElement).value)}
                placeholder={t('admin.common.navSearch')}
                class="h-9 w-full rounded-full border border-line bg-surface-strong pl-9 pr-3 text-xs text-fg placeholder:text-muted/70 focus:border-primary focus:outline-none"
              />
            </div>
          </div>
        )}
        {renderNav(compact)}
        <div class={cn('border-t border-line py-3 text-[11px] text-muted', compact ? 'px-2 text-center' : 'px-5')}>
          {!compact && (
            <p>
              © {new Date().getFullYear()} {brandName.value}
              {app.appVersion && <span class="ml-1 text-primary">{app.appVersion}</span>}
            </p>
          )}
          <button
            type="button"
            class="mt-2 hidden rounded-full p-1.5 text-muted hover:bg-primary-soft hover:text-primary lg:inline-flex"
            title={collapsed.value ? t('admin.layout.expandSidebar') : t('admin.layout.collapseSidebar')}
            onClick={toggleSidebar}
          >
            {collapsed.value ? <ChevronsRight class="h-4 w-4" /> : <ChevronsLeft class="h-4 w-4" />}
          </button>
        </div>
      </div>
    )

    const localeOptions = computed(() => localeSelectOptions())

    return () => (
      <div class="flex min-h-screen">
        <aside
          class={cn(
            'zs-glass sticky top-0 hidden h-screen shrink-0 overflow-hidden border-y-0 border-l-0 transition-[width] duration-300 lg:block',
            collapsed.value ? 'w-[72px]' : 'w-64',
          )}
        >
          {sidebarBody(collapsed.value)}
        </aside>
        <Sheet modelValue={mobileOpen.value} onUpdate:modelValue={(v: boolean) => (mobileOpen.value = v)} side="left" width="w-72">
          {sidebarBody(false)}
        </Sheet>

        <div class="flex min-w-0 flex-1 flex-col">
          <header class="zs-glass sticky top-0 z-30 flex h-16 items-center gap-3 border-x-0 border-t-0 px-4 sm:px-6">
            <span class="lg:hidden">
              <Button variant="ghost" size="icon" onClick={() => (mobileOpen.value = true)}>
                <Menu class="h-5 w-5" />
              </Button>
            </span>
            <span class="hidden lg:block">
              <Button variant="ghost" size="icon" onClick={toggleSidebar} title={t('admin.layout.collapseSidebar')}>
                {collapsed.value ? <ChevronsRight class="h-4 w-4" /> : <ChevronsLeft class="h-4 w-4" />}
              </Button>
            </span>
            <p class="hidden truncate text-sm text-muted sm:block">{t('admin.layout.workspace')}</p>
            <div class="ml-auto flex items-center gap-2">
              <div class="w-32">
                <Select size="sm" modelValue={locale.value} options={localeOptions.value} onUpdate:modelValue={(v) => setLocale(String(v) as AppLocale)} />
              </div>
              <Button size="icon-sm" variant="outline" title={t('admin.updateCheck.button')} onClick={() => (updateOpen.value = true)}>
                <RefreshCw class="h-3.5 w-3.5" />
              </Button>
              <Button size="icon-sm" variant="outline" title={t('admin.common.toggleTheme')} onClick={() => app.toggleTheme()}>
                {app.theme === 'dark' ? <Sun class="h-3.5 w-3.5" /> : <Moon class="h-3.5 w-3.5" />}
              </Button>
              {app.siteUrl && (
                <a
                  href={app.siteUrl}
                  target="_blank"
                  rel="noopener"
                  title={t('admin.layout.homePage')}
                  class="inline-flex h-7 w-7 items-center justify-center rounded-full border border-line-strong bg-surface-strong text-fg transition-colors hover:border-primary hover:text-primary"
                >
                  <Home class="h-3.5 w-3.5" />
                </a>
              )}
              <Button size="sm" variant="outline" onClick={logout}>
                <LogOut class="h-3.5 w-3.5" />
                <span class="hidden sm:inline">{t('admin.common.logout')}</span>
              </Button>
            </div>
          </header>
          <main class="min-w-0 flex-1 px-4 py-6 sm:px-6 lg:px-8">
            <div class="mx-auto w-full max-w-[1600px]">
              <RouterView>
                {{
                  default: ({ Component: View }: { Component: Component | undefined }) =>
                    View ? (route.meta.compliance ? <ComplianceGuard>{{ default: () => h(View, { key: route.path }) }}</ComplianceGuard> : h(View, { key: route.path })) : null,
                }}
              </RouterView>
            </div>
          </main>
        </div>
        <SystemUpdateDialog open={updateOpen.value} onUpdate:open={(v: boolean) => (updateOpen.value = v)} />
      </div>
    )
  },
})
