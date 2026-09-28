import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FilterBar, FormField, IdCell, Input, ListPagination, PageHeader, Select, Textarea, type DataTableColumn } from '@/components/ui'
import type { AdminAffiliateWithdraw } from '@/api/types'
import { formatDate } from '@/utils/format'
import { AFFILIATE_WITHDRAW_STATUSES, AFFILIATE_WITHDRAW_STATUS_PENDING_REVIEW } from './affiliateUtils'
import { AffiliateProfileCell } from './components/AffiliateProfileCell'
import { useAffiliateWithdraws } from './useAffiliateWithdraws'

const STATUS_KEY: Record<string, string> = { pending_review: 'pendingReview', rejected: 'rejected', paid: 'paid' }
const STATUS_TONE = { pending_review: 'warning', rejected: 'neutral', paid: 'success' } as const

const processorName = (p: AdminAffiliateWithdraw['processor']) => (typeof p === 'object' && p !== null ? p.username : p) || '-'

export default defineComponent({
  name: 'AffiliateWithdrawsView',
  setup() {
    const { t } = useI18n()
    const p = useAffiliateWithdraws()
    onMounted(() => void p.list.fetchData(1))

    const statusLabel = (s?: string) => (s && STATUS_KEY[s] ? t(`admin.affiliatesWithdraws.status.${STATUS_KEY[s]}`) : s || '-')

    const columns = (): DataTableColumn<AdminAffiliateWithdraw>[] => [
      { key: 'id', title: t('admin.affiliatesWithdraws.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'user', title: t('admin.affiliatesWithdraws.table.user'), class: 'min-w-[140px]', render: (r) => <AffiliateProfileCell profile={r.affiliate_profile} /> },
      { key: 'amount', title: t('admin.affiliatesWithdraws.table.amount'), class: 'zs-num font-mono text-xs font-semibold text-primary', render: (r) => r.amount || '0.00' },
      { key: 'channel', title: t('admin.affiliatesWithdraws.table.channel'), class: 'break-words text-xs text-fg', render: (r) => r.channel || '-' },
      { key: 'account', title: t('admin.affiliatesWithdraws.table.account'), class: 'max-w-[180px] break-all text-xs text-muted', render: (r) => r.account || r.account_info || '-' },
      {
        key: 'status',
        title: t('admin.affiliatesWithdraws.table.status'),
        render: (r) => (
          <Badge tone={STATUS_TONE[r.status as keyof typeof STATUS_TONE] ?? 'secondary'} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      { key: 'rejectReason', title: t('admin.affiliatesWithdraws.table.rejectReason'), class: 'max-w-[180px] break-words text-xs text-muted', render: (r) => r.reject_reason || '-' },
      {
        key: 'processedBy',
        title: t('admin.affiliatesWithdraws.table.processedBy'),
        class: 'text-xs text-muted',
        render: (r) => (
          <div>
            <div class="break-words">{processorName(r.processor)}</div>
            <div class="mt-0.5 whitespace-nowrap">{formatDate(r.processed_at) || '-'}</div>
          </div>
        ),
      },
      { key: 'createdAt', title: t('admin.affiliatesWithdraws.table.createdAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.affiliatesWithdraws.table.action'),
        align: 'right',
        render: (r) => {
          const disabled = p.operating.value || r.status !== AFFILIATE_WITHDRAW_STATUS_PENDING_REVIEW
          return (
            <div class="flex justify-end gap-2 whitespace-nowrap">
              <Button size="sm" disabled={disabled} onClick={() => p.openReject(r)}>
                {t('admin.affiliatesWithdraws.actions.reject')}
              </Button>
              <Button size="sm" variant="primary" disabled={disabled} onClick={() => p.pay(r)}>
                {t('admin.affiliatesWithdraws.actions.pay')}
              </Button>
            </div>
          )
        },
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.affiliatesWithdraws.title')} />
        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.keyword} placeholder={t('admin.affiliatesWithdraws.filters.keyword')} onEnter={p.list.handleSearch} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.affiliateProfileId} placeholder={t('admin.affiliatesWithdraws.filters.profileId')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('admin.affiliatesWithdraws.filters.statusAll'), value: '__all__' },
                    ...AFFILIATE_WITHDRAW_STATUSES.map((s) => ({ label: statusLabel(s), value: s })),
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
            emptyText={t('admin.affiliatesWithdraws.empty')}
            minWidth="980px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.showReject.value}
          title={t('admin.affiliatesWithdraws.actions.reject')}
          description={p.rejectTarget.value ? t('admin.affiliatesWithdraws.actions.rejectConfirm', { id: p.rejectTarget.value.id }) : ''}
          size="sm"
        >
          {{
            default: () => (
              <FormField label={t('admin.affiliatesWithdraws.table.rejectReason')} hint={t('admin.affiliatesWithdraws.actions.rejectReasonPrompt')}>
                <Textarea v-model={p.rejectReason.value} rows={3} placeholder={t('admin.affiliatesWithdraws.actions.rejectReasonPrompt')} />
              </FormField>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showReject.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="danger" loading={p.operating.value} onClick={p.submitReject}>
                  {t('admin.affiliatesWithdraws.actions.reject')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
