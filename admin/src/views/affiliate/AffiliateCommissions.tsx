import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminAffiliateCommission } from '@/api/types'
import { formatDate } from '@/utils/format'
import { AFFILIATE_COMMISSION_STATUSES } from './affiliateUtils'
import { AffiliateProfileCell } from './components/AffiliateProfileCell'
import { useAffiliateCommissions } from './useAffiliateCommissions'

const STATUS_KEY: Record<string, string> = {
  pending_confirm: 'pendingConfirm',
  available: 'available',
  rejected: 'rejected',
  withdrawn: 'withdrawn',
}
const STATUS_TONE = { pending_confirm: 'warning', available: 'success', rejected: 'neutral', withdrawn: 'info' } as const

export default defineComponent({
  name: 'AffiliateCommissionsView',
  setup() {
    const { t } = useI18n()
    const p = useAffiliateCommissions()
    onMounted(() => void p.list.fetchData(1))

    const statusLabel = (s?: string) => (s && STATUS_KEY[s] ? t(`admin.affiliatesCommissions.status.${STATUS_KEY[s]}`) : s || '-')
    const num = 'zs-num font-mono text-xs text-fg'
    const date = 'whitespace-nowrap text-xs text-muted'

    const columns = (): DataTableColumn<AdminAffiliateCommission>[] => [
      { key: 'id', title: t('admin.affiliatesCommissions.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'user', title: t('admin.affiliatesCommissions.table.user'), class: 'min-w-[160px]', render: (r) => <AffiliateProfileCell profile={r.affiliate_profile} /> },
      { key: 'orderNo', title: t('admin.affiliatesCommissions.table.orderNo'), class: 'min-w-[160px] break-all font-mono text-xs text-fg', render: (r) => r.order?.order_no || '-' },
      { key: 'baseAmount', title: t('admin.affiliatesCommissions.table.baseAmount'), class: num, render: (r) => r.base_amount || '0.00' },
      { key: 'rate', title: t('admin.affiliatesCommissions.table.rate'), class: num, render: (r) => `${r.rate_percent || '0.00'}%` },
      { key: 'commission', title: t('admin.affiliatesCommissions.table.commission'), class: 'zs-num font-mono text-xs font-semibold text-primary', render: (r) => r.commission_amount || '0.00' },
      {
        key: 'status',
        title: t('admin.affiliatesCommissions.table.status'),
        render: (r) => (
          <Badge tone={STATUS_TONE[r.status as keyof typeof STATUS_TONE] ?? 'secondary'} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      { key: 'confirmAt', title: t('admin.affiliatesCommissions.table.confirmAt'), class: date, render: (r) => formatDate(r.confirm_at) || '-' },
      { key: 'availableAt', title: t('admin.affiliatesCommissions.table.availableAt'), class: date, render: (r) => formatDate(r.available_at) || '-' },
      { key: 'createdAt', title: t('admin.affiliatesCommissions.table.createdAt'), class: date, render: (r) => formatDate(r.created_at) },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.affiliatesCommissions.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.keyword} placeholder={t('admin.affiliatesCommissions.filters.keyword')} onEnter={p.list.handleSearch} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.orderNo} placeholder={t('admin.affiliatesCommissions.filters.orderNo')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.affiliateProfileId} placeholder={t('admin.affiliatesCommissions.filters.profileId')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('admin.affiliatesCommissions.filters.statusAll'), value: '__all__' },
                    ...AFFILIATE_COMMISSION_STATUSES.map((s) => ({ label: statusLabel(s), value: s })),
                  ]}
                />
              </>
            ),
            actions: () => (
              <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.affiliatesCommissions.empty')}
            minWidth="1000px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>
      </div>
    )
  },
})
