import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  PageHeader,
  Select,
  Textarea,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminResellerWithdraw } from '@/api/types'
import { formatDate } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { CreatedRange } from './components/CreatedRange'
import { RefreshButton } from './components/RefreshButton'
import { ResellerCell } from './components/ResellerCell'
import { RESELLER_WITHDRAW_STATUSES, amountOrZero, processorName, withdrawStatusTone } from './resellerUtils'
import { useResellerWithdraws } from './useResellerFinanceLists'

export default defineComponent({
  name: 'ResellerWithdrawsView',
  setup() {
    const { t } = useI18n()
    const w = useResellerWithdraws()
    const { filters, list } = w
    onMounted(() => void list.fetchData(1))

    const statusLabel = (v?: string) =>
      v && (RESELLER_WITHDRAW_STATUSES as readonly string[]).includes(v) ? t(`admin.resellerWithdraws.status.${v}`) : v || '-'

    const payoutDetails = (row: AdminResellerWithdraw) => (
      <div class="space-y-1 break-words text-xs">
        <div>{row.channel || '-'}</div>
        <div class="break-all text-muted">{row.account || '-'}</div>
        {row.payee_name && <div>{t('admin.resellerWithdraws.table.payeeName')}: {row.payee_name}</div>}
        {row.qr_code_url && <a href={getImageUrl(row.qr_code_url)} target="_blank" rel="noopener noreferrer" class="text-primary underline">{t('admin.resellerWithdraws.table.viewQr')}</a>}
      </div>
    )

    const actions = (row: AdminResellerWithdraw) => {
      const disabled = w.operating.value || row.status !== 'pending'
      return (
        <div class="flex flex-wrap gap-2">
          <Button size="xs" disabled={disabled} onClick={() => w.openReject(row)}>{t('admin.resellerWithdraws.actions.reject')}</Button>
          <Button size="xs" variant="primary" disabled={disabled} onClick={() => w.pay(row)}>{t('admin.resellerWithdraws.actions.pay')}</Button>
        </div>
      )
    }

    const columns = (): DataTableColumn<AdminResellerWithdraw>[] => [
      {
        key: 'id',
        title: t('admin.resellerWithdraws.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'reseller',
        title: t('admin.resellerWithdraws.table.reseller'),
        render: (r) => <ResellerCell profile={r.profile} resellerId={r.reseller_id} />,
      },
      {
        key: 'amount',
        title: t('admin.resellerWithdraws.table.amount'),
        class: 'zs-num text-sm font-semibold',
        render: (r) => amountOrZero(r.amount),
      },
      {
        key: 'currency',
        title: t('admin.resellerWithdraws.table.currency'),
        class: 'font-mono text-xs text-muted',
        render: (r) => r.currency || '-',
      },
      {
        key: 'channel',
        title: t('admin.resellerWithdraws.table.channel'),
        class: 'text-xs break-words',
        render: payoutDetails,
      },
      {
        key: 'status',
        title: t('admin.resellerWithdraws.table.status'),
        render: (r) => (
          <Badge tone={withdrawStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      {
        key: 'rejectReason',
        title: t('admin.resellerWithdraws.table.rejectReason'),
        class: 'min-w-[140px] break-words text-xs text-muted',
        render: (r) => r.reject_reason || '-',
      },
      {
        key: 'processedBy',
        title: t('admin.resellerWithdraws.table.processedBy'),
        class: 'text-xs text-muted',
        render: (r) => (
          <div>
            <div class="break-words text-fg">{processorName(r)}</div>
            <div class="mt-0.5 whitespace-nowrap">{formatDate(r.processed_at) || '-'}</div>
          </div>
        ),
      },
      {
        key: 'createdAt',
        title: t('admin.resellerWithdraws.table.createdAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.created_at),
      },
      {
        key: 'action',
        title: t('admin.resellerWithdraws.table.action'),
        align: 'right',
        render: actions,
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.resellerWithdraws.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={filters.keyword}
                  placeholder={t('admin.resellerWithdraws.filters.keyword')}
                  onUpdate:modelValue={list.debouncedSearch}
                />
                <Input v-model={filters.resellerId} placeholder={t('admin.resellerWithdraws.filters.resellerId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.userId} placeholder={t('admin.resellerWithdraws.filters.userId')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    {
                      label: t('admin.resellerWithdraws.filters.statusAll'),
                      value: '__all__',
                    },
                    ...RESELLER_WITHDRAW_STATUSES.map((v) => ({
                      label: statusLabel(v),
                      value: v,
                    })),
                  ]}
                />
                <CreatedRange
                  label={t('admin.resellerWithdraws.filters.createdRange')}
                  v-model:from={filters.createdFrom}
                  v-model:to={filters.createdTo}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => <RefreshButton loading={w.refreshing.value} onClick={w.refresh} />,
          }}
        </FilterBar>
        <div>
          <div class="space-y-3 md:hidden">
            {!list.loading.value && list.items.value.length === 0 && <p class="py-8 text-center text-sm text-muted">{t('admin.resellerWithdraws.empty')}</p>}
            {list.items.value.map((row) => (
              <article key={row.id} class="zs-glass space-y-3 rounded-zs p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="zs-num font-semibold">{amountOrZero(row.amount)} {row.currency}</div>
                  <Badge tone={withdrawStatusTone(row.status)} dot>{statusLabel(row.status)}</Badge>
                </div>
                <ResellerCell profile={row.profile} resellerId={row.reseller_id} />
                {payoutDetails(row)}
                <p class="text-xs text-muted">{formatDate(row.created_at)}</p>
                {row.reject_reason && <p class="text-xs text-danger">{row.reject_reason}</p>}
                {row.status === 'pending' && actions(row)}
              </article>
            ))}
          </div>
          <div class="hidden md:block"><DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.resellerWithdraws.empty')}
            minWidth="1120px"
          /></div>
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>

        <Dialog v-model={w.showRejectDialog.value} title={`${t('admin.resellerWithdraws.actions.reject')} #${w.selected.value?.id ?? '-'}`} size="sm">
          {{
            default: () => (
              <FormField label={t('admin.resellerWithdraws.actions.rejectReasonPrompt')}>
                <Textarea v-model={w.rejectReason.value} rows={4} />
              </FormField>
            ),
            footer: () => (
              <>
                <Button onClick={() => (w.showRejectDialog.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="danger" loading={w.operating.value} onClick={w.submitReject}>
                  {t('admin.resellerWithdraws.actions.reject')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
