import { defineComponent, ref, watch } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Crown, Menu } from 'lucide-vue-next'
import { AffiliatePanel } from '@/components/personal/AffiliatePanel'
import { ApiPanel } from '@/components/personal/ApiPanel'
import { GiftCardPanel } from '@/components/personal/GiftCardPanel'
import { OrdersPanel } from '@/components/personal/OrdersPanel'
import { OverviewPanel } from '@/components/personal/OverviewPanel'
import { ProfilePanel } from '@/components/personal/ProfilePanel'
import { ResellerEntry } from '@/components/personal/ResellerEntry'
import { SecurityPanel } from '@/components/personal/SecurityPanel'
import { WalletPanel } from '@/components/personal/WalletPanel'
import { Alert, Badge, Button, Mascot, Sheet, cn } from '@/components/ui'
import { usePersonalCenter } from '@/composables/personal/usePersonalCenter'

export default defineComponent({
  name: 'PersonalCenter',
  props: { section: { type: String, default: 'overview' } },
  setup(props) {
    const { t } = useI18n()
    const shell = usePersonalCenter(() => props.section)
    const menuOpen = ref(false)
    watch(() => shell.currentSection.value, () => (menuOpen.value = false))

    const navItems = (mobile: boolean, current: string) =>
      shell.navItems.value.map((item) => {
        const Icon = item.icon
        const active = current === item.key
        return (
          <RouterLink
            key={item.key}
            to={item.to}
            aria-current={active ? 'page' : undefined}
            onClick={mobile ? () => (menuOpen.value = false) : undefined}
            class={cn(
              'group relative flex items-center gap-2.5 rounded-zs px-4 py-2.5 text-sm font-bold transition',
              active ? 'bg-primary-soft text-primary-text' : 'text-muted hover:bg-surface-muted hover:text-fg',
            )}
          >
            <span class={cn('absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-r-full', active ? 'zs-gradient-bg' : 'bg-transparent')} />
            <Icon class={cn('size-4 shrink-0 transition-transform', active && 'scale-110')} />
            <span>{item.label}</span>
          </RouterLink>
        )
      })

    const panel = () => {
      switch (shell.currentSection.value) {
        case 'profile':
          return <ProfilePanel />
        case 'security':
          return <SecurityPanel />
        case 'orders':
          return <OrdersPanel />
        case 'wallet':
          return <WalletPanel />
        case 'affiliate':
          return <AffiliatePanel />
        case 'giftCard':
          return <GiftCardPanel />
        case 'api':
          return <ApiPanel />
        case 'reseller':
          return <ResellerEntry />
        default:
          return <OverviewPanel shell={shell} />
      }
    }

    return () => {
      const store = shell.store
      const level = store.currentLevel
      const levelIcon = shell.levelIconUrl(level)
      const current = shell.currentSection.value
      return (
        <div class="zs-page space-y-6 py-6 sm:py-8">
          <header class="zs-card relative overflow-hidden p-6 sm:p-8">
            <div class="zs-soft-bg pointer-events-none absolute inset-0" />
            <div class="pointer-events-none absolute -right-6 -bottom-10 hidden h-44 w-40 opacity-95 md:block">
              <Mascot mood="happy" />
            </div>
            <div class="relative flex flex-col gap-5 md:pr-40 lg:flex-row lg:items-center lg:justify-between">
              <div class="flex min-w-0 items-center gap-4">
                <div class="zs-gradient-bg zs-title flex size-16 shrink-0 items-center justify-center rounded-zs-lg text-3xl text-on-primary shadow-zs [text-shadow:0_1px_2px_rgba(0,0,0,.18)]">
                  {shell.displayInitial.value}
                </div>
                <div class="min-w-0">
                  <p class="text-xs font-bold tracking-[0.2em] text-primary-text">{t('personalCenter.title')}</p>
                  <h1 class="zs-title mt-1 truncate text-2xl text-fg sm:text-3xl">{store.displayName}</h1>
                  <p class="mt-1 truncate text-sm text-muted">{store.profile?.email || t('personalCenter.subtitle')}</p>
                </div>
              </div>
              <div class="flex flex-wrap items-center gap-2">
                <Badge tone={shell.emailVerified.value ? 'success' : 'warning'}>
                  {shell.emailVerified.value ? t('personalCenter.overview.emailVerified') : t('personalCenter.overview.emailUnverified')}
                </Badge>
                {level && (
                  <Badge tone="primary">
                    {levelIcon ? <img src={levelIcon} alt="" class="size-3.5 object-contain" /> : level.icon ? <span>{level.icon}</span> : <Crown class="size-3.5" />}
                    {shell.levelName(level)}
                  </Badge>
                )}
              </div>
            </div>
          </header>

          <div class="grid grid-cols-1 gap-6 lg:grid-cols-12">
            <aside class="lg:col-span-3">
              <Button variant="secondary" class="w-full justify-between lg:hidden" aria-label={t('zs.menu')} onClick={() => (menuOpen.value = true)}>
                <span class="inline-flex items-center gap-2"><Menu class="size-4" />{t('zs.menu')}</span>
                <span class="text-xs text-muted">{shell.navItems.value.find((item) => item.key === current)?.label}</span>
              </Button>
              <nav class="zs-card sticky top-24 hidden p-3 lg:block" aria-label={t('personalCenter.title')}>
                <div class="flex flex-col gap-1">{navItems(false, current)}</div>
              </nav>
            </aside>
            <section class="min-w-0 space-y-6 lg:col-span-9">
              {shell.loadError.value && <Alert tone="error">{shell.loadError.value}</Alert>}
              {panel()}
            </section>
          </div>
          <Sheet modelValue={menuOpen.value} onUpdate:modelValue={(value: boolean) => (menuOpen.value = value)} title={t('personalCenter.title')}>
            <nav class="space-y-1" aria-label={t('personalCenter.title')}>{navItems(true, current)}</nav>
          </Sheet>
        </div>
      )
    }
  },
})
