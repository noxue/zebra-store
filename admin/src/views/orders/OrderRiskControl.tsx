import { defineComponent, onMounted, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { Gauge, Network, Save, ShieldCheck, Sparkles, UserRound } from 'lucide-vue-next'
import { Badge, Button, FormField, Input, PageHeader, Switch, Textarea, cn } from '@/components/ui'
import type { RateLimitForm } from './riskControlUtils'
import { useOrderRiskControl } from './useOrderRiskControl'

const P = 'admin.settings.orderRiskControl'

export default defineComponent({
  name: 'OrderRiskControlView',
  setup() {
    const { t } = useI18n()
    const r = useOrderRiskControl()
    onMounted(() => void r.load())

    const numberField = (label: string, hint: string | undefined, model: { get: () => number | ''; set: (v: number | '') => void }, min: number, max: number) => (
      <FormField label={label} hint={hint}>
        <Input type="number" min={min} max={max} modelValue={model.get()} onUpdate:modelValue={(v) => model.set(v === '' ? '' : Number(v))} />
      </FormField>
    )

    const rateLimitBlock = (rl: RateLimitForm, title: string, hint: string, icon: VNodeChild) => (
      <div class="rounded-zs border border-line bg-surface-strong p-4">
        <div class="mb-4 flex items-center justify-between gap-4">
          <div class="flex items-start gap-2">
            {icon}
            <div>
              <div class="text-sm font-medium text-fg">{title}</div>
              <p class="mt-0.5 text-xs text-muted">{hint}</p>
            </div>
          </div>
          <Switch v-model={rl.enabled} />
        </div>
        {rl.enabled && (
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
            {numberField(t(`${P}.rateLimit.windowSeconds`), undefined, { get: () => rl.window_seconds, set: (v) => (rl.window_seconds = v) }, 10, 3600)}
            {numberField(t(`${P}.rateLimit.maxRequests`), undefined, { get: () => rl.max_requests, set: (v) => (rl.max_requests = v) }, 1, 100)}
            {numberField(t(`${P}.rateLimit.blockSeconds`), undefined, { get: () => rl.block_seconds, set: (v) => (rl.block_seconds = v) }, 0, 86400)}
          </div>
        )}
      </div>
    )

    const iconBadge = (Icon: typeof ShieldCheck, cls: string) => (
      <div class={cn('flex h-10 w-10 shrink-0 items-center justify-center rounded-zs shadow-zs-sm', cls)}>
        <Icon class="h-5 w-5" />
      </div>
    )

    return () => {
      const f = r.form
      const g = f.guest
      const m = f.member
      return (
        <div class="space-y-6">
          <PageHeader title={t(`${P}.title`)} subtitle={t(`${P}.subtitle`)}>
            {{
              actions: () => (
                <Button variant="primary" loading={r.saving.value} disabled={r.loading.value} onClick={r.save}>
                  <Save class="h-4 w-4" />
                  {r.saving.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
                </Button>
              ),
            }}
          </PageHeader>

          <div class={cn('space-y-6 transition-opacity', r.loading.value && 'pointer-events-none opacity-60')}>
            {/* master switch */}
            <section class="zs-glass overflow-hidden rounded-zs-lg shadow-zs">
              <div class="flex flex-col gap-5 p-5 sm:flex-row sm:items-center sm:justify-between">
                <div class="flex items-start gap-3">
                  {iconBadge(ShieldCheck, 'zs-gradient-bg text-on-primary')}
                  <div>
                    <h2 class="zs-display text-base text-fg">{t(`${P}.master.title`)}</h2>
                    <p class="mt-1 max-w-2xl text-xs leading-5 text-muted">{t(`${P}.master.subtitle`)}</p>
                  </div>
                </div>
                <div class="flex items-center gap-3 rounded-full border border-line bg-surface-strong px-4 py-2">
                  <span class={cn('text-xs font-medium', f.enabled ? 'text-success-text' : 'text-muted')}>
                    {f.enabled ? t(`${P}.statusEnabled`) : t(`${P}.statusDisabled`)}
                  </span>
                  <Switch v-model={f.enabled} />
                </div>
              </div>
            </section>

            {f.enabled && (
              <>
                {/* guest policy */}
                <section class="zs-glass overflow-hidden rounded-zs-lg border-warning/40 shadow-zs">
                  <div class="border-b border-warning/30 bg-warning-soft/70 p-5">
                    <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
                      <div class="flex items-start gap-3">
                        {iconBadge(Network, 'bg-warning text-on-primary')}
                        <div>
                          <div class="flex flex-wrap items-center gap-2">
                            <h3 class="zs-display text-base text-fg">{t(`${P}.guest.title`)}</h3>
                            <Badge tone="warning">{t(`${P}.guest.priority`)}</Badge>
                          </div>
                          <p class="mt-1 max-w-2xl text-xs leading-5 text-muted">{t(`${P}.guest.subtitle`)}</p>
                        </div>
                      </div>
                      <div class="flex items-center gap-3">
                        <Button size="sm" onClick={r.applyRecommendedGuestPolicy}>
                          <Sparkles class="h-3.5 w-3.5" />
                          {t(`${P}.guest.applyRecommended`)}
                        </Button>
                        <Switch v-model={g.enabled} />
                      </div>
                    </div>
                  </div>
                  {g.enabled && (
                    <div class="space-y-5 p-5">
                      <div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
                        {numberField(t(`${P}.guest.maxPendingPerIP`), t(`${P}.guest.maxPendingPerIPHint`), { get: () => g.max_pending_orders_per_ip, set: (v) => (g.max_pending_orders_per_ip = v) }, 0, 100)}
                        {numberField(
                          t(`${P}.guest.maxQuantityPerProduct`),
                          t(`${P}.guest.maxQuantityPerProductHint`),
                          { get: () => g.max_quantity_per_product_per_order, set: (v) => (g.max_quantity_per_product_per_order = v) },
                          0,
                          100000,
                        )}
                        {numberField(
                          t(`${P}.guest.maxPendingQuantityPerIPProduct`),
                          t(`${P}.guest.maxPendingQuantityPerIPProductHint`),
                          { get: () => g.max_pending_quantity_per_ip_product, set: (v) => (g.max_pending_quantity_per_ip_product = v) },
                          0,
                          100000,
                        )}
                        {numberField(
                          t(`${P}.guest.paymentExpireMinutes`),
                          t(`${P}.guest.paymentExpireMinutesHint`),
                          { get: () => g.payment_expire_minutes, set: (v) => (g.payment_expire_minutes = v) },
                          0,
                          10080,
                        )}
                      </div>
                      <div class="rounded-zs border border-dashed border-line-strong bg-surface-muted/50 px-4 py-3 text-xs leading-5 text-muted">{r.guestSummary.value}</div>
                      {rateLimitBlock(g.rate_limit, t(`${P}.guest.rateLimitTitle`), t(`${P}.guest.rateLimitHint`), <Gauge class="mt-0.5 h-4 w-4 text-warning-text" />)}
                    </div>
                  )}
                </section>

                {/* member policy */}
                <section class="zs-glass overflow-hidden rounded-zs-lg shadow-zs">
                  <div class="border-b border-line bg-accent-soft/60 p-5">
                    <div class="flex items-start justify-between gap-4">
                      <div class="flex items-start gap-3">
                        {iconBadge(UserRound, 'bg-accent text-on-primary')}
                        <div>
                          <h3 class="zs-display text-base text-fg">{t(`${P}.member.title`)}</h3>
                          <p class="mt-1 max-w-2xl text-xs leading-5 text-muted">{t(`${P}.member.subtitle`)}</p>
                        </div>
                      </div>
                      <Switch v-model={m.enabled} />
                    </div>
                  </div>
                  {m.enabled && (
                    <div class="space-y-5 p-5">
                      <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
                        {numberField(t(`${P}.member.maxPendingPerUser`), t(`${P}.member.maxPendingPerUserHint`), { get: () => m.max_pending_orders_per_user, set: (v) => (m.max_pending_orders_per_user = v) }, 0, 100)}
                        {numberField(t(`${P}.member.maxPendingPerIP`), t(`${P}.member.maxPendingPerIPHint`), { get: () => m.max_pending_orders_per_ip, set: (v) => (m.max_pending_orders_per_ip = v) }, 0, 100)}
                        {numberField(
                          t(`${P}.member.maxQuantityPerProduct`),
                          t(`${P}.member.maxQuantityPerProductHint`),
                          { get: () => m.max_quantity_per_product_per_order, set: (v) => (m.max_quantity_per_product_per_order = v) },
                          0,
                          100000,
                        )}
                      </div>
                      {rateLimitBlock(m.rate_limit, t(`${P}.member.rateLimitTitle`), t(`${P}.member.rateLimitHint`), <Gauge class="mt-0.5 h-4 w-4 text-info-text" />)}
                    </div>
                  )}
                </section>

                {/* ip blacklist */}
                <section class="zs-glass rounded-zs-lg p-5 shadow-zs">
                  <div class="mb-4">
                    <h3 class="zs-display text-sm text-fg">{t(`${P}.ipBlacklist.title`)}</h3>
                    <p class="mt-1 text-xs leading-5 text-muted">{t(`${P}.ipBlacklist.subtitle`)}</p>
                  </div>
                  <Textarea v-model={f.common.ip_blacklist_text} rows={6} mono placeholder={t(`${P}.ipBlacklist.placeholder`)} />
                </section>
              </>
            )}
          </div>
        </div>
      )
    }
  },
})
