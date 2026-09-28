import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Banknote, Coins, ShieldCheck } from 'lucide-vue-next'
import { useResellerFinance } from '@/composables/reseller/useResellerFinance'
import { ResellerPageHeader, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { ResellerDonut } from '@/components/reseller/ResellerDonut'
import { Alert, Badge, Button, Card, StatCard } from '@/components/ui'
import { formatResellerConsoleAmount, resellerCurrencyColor } from '@/utils/reseller/console'
import { balanceStatusTone, getResellerBalanceStatusKey, getResellerFinanceStatusView, pickPrimaryResellerBalance } from '@/utils/reseller/finance'

export default defineComponent({
  name: 'ResellerFinance',
  setup() {
    const { t } = useI18n()
    const f = useResellerFinance()
    onMounted(() => void Promise.all([f.loadDashboard(), f.loadBalances()]))
    const balanceList = computed(() => (f.balances.value.length ? f.balances.value : f.dashboard.value?.balances || []))
    const primary = computed(() => pickPrimaryResellerBalance(balanceList.value))
    const statusView = computed(() => getResellerFinanceStatusView(f.dashboard.value?.profile))
    const segments = computed(() =>
      balanceList.value.map((b, i) => ({ value: Number(b.available_amount) || 0, color: resellerCurrencyColor(i), label: b.currency })).filter((s) => s.value > 0),
    )
    const balanceLabel = (s?: string) => {
      const key = getResellerBalanceStatusKey(s)
      return key ? t(`personalCenter.reseller.balanceStatus.${key}`) : s || '-'
    }
    const share = (value: string, total: number) => (total > 0 ? Math.max(0, Math.min(100, ((Number(value) || 0) / total) * 100)) : 0)

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.finance.title')} description={t('resellerConsole.finance.description')}>
          {{
            actions: () => (
              <Button size="sm" to="/reseller/withdraws">
                {t('resellerConsole.nav.withdraws')}
              </Button>
            ),
          }}
        </ResellerPageHeader>
        {f.dashboardLoading.value || f.balanceLoading.value ? (
          <ResellerPageState loading title={t('resellerConsole.common.loading')} />
        ) : (
          <>
            <div class="grid gap-4 sm:grid-cols-3">
              <StatCard label={t('personalCenter.reseller.primaryAvailable')} tone="gold">
                {{
                  default: () => <span class="text-xl">{primary.value ? formatResellerConsoleAmount(primary.value.available_amount, primary.value.currency) : '-'}</span>,
                  icon: () => <Banknote class="size-5" />,
                }}
              </StatCard>
              <StatCard label={t('personalCenter.reseller.currencyCount')} value={balanceList.value.length} tone="accent">
                {{ icon: () => <Coins class="size-5" /> }}
              </StatCard>
              <StatCard label={t('personalCenter.reseller.settlementStatus')} tone="success">
                {{
                  default: () => <Badge tone={statusView.value.badgeTone} size="md">{t(`personalCenter.reseller.${statusView.value.namespace}.${statusView.value.key}`)}</Badge>,
                  icon: () => <ShieldCheck class="size-5" />,
                }}
              </StatCard>
            </div>
            <Alert tone="info">{t('resellerConsole.finance.settlementHint')}</Alert>
            <div class="grid gap-5 lg:grid-cols-[1fr_1.3fr]">
              <Card>
                <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.finance.distribution')}</h3>
                {segments.value.length ? (
                  <ResellerDonut segments={segments.value} centerValue={balanceList.value.length} centerLabel={t('personalCenter.reseller.currencyCount')} />
                ) : (
                  <p class="py-8 text-center text-sm text-muted">{t('personalCenter.reseller.balanceEmpty')}</p>
                )}
              </Card>
              <Card>
                <h3 class="zs-title mb-4 text-lg text-fg">{t('personalCenter.reseller.balanceTitle')}</h3>
                {balanceList.value.length === 0 ? (
                  <p class="py-8 text-center text-sm text-muted">{t('personalCenter.reseller.balanceEmpty')}</p>
                ) : (
                  <div class="space-y-4">
                    {balanceList.value.map((b) => {
                      const total = (Number(b.available_amount) || 0) + (Number(b.locked_amount) || 0) + (Number(b.negative_amount) || 0)
                      return (
                        <div key={b.id} class="rounded-zs border border-line bg-surface-strong p-4">
                          <div class="flex items-center justify-between gap-2">
                            <span class="whitespace-nowrap zs-num text-lg font-bold text-fg">{b.currency}</span>
                            <Badge tone={balanceStatusTone(b.status)}>{balanceLabel(b.status)}</Badge>
                          </div>
                          <div class="mt-3 flex h-2.5 overflow-hidden rounded-full bg-surface-muted">
                            <span class="bg-success" style={{ width: `${share(b.available_amount, total)}%` }} />
                            <span class="bg-gold" style={{ width: `${share(b.locked_amount, total)}%` }} />
                            <span class="bg-danger" style={{ width: `${share(b.negative_amount, total)}%` }} />
                          </div>
                          <dl class="mt-3 grid grid-cols-3 gap-2 text-xs">
                            {[
                              ['availableAmount', b.available_amount, 'text-success-text'],
                              ['lockedAmount', b.locked_amount, 'text-warning-text'],
                              ['negativeAmount', b.negative_amount, 'text-danger-text'],
                            ].map(([key, value, cls]) => (
                              <div key={key}>
                                <dt class="text-muted">{t(`personalCenter.reseller.${key}`)}</dt>
                                <dd class={['zs-num mt-0.5 text-sm font-bold', cls]}>{formatResellerConsoleAmount(value)}</dd>
                              </div>
                            ))}
                          </dl>
                        </div>
                      )
                    })}
                  </div>
                )}
              </Card>
            </div>
          </>
        )}
      </div>
    )
  },
})
