import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Banknote, CreditCard, Layers, ReceiptText, RefreshCw, Wallet } from 'lucide-vue-next'
import type { WalletTransactionData } from '@/api/types'
import { Badge, Button, Card, DataTable, Field, Input, Pagination, Select, StatCard, columns } from '@/components/ui'
import { useLocalized } from '@/composables/useLocalized'
import { useWalletPanel } from '@/composables/personal/useWalletPanel'
import { formatDateTime } from '@/utils/format'
import { signedAmount } from '@/utils/personal'
import { DashedNote, PanelAlertBox, PanelHeading } from './PanelParts'

/** Wallet: balance tiles, recharge form, transactions table. */
export const WalletPanel = defineComponent({
  name: 'WalletPanel',
  setup() {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    const w = useWalletPanel()

    const txColumns = () =>
      columns<WalletTransactionData>([
        { key: 'created_at', title: t('personalCenter.wallet.table.createdAt'), render: (r) => <span class="text-xs text-muted">{formatDateTime(r.created_at)}</span> },
        { key: 'type', title: t('personalCenter.wallet.table.type'), render: (r) => w.typeLabel(r.type) },
        {
          key: 'direction',
          title: t('personalCenter.wallet.table.direction'),
          render: (r) => <Badge tone={r.direction === 'in' ? 'success' : r.direction === 'out' ? 'danger' : 'warning'}>{w.directionLabel(r.direction)}</Badge>,
        },
        {
          key: 'amount',
          title: t('personalCenter.wallet.table.amount'),
          align: 'right',
          render: (r) => <span class={['zs-num font-bold', r.direction === 'in' ? 'text-success-text' : 'text-danger-text']}>{signedAmount(r.direction, formatPrice(r.amount))}</span>,
        },
        { key: 'balance_after', title: t('personalCenter.wallet.table.balanceAfter'), align: 'right', render: (r) => <span class="zs-num">{formatPrice(r.balance_after)}</span> },
        { key: 'remark', title: t('personalCenter.wallet.table.remark'), render: (r) => <span class="text-xs text-muted">{r.remark || '-'}</span> },
      ])

    return () => {
      const pag = w.transactions.pagination
      return (
        <div class="space-y-5">
          <Card>
            <PanelHeading title={t('personalCenter.wallet.title')} description={t('personalCenter.wallet.subtitle')} icon={Wallet}>
              {{ actions: () => <Badge tone="accent">{t('personalCenter.tabs.wallet')}</Badge> }}
            </PanelHeading>
            <PanelAlertBox alert={w.alert.value} />
            <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
              <div class="zs-gradient-bg relative overflow-hidden rounded-zs-lg p-5 text-on-primary shadow-zs">
                <div class="text-xs font-bold opacity-85">{t('personalCenter.wallet.balanceLabel')}</div>
                <div class="zs-num mt-2 text-3xl font-bold [text-shadow:0_1px_2px_rgba(0,0,0,.18)]">{w.balanceDisplay.value}</div>
                <Banknote class="absolute -right-3 -bottom-3 size-20 opacity-20" />
              </div>
              <StatCard label={t('personalCenter.wallet.transactionsLabel')} value={pag.total} tone="accent">
                {{ icon: () => <ReceiptText class="size-5" /> }}
              </StatCard>
              <StatCard label={t('personalCenter.wallet.currentPageLabel')} value={t('orders.pageInfo', { page: pag.page, total: Math.max(pag.total_page, 1) })} tone="secondary">
                {{ icon: () => <Layers class="size-5" /> }}
              </StatCard>
            </div>
          </Card>

          <Card>
            <PanelHeading title={t('personalCenter.wallet.rechargeTitle')} description={t('personalCenter.wallet.rechargeSubtitle')} icon={CreditCard} />
            <form
              class="grid grid-cols-1 gap-4 md:grid-cols-[1fr_1fr_1.6fr_auto] md:items-end"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void w.submitRecharge()
              }}
            >
              <Field label={t('personalCenter.wallet.amountLabel')}>
                <Input v-model={w.form.amount} inputmode="decimal" placeholder={t('personalCenter.wallet.amountPlaceholder')} />
              </Field>
              <Field label={t('personalCenter.wallet.channelLabel')}>
                <Select
                  modelValue={w.form.channelId || ''}
                  options={w.channelOptions.value}
                  placeholder={t('personalCenter.wallet.channelPlaceholder')}
                  disabled={!w.hasChannels.value || w.channelLoading.value || w.recharging.value}
                  onUpdate:modelValue={(v: string | number) => {
                    w.form.channelId = Number(v) || 0
                  }}
                />
              </Field>
              <Field label={t('personalCenter.wallet.remarkLabel')}>
                <Input v-model={w.form.remark} placeholder={t('personalCenter.wallet.remarkPlaceholder')} />
              </Field>
              <Button type="submit" loading={w.recharging.value} disabled={w.channelLoading.value || !w.hasChannels.value}>
                {w.recharging.value ? t('personalCenter.wallet.recharging') : t('personalCenter.wallet.rechargeSubmit')}
              </Button>
            </form>
            {w.isSurcharge.value && (
              <div class="mt-4 grid grid-cols-1 gap-3 text-sm md:grid-cols-3">
                {[
                  [t('payment.feeRateLabel'), w.feeRateDisplay.value],
                  [t('payment.fixedFeeLabel'), w.fixedFeeDisplay.value],
                  [t('payment.feeAmountLabel'), w.feeAmountDisplay.value],
                ].map(([label, value]) => (
                  <div key={label} class="rounded-zs border border-warning/40 bg-warning-soft p-4">
                    <div class="text-xs font-bold text-warning-text">{label}</div>
                    <div class="zs-num mt-1 font-bold text-fg">{value}</div>
                  </div>
                ))}
              </div>
            )}
            {w.amountHint.value && <p class="mt-3 text-xs text-warning-text">{w.amountHint.value}</p>}
            {w.channelLoading.value ? (
              <p class="mt-3 text-xs text-muted">{t('common.loading')}</p>
            ) : !w.hasChannels.value ? (
              <p class="mt-3 text-xs text-warning-text">{t('payment.channelEmpty')}</p>
            ) : null}
          </Card>

          <Card>
            <PanelHeading title={t('personalCenter.wallet.detailTitle')} icon={ReceiptText}>
              {{
                actions: () => (
                  <Button size="sm" variant="secondary" onClick={() => void w.refresh()}>
                    <RefreshCw class="size-4" />
                    {t('orders.filters.refresh')}
                  </Button>
                ),
              }}
            </PanelHeading>
            {!w.transactions.loading.value && w.transactions.rows.value.length === 0 ? (
              <DashedNote>{t('personalCenter.wallet.empty')}</DashedNote>
            ) : (
              <DataTable columns={txColumns()} rows={w.transactions.rows.value} loading={w.transactions.loading.value && w.transactions.rows.value.length === 0} />
            )}
            <div class="mt-4">
              <Pagination page={pag.page} totalPages={pag.total_page} onChange={(pg: number) => void w.transactions.load(pg)} />
            </div>
          </Card>
        </div>
      )
    }
  },
})
