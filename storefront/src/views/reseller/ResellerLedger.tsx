import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { Lock, RotateCw, SlidersHorizontal, TrendingUp, Undo2, Wallet } from 'lucide-vue-next'
import type { ResellerLedgerData } from '@/api/types'
import { useResellerLedger } from '@/composables/reseller/useResellerLedger'
import { ResellerPageHeader } from '@/components/reseller/ConsoleParts'
import { Badge, Button, Card, DataTable, Field, Input, Pagination, Select, columns, cn } from '@/components/ui'
import { formatResellerConsoleDate } from '@/utils/reseller/console'
import { RESELLER_LEDGER_STATUSES, RESELLER_LEDGER_TYPES } from '@/utils/reseller/constants'
import { getResellerLedgerStatusKey, getResellerLedgerTypeKey, ledgerAmountDisplay, ledgerDirection, ledgerStatusTone } from '@/utils/reseller/finance'

const typeIcon = (type?: string): LucideIcon => {
  switch (type) {
    case 'order_profit':
      return TrendingUp
    case 'refund_deduct':
      return Undo2
    case 'withdraw_lock':
      return Lock
    case 'withdraw_paid':
      return Wallet
    default:
      return SlidersHorizontal
  }
}

export default defineComponent({
  name: 'ResellerLedger',
  setup() {
    const { t } = useI18n()
    const l = useResellerLedger()
    onMounted(() => void l.reload())
    const typeLabel = (type?: string) => {
      const key = getResellerLedgerTypeKey(type)
      return key ? t(`personalCenter.reseller.ledgerType.${key}`) : type || '-'
    }
    const statusLabel = (s?: string) => {
      const key = getResellerLedgerStatusKey(s)
      return key ? t(`personalCenter.reseller.ledgerStatus.${key}`) : s || '-'
    }
    const typeOptions = computed(() => [{ value: 'all', label: t('resellerConsole.common.allTypes') }, ...RESELLER_LEDGER_TYPES.map((v) => ({ value: v, label: typeLabel(v) }))])
    const statusOptions = computed(() => [{ value: 'all', label: t('resellerConsole.orders.statusAll') }, ...RESELLER_LEDGER_STATUSES.map((v) => ({ value: v, label: statusLabel(v) }))])
    const ledgerColumns = columns<ResellerLedgerData>([
      {
        key: 'type',
        title: t('personalCenter.reseller.ledgerTable.type'),
        render: (row) => {
          const Icon = typeIcon(row.type)
          const dir = ledgerDirection(row)
          return (
            <span class="flex items-center gap-2">
              <span class={cn('flex size-8 items-center justify-center rounded-full', dir === 'income' ? 'bg-success-soft text-success-text' : dir === 'expense' ? 'bg-danger-soft text-danger-text' : 'bg-surface-muted text-muted')}>
                <Icon class="size-4" />
              </span>
              <span class="font-bold">{typeLabel(row.type)}</span>
            </span>
          )
        },
      },
      {
        key: 'amount',
        title: t('personalCenter.reseller.ledgerTable.amount'),
        align: 'right',
        render: (row) => {
          const dir = ledgerDirection(row)
          return <span class={cn('whitespace-nowrap zs-num font-bold', dir === 'income' ? 'text-success-text' : dir === 'expense' ? 'text-danger-text' : 'text-fg')}>{ledgerAmountDisplay(row)}</span>
        },
      },
      { key: 'status', title: t('personalCenter.reseller.ledgerTable.status'), render: (row) => <Badge tone={ledgerStatusTone(row.status)}>{statusLabel(row.status)}</Badge> },
      { key: 'order_id', title: t('resellerConsole.ledger.orderId'), render: (row) => <span class="whitespace-nowrap zs-num text-xs">{row.order_id || '-'}</span> },
      { key: 'available_at', title: t('personalCenter.reseller.ledgerTable.availableAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.available_at)}</span> },
      { key: 'created_at', title: t('personalCenter.reseller.ledgerTable.createdAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</span> },
    ])

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.ledger.title')} description={t('resellerConsole.ledger.description')}>
          {{
            actions: () => (
              <Button variant="secondary" size="sm" onClick={() => void l.reload()}>
                <RotateCw class="size-4" />
                {t('orders.filters.refresh')}
              </Button>
            ),
          }}
        </ResellerPageHeader>
        <Card>
          <form
            class="grid gap-3 md:grid-cols-[1fr_1fr_1fr_auto] md:items-end"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void l.reload()
            }}
          >
            <Field label={t('personalCenter.reseller.ledgerTable.type')}>
              <Select v-model={l.filters.type} options={typeOptions.value} size="sm" />
            </Field>
            <Field label={t('personalCenter.reseller.ledgerTable.status')}>
              <Select v-model={l.filters.status} options={statusOptions.value} size="sm" />
            </Field>
            <Field label={t('resellerConsole.ledger.orderId')}>
              <Input v-model={l.filters.order_id} size="sm" inputmode="numeric" placeholder={t('resellerConsole.ledger.orderId')} />
            </Field>
            <div class="flex gap-2">
              <Button type="submit" size="sm">
                {t('zsReseller.filter')}
              </Button>
              <Button size="sm" variant="secondary" disabled={!l.hasActiveFilter.value} onClick={l.resetFilters}>
                {t('resellerConsole.common.reset')}
              </Button>
            </div>
          </form>
        </Card>
        <Card>
          <DataTable
            columns={ledgerColumns}
            rows={l.ledgerEntries.value}
            loading={l.ledgerLoading.value}
            emptyText={l.hasActiveFilter.value ? t('resellerConsole.common.noFilterResult') : t('personalCenter.reseller.ledgerEmpty')}
          />
          <div class="mt-4">
            <Pagination page={l.ledgerPagination.page} totalPages={l.ledgerPagination.total_page} onChange={(pg: number) => void l.goPage(pg)} />
          </div>
        </Card>
      </div>
    )
  },
})
