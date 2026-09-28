import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Badge, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminResellerLedgerEntry } from '@/api/types'
import { adminUrl } from '@/utils/adminBase'
import { formatDate } from '@/utils/format'
import { CreatedRange } from './components/CreatedRange'
import { RefreshButton } from './components/RefreshButton'
import { ResellerCell } from './components/ResellerCell'
import { RESELLER_LEDGER_STATUSES, RESELLER_LEDGER_TYPES, amountOrZero, ledgerStatusTone } from './resellerUtils'
import { useResellerLedgerEntries } from './useResellerFinanceLists'

export default defineComponent({
  name: 'ResellerLedgerEntriesView',
  setup() {
    const { t } = useI18n()
    const { filters, list, refreshing, refresh } = useResellerLedgerEntries()
    onMounted(() => void list.fetchData(1))

    const typeLabel = (v?: string) => (v && (RESELLER_LEDGER_TYPES as readonly string[]).includes(v) ? t(`admin.resellerLedgerEntries.types.${v}`) : v || '-')
    const statusLabel = (v?: string) =>
      v && (RESELLER_LEDGER_STATUSES as readonly string[]).includes(v) ? t(`admin.resellerLedgerEntries.status.${v}`) : v || '-'

    const columns = (): DataTableColumn<AdminResellerLedgerEntry>[] => [
      {
        key: 'id',
        title: t('admin.resellerLedgerEntries.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'reseller',
        title: t('admin.resellerLedgerEntries.table.reseller'),
        render: (r) => <ResellerCell profile={r.profile} resellerId={r.reseller_id} />,
      },
      {
        key: 'orderNo',
        title: t('admin.resellerLedgerEntries.table.orderNo'),
        class: 'min-w-[160px] break-all font-mono text-xs',
        render: (r) =>
          r.order?.order_no ? (
            <a href={adminUrl(`/orders?order_no=${encodeURIComponent(r.order.order_no)}`)} target="_blank" rel="noopener" class="text-accent hover:underline">
              {r.order.order_no}
            </a>
          ) : (
            '-'
          ),
      },
      {
        key: 'type',
        title: t('admin.resellerLedgerEntries.table.type'),
        class: 'text-xs whitespace-nowrap',
        render: (r) => typeLabel(r.type),
      },
      {
        key: 'amount',
        title: t('admin.resellerLedgerEntries.table.amount'),
        class: 'zs-num text-sm font-semibold',
        render: (r) => amountOrZero(r.amount),
      },
      {
        key: 'currency',
        title: t('admin.resellerLedgerEntries.table.currency'),
        class: 'font-mono text-xs text-muted',
        render: (r) => r.currency || '-',
      },
      {
        key: 'status',
        title: t('admin.resellerLedgerEntries.table.status'),
        render: (r) => (
          <Badge tone={ledgerStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'availableAt',
        title: t('admin.resellerLedgerEntries.table.availableAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.available_at) || '-',
      },
      {
        key: 'withdrawRequest',
        title: t('admin.resellerLedgerEntries.table.withdrawRequest'),
        class: 'font-mono text-xs',
        render: (r) => (r.withdraw_request_id ? `#${r.withdraw_request_id}` : '-'),
      },
      {
        key: 'createdAt',
        title: t('admin.resellerLedgerEntries.table.createdAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.resellerLedgerEntries.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={filters.keyword}
                  placeholder={t('admin.resellerLedgerEntries.filters.keyword')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input
                  v-model={filters.resellerId}
                  placeholder={t('admin.resellerLedgerEntries.filters.resellerId')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input v-model={filters.userId} placeholder={t('admin.resellerLedgerEntries.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.orderId} placeholder={t('admin.resellerLedgerEntries.filters.orderId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.orderNo} placeholder={t('admin.resellerLedgerEntries.filters.orderNo')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.type}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerLedgerEntries.filters.typeAll'),
                      value: '__all__',
                    },
                    ...RESELLER_LEDGER_TYPES.map((v) => ({
                      label: typeLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerLedgerEntries.filters.statusAll'),
                      value: '__all__',
                    },
                    ...RESELLER_LEDGER_STATUSES.map((v) => ({
                      label: statusLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <div class="hidden lg:block" />
                <CreatedRange
                  label={t('admin.resellerLedgerEntries.filters.createdRange')}
                  v-model:from={filters.createdFrom}
                  v-model:to={filters.createdTo}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => <RefreshButton loading={refreshing.value} onClick={refresh} />,
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.resellerLedgerEntries.empty')}
            minWidth="1120px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
