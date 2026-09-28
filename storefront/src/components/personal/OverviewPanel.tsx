import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Crown, Percent, ShieldCheck, ShoppingBag, Star } from 'lucide-vue-next'
import type { PublicMemberLevel } from '@/api/types'
import { Badge, Button, Card, StatCard } from '@/components/ui'
import type { usePersonalCenter } from '@/composables/personal/usePersonalCenter'
import { discountNumber } from '@/utils/personal'
import { OrderRow } from './OrderRow'
import { DashedNote, ProgressBar, SkeletonRows } from './PanelParts'
import type { PropType } from 'vue'

type Shell = ReturnType<typeof usePersonalCenter>

/** Overview: stat tiles, member level progress, recent orders. */
export const OverviewPanel = defineComponent({
  name: 'OverviewPanel',
  props: { shell: { type: Object as PropType<Shell>, required: true } },
  setup(props) {
    const { t } = useI18n()

    const levelIcon = (level: PublicMemberLevel | null, fallback: string, size: string) => {
      const url = props.shell.levelIconUrl(level)
      if (url) return <img src={url} alt="" class={[size, 'object-contain']} />
      return <span>{level?.icon || fallback}</span>
    }

    const progressRow = (label: string, current: number, target: number, percent: number) => (
      <div>
        <div class="mb-1.5 flex items-center justify-between text-xs text-muted">
          <span class="font-bold">{label}</span>
          <span class="zs-num">
            {current.toFixed(2)} <span class="opacity-40">/</span> {target.toFixed(2)}
          </span>
        </div>
        <ProgressBar percent={percent} />
      </div>
    )

    return () => {
      const { store, levelName, discountLabel, emailVerified } = props.shell
      const next = store.nextLevel
      const progress = store.upgradeProgress
      return (
        <div class="space-y-6">
          <div class="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <StatCard label={t('personalCenter.tabs.orders')} value={store.loadingOrders ? '—' : store.ordersTotal} tone="accent">
              {{ icon: () => <ShoppingBag class="size-5" /> }}
            </StatCard>
            <StatCard label={t('personalCenter.memberLevel.currentLevel')} tone="secondary">
              {{
                default: () => <span class="zs-title text-base font-normal sm:text-xl">{levelName(store.currentLevel)}</span>,
                icon: () => <Crown class="size-5" />,
              }}
            </StatCard>
            <StatCard label={t('personalCenter.memberLevel.discountRate')} tone="gold">
              {{
                default: () => <span class="zs-title text-base font-normal sm:text-xl">{discountLabel(store.currentLevel)}</span>,
                icon: () => <Percent class="size-5" />,
              }}
            </StatCard>
            <StatCard label={t('personalCenter.overview.accountLabel')} tone="success">
              {{
                default: () => (
                  <Badge size="xs" tone={emailVerified.value ? 'success' : 'warning'}>
                    {emailVerified.value ? t('personalCenter.overview.emailVerified') : t('personalCenter.overview.emailUnverified')}
                  </Badge>
                ),
                icon: () => <ShieldCheck class="size-5" />,
              }}
            </StatCard>
          </div>

          {store.memberLevels.length > 0 && (
            <Card>
              <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <div class="flex items-center gap-3.5">
                  <div class="flex size-12 shrink-0 items-center justify-center rounded-zs bg-primary-soft text-2xl">
                    {levelIcon(store.currentLevel, '🌸', 'size-8')}
                  </div>
                  <div>
                    <p class="text-xs font-bold text-muted">{t('personalCenter.memberLevel.currentLevel')}</p>
                    <p class="zs-title mt-0.5 text-xl text-fg">{levelName(store.currentLevel)}</p>
                  </div>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <Badge tone="accent" size="md">
                    {t('personalCenter.memberLevel.discountRate')} {discountLabel(store.currentLevel)}
                  </Badge>
                  {!next && store.currentLevel && (
                    <Badge tone="success" size="md">
                      {t('personalCenter.memberLevel.highestLevel')}
                    </Badge>
                  )}
                </div>
              </div>
              {next && (
                <div class="zs-soft-bg mt-5 rounded-zs border border-line p-4">
                  <div class="flex items-center gap-3">
                    <div class="flex size-10 shrink-0 items-center justify-center rounded-zs-sm bg-surface-strong text-lg">
                      {next.icon ? levelIcon(next, '⭐', 'size-6') : <Star class="size-5 text-gold" />}
                    </div>
                    <div class="min-w-0">
                      <p class="text-xs font-bold text-muted">{t('personalCenter.memberLevel.nextLevel')}</p>
                      <div class="mt-0.5 flex items-center gap-2">
                        <span class="truncate text-sm font-bold text-fg">{levelName(next)}</span>
                        {discountNumber(next.discount_rate) && (
                          <span class="text-xs font-bold text-primary-text">
                            {t('personalCenter.memberLevel.discountOff', { n: discountNumber(next.discount_rate) })}
                          </span>
                        )}
                      </div>
                    </div>
                  </div>
                  {progress && (
                    <div class="mt-4 space-y-3">
                      {progress.rechargePercent !== null &&
                        progressRow(t('personalCenter.memberLevel.rechargeProgress'), progress.recharged, progress.rechargeThreshold, progress.rechargePercent)}
                      {progress.spendPercent !== null &&
                        progressRow(t('personalCenter.memberLevel.spendProgress'), progress.spent, progress.spendThreshold, progress.spendPercent)}
                    </div>
                  )}
                </div>
              )}
            </Card>
          )}

          <Card>
            <div class="mb-4 flex flex-wrap items-center justify-between gap-2">
              <h3 class="zs-title text-lg text-fg">{t('personalCenter.overview.recentOrdersTitle')}</h3>
              <RouterLink to="/me/orders" class="text-sm font-bold text-primary-text hover:underline">
                {t('personalCenter.overview.viewAllOrders')} →
              </RouterLink>
            </div>
            {store.loadingOrders ? (
              <SkeletonRows />
            ) : store.recentOrders.length === 0 ? (
              <DashedNote>
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <span>{t('personalCenter.overview.emptyOrders')}</span>
                  <Button size="sm" variant="soft" to="/products">
                    {t('orders.emptyAction')}
                  </Button>
                </div>
              </DashedNote>
            ) : (
              <div class="space-y-3">
                {store.recentOrders.map((o) => (
                  <OrderRow key={o.order_no} order={o} />
                ))}
              </div>
            )}
          </Card>
        </div>
      )
    }
  },
})
