import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { Badge, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, cn, type DataTableColumn } from '@/components/ui'
import type { AdminResellerBalanceAccount } from '@/api/types'
import { formatDate } from '@/utils/format'
import { RefreshButton } from './components/RefreshButton'
import { ResellerCell } from './components/ResellerCell'
import { RESELLER_BALANCE_STATUSES, amountOrZero, balanceStatusTone, isNegativeBalance } from './resellerUtils'
import { useResellerBalanceAccounts } from './useResellerFinanceLists'

export default defineComponent({
  name: 'ResellerBalanceAccountsView',
  setup() {
    const { t } = useI18n()
    const { filters, list, refreshing, refresh } = useResellerBalanceAccounts()
    onMounted(() => void list.fetchData(1))

    const statusLabel = (v?: string) =>
      v && (RESELLER_BALANCE_STATUSES as readonly string[]).includes(v) ? t(`admin.resellerBalanceAccounts.status.${v}`) : v || '-'

    const columns = (): DataTableColumn<AdminResellerBalanceAccount>[] => [
      {
        key: 'id',
        title: t('admin.resellerBalanceAccounts.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'reseller',
        title: t('admin.resellerBalanceAccounts.table.reseller'),
        render: (r) => <ResellerCell profile={r.profile} resellerId={r.reseller_id} />,
      },
      {
        key: 'currency',
        title: t('admin.resellerBalanceAccounts.table.currency'),
        class: 'font-mono text-xs text-muted',
        render: (r) => r.currency || '-',
      },
      {
        key: 'available',
        title: t('admin.resellerBalanceAccounts.table.available'),
        class: 'zs-num text-sm font-semibold',
        render: (r) => amountOrZero(r.available_amount_cache),
      },
      {
        key: 'locked',
        title: t('admin.resellerBalanceAccounts.table.locked'),
        class: 'zs-num text-sm',
        render: (r) => amountOrZero(r.locked_amount_cache),
      },
      {
        key: 'negative',
        title: t('admin.resellerBalanceAccounts.table.negative'),
        render: (r) => (
          <span class={cn('zs-num text-sm', isNegativeBalance(r.negative_amount_cache) ? 'font-semibold text-danger-text' : 'text-fg')}>
            {amountOrZero(r.negative_amount_cache)}
          </span>
        ),
      },
      {
        key: 'status',
        title: t('admin.resellerBalanceAccounts.table.status'),
        render: (r) => (
          <Badge tone={balanceStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'lastLedger',
        title: t('admin.resellerBalanceAccounts.table.lastLedger'),
        class: 'font-mono text-xs',
        render: (r) => (r.last_ledger_entry_id ? `#${r.last_ledger_entry_id}` : '-'),
      },
      {
        key: 'updatedAt',
        title: t('admin.resellerBalanceAccounts.table.updatedAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.updated_at),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.resellerBalanceAccounts.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={filters.keyword}
                  placeholder={t('admin.resellerBalanceAccounts.filters.keyword')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input
                  v-model={filters.resellerId}
                  placeholder={t('admin.resellerBalanceAccounts.filters.resellerId')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input v-model={filters.userId} placeholder={t('admin.resellerBalanceAccounts.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerBalanceAccounts.filters.statusAll'),
                      value: '__all__',
                    },
                    ...RESELLER_BALANCE_STATUSES.map((v) => ({
                      label: statusLabel(v),
                      value: v,
                    })),
                  ]}
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
            emptyText={t('admin.resellerBalanceAccounts.empty')}
            minWidth="980px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
