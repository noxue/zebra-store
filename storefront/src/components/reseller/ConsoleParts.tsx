import { defineComponent, type PropType } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { CheckCircle2 } from 'lucide-vue-next'
import { Alert, Button, Card, Kitty, Mascot, PetalLoader, type AlertTone } from '@/components/ui'

/** Page title row used by every console page. */
export const ResellerPageHeader = defineComponent({
  name: 'ResellerPageHeader',
  props: {
    title: { type: String, required: true },
    description: { type: String, default: '' },
  },
  setup(props, { slots }) {
    return () => (
      <div class="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div class="min-w-0">
          <h1 class="zs-title flex items-center gap-2 text-2xl">
            <span class="zs-gradient-text">{props.title}</span>
            <span class="zs-sparkle text-sm" aria-hidden="true">
              ✦
            </span>
          </h1>
          {props.description && <p class="mt-1 text-sm text-muted">{props.description}</p>}
        </div>
        {slots.actions && <div class="flex flex-wrap items-center gap-2">{slots.actions()}</div>}
      </div>
    )
  },
})

/** Loading or empty placeholder inside a card. */
export const ResellerPageState = defineComponent({
  name: 'ResellerPageState',
  props: {
    loading: Boolean,
    title: { type: String, default: '' },
    description: { type: String, default: '' },
  },
  setup(props, { slots }) {
    return () => (
      <Card>
        {props.loading ? (
          <div class="py-10">
            <PetalLoader label={props.title} />
          </div>
        ) : (
          <div class="flex flex-col items-center gap-2 py-8 text-center">
            <div class="h-20 w-24">
              <Kitty mood="sleepy" />
            </div>
            <div class="zs-title text-base text-fg">{props.title}</div>
            {props.description && <p class="max-w-md text-sm text-muted">{props.description}</p>}
            {slots.default?.()}
          </div>
        )}
      </Card>
    )
  },
})

/** Alert from a composable `{ tone, message }` ref value. */
export const ResellerAlert = defineComponent({
  name: 'ResellerAlert',
  props: { alert: { type: Object as PropType<{ tone: AlertTone; message: string } | null>, default: null } },
  setup(props) {
    return () => (props.alert ? <Alert tone={props.alert.tone}>{props.alert.message}</Alert> : null)
  },
})

/** Friendly "not active yet" card with mascot, benefits and apply CTA. */
export const ResellerInactiveCard = defineComponent({
  name: 'ResellerInactiveCard',
  props: {
    canApply: Boolean,
    opened: Boolean,
  },
  setup(props) {
    const { t } = useI18n()
    return () => (
      <Card padding="lg">
        <div class="grid items-center gap-8 md:grid-cols-[1fr_220px]">
          <div>
            <span class="inline-flex rounded-full bg-primary-soft px-3 py-1 text-xs font-bold text-primary-text">{t('zsReseller.consoleBadge')}</span>
            <h2 class="zs-title mt-3 text-2xl text-fg">
              {props.opened || props.canApply ? t('resellerConsole.dashboard.inactiveTitle') : t('zsReseller.notOpenedTitle')}
            </h2>
            <p class="mt-2 text-sm leading-relaxed text-muted">
              {props.opened || props.canApply ? t('resellerConsole.dashboard.inactiveDescription') : t('zsReseller.notOpenedDescription')}
            </p>
            <ul class="mt-5 space-y-2.5">
              {[1, 2, 3].map((i) => (
                <li key={i} class="flex items-start gap-2.5 text-sm text-fg">
                  <CheckCircle2 class="mt-0.5 size-5 shrink-0 text-success" />
                  <span>{t(`resellerConsole.dashboard.benefit${i}`)}</span>
                </li>
              ))}
            </ul>
            <div class="mt-6 flex flex-wrap gap-3">
              <Button to="/reseller/apply">{props.canApply ? t('zsReseller.goApply') : t('resellerConsole.nav.apply')}</Button>
              <Button to="/me" variant="secondary">
                {t('resellerConsole.nav.backStore')}
              </Button>
            </div>
          </div>
          <div class="mx-auto hidden h-56 w-48 md:block">
            <Mascot mood={props.canApply ? 'happy' : 'wink'} builtin />
          </div>
        </div>
        <p class="mt-6 rounded-zs bg-surface-muted px-4 py-3 text-center text-xs text-muted">{t('zsReseller.inactiveHint')}</p>
      </Card>
    )
  },
})

/** Card-style link used for quick actions and setup checklist. */
export const ResellerTileLink = defineComponent({
  name: 'ResellerTileLink',
  props: { to: { type: String, required: true } },
  setup(props, { slots }) {
    return () => (
      <RouterLink to={props.to} class="zs-card zs-card-hover group block p-4">
        {slots.default?.()}
      </RouterLink>
    )
  },
})
