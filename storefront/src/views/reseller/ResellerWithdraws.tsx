import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Send } from 'lucide-vue-next'
import type { ResellerWithdrawData } from '@/api/types'
import { useResellerWithdraws } from '@/composables/reseller/useResellerWithdraws'
import { ResellerAlert, ResellerPageHeader } from '@/components/reseller/ConsoleParts'
import { Alert, Badge, Button, Card, DataTable, Field, Input, Pagination, Select, columns } from '@/components/ui'
import { formatResellerConsoleAmount, formatResellerConsoleDate } from '@/utils/reseller/console'
import { getResellerWithdrawStatusKey, withdrawStatusTone } from '@/utils/reseller/finance'

export default defineComponent({
  name: 'ResellerWithdraws',
  setup() {
    const { t } = useI18n()
    const w = useResellerWithdraws()
    const k = (key: string) => t(`personalCenter.reseller.${key}`)
    onMounted(() => void w.load())
    const statusLabel = (s?: string) => {
      const key = getResellerWithdrawStatusKey(s)
      return key ? k(`withdrawStatus.${key}`) : s || '-'
    }
    const currencyOptions = computed(() => w.currencies.value.map((c) => ({ value: c, label: c })))
    const withdrawColumns = columns<ResellerWithdrawData>([
      { key: 'amount', title: k('withdrawTable.amount'), render: (row) => <span class="whitespace-nowrap zs-num font-bold">{formatResellerConsoleAmount(row.amount, row.currency)}</span> },
      {
        key: 'channel',
        title: k('withdrawTable.channel'),
        render: (row) => (
          <div>
            <div class="font-bold">{row.channel}</div>
            <div class="break-all text-xs text-muted">{row.account}</div>
          </div>
        ),
      },
      {
        key: 'status',
        title: k('withdrawTable.status'),
        render: (row) => (
          <div class="space-y-1">
            <Badge tone={withdrawStatusTone(row.status)}>{statusLabel(row.status)}</Badge>
            {row.reject_reason && <div class="text-xs text-danger-text">{row.reject_reason}</div>}
          </div>
        ),
      },
      { key: 'created_at', title: k('withdrawTable.createdAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</span> },
      { key: 'processed_at', title: k('withdrawTable.processedAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.processed_at)}</span> },
    ])

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.withdraws.title')} description={t('resellerConsole.withdraws.description')} />
        <ResellerAlert alert={w.alert.value} />
        <div class="grid gap-5 lg:grid-cols-[380px_minmax(0,1fr)]">
          <Card>
            <h3 class="zs-title text-lg text-fg">{k('withdrawTitle')}</h3>
            <p class="mb-4 mt-1 text-xs text-muted">{k('withdrawSubtitle')}</p>
            {!w.withdrawEnabled.value && !w.dashboardLoading.value && (
              <div class="mb-4">
                <Alert tone="warning">{w.disabledReasonText.value}</Alert>
              </div>
            )}
            <form
              class="space-y-4"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void w.submit()
              }}
            >
              <Field label={k('withdrawCurrencyLabel')} hint={w.currencies.value.length ? '' : t('resellerConsole.withdraws.noCurrency')}>
                {w.currencies.value.length ? (
                  <Select v-model={w.form.currency} options={currencyOptions.value} />
                ) : (
                  <Input v-model={w.form.currency} placeholder={t('resellerConsole.common.currencyPlaceholder')} />
                )}
              </Field>
              <Field
                label={k('withdrawAmountLabel')}
                error={w.amountExceeded.value ? t('resellerConsole.withdraws.exceedAvailable') : ''}
                hint={w.selectedAvailable.value !== null ? `${t('resellerConsole.withdraws.available')}: ${formatResellerConsoleAmount(w.selectedAvailable.value, w.form.currency)}` : ''}
              >
                <Input v-model={w.form.amount} inputmode="decimal" placeholder={k('withdrawAmountPlaceholder')} invalid={w.amountExceeded.value} />
              </Field>
              <Field label={k('withdrawChannelLabel')}>
                <Input v-model={w.form.channel} placeholder={k('withdrawChannelPlaceholder')} />
              </Field>
              <Field label={k('withdrawAccountLabel')}>
                <Input v-model={w.form.account} placeholder={k('withdrawAccountPlaceholder')} />
              </Field>
              <Button type="submit" block loading={w.submittingWithdraw.value} disabled={!w.withdrawEnabled.value || w.amountExceeded.value}>
                <Send class="size-4" />
                {w.submittingWithdraw.value ? k('withdrawing') : k('withdrawSubmit')}
              </Button>
            </form>
          </Card>
          <Card>
            <h3 class="zs-title mb-4 text-lg text-fg">{k('withdrawRecordTitle')}</h3>
            <DataTable columns={withdrawColumns} rows={w.withdraws.value} loading={w.withdrawsLoading.value} emptyText={k('withdrawEmpty')} />
            <div class="mt-4">
              <Pagination page={w.withdrawsPagination.page} totalPages={w.withdrawsPagination.total_page} onChange={(pg: number) => void w.loadWithdraws({ page: pg })} />
            </div>
          </Card>
        </div>
      </div>
    )
  },
})
