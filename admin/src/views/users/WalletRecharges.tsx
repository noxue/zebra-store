import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, RangeFilter, Select, type DataTableColumn } from '@/components/ui'
import type { AdminWalletRecharge } from '@/api/types'
import { formatDate } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import { paymentStatusLabel, paymentStatusTone } from '@/utils/status'
import { channelTypeLabel, providerTypeLabel, RECHARGE_PROVIDER_TYPES, RECHARGE_STATUSES, useWalletRecharges } from './useWalletRecharges'

const linkClass = 'text-primary underline-offset-4 hover:underline'

export default defineComponent({
  name: 'WalletRechargesView',
  setup() {
    const { t } = useI18n()
    const { filters, list, refreshing, refresh } = useWalletRecharges()
    onMounted(() => void list.fetchData(1))

    const extLink = (href: string, id: number) => (
      <a href={adminUrl(href)} target="_blank" rel="noopener" class={linkClass}>
        #{id}
      </a>
    )

    const columns = (): DataTableColumn<AdminWalletRecharge>[] => [
      { key: 'id', title: t('admin.walletRecharges.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'rechargeNo', title: t('admin.walletRecharges.table.rechargeNo'), class: 'min-w-[130px] font-mono text-xs break-all', render: (r) => r.recharge_no },
      {
        key: 'user',
        title: t('admin.walletRecharges.table.user'),
        class: 'min-w-[130px] text-xs text-muted',
        render: (r) => (
          <div>
            <div class="text-fg">{r.user_id ? extLink(`/users/${r.user_id}`, r.user_id) : '-'}</div>
            {r.user?.display_name && <div class="mt-0.5 break-words text-fg">{r.user.display_name}</div>}
            {r.user?.email && <div class="mt-0.5 break-all">{r.user.email}</div>}
          </div>
        ),
      },
      {
        key: 'payment',
        title: t('admin.walletRecharges.table.payment'),
        class: 'min-w-[100px] text-xs text-muted',
        render: (r) => (
          <div>
            <div class="text-fg">{r.payment_id ? extLink(`/payments?payment_id=${r.payment_id}`, r.payment_id) : '-'}</div>
            {r.payment_status && (
              <div class="mt-1">
                <Badge tone={paymentStatusTone(r.payment_status)}>{paymentStatusLabel(t, r.payment_status)}</Badge>
              </div>
            )}
          </div>
        ),
      },
      {
        key: 'channel',
        title: t('admin.walletRecharges.table.channel'),
        class: 'min-w-[130px] text-xs text-muted',
        render: (r) => (
          <div>
            <div class="break-words text-fg">{r.channel_name || '-'}</div>
            <div class="break-words">
              {providerTypeLabel(t, r.provider_type)} / {channelTypeLabel(t, r.channel_type)}
            </div>
            <div class="mt-1">
              {t('admin.walletRecharges.channelId')}: {r.channel_id ? extLink(`/payment-channels?channel_id=${r.channel_id}`, r.channel_id) : '-'}
            </div>
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.walletRecharges.table.status'),
        render: (r) => (
          <Badge tone={paymentStatusTone(r.status)} dot>
            {paymentStatusLabel(t, r.status)}
          </Badge>
        ),
      },
      {
        key: 'amount',
        title: t('admin.walletRecharges.table.amount'),
        class: 'min-w-[130px] text-xs text-muted',
        render: (r) => (
          <div>
            <div class="zs-num text-sm font-semibold text-fg">
              {r.amount} {r.currency}
            </div>
            <div class="mt-1">
              {t('admin.walletRecharges.payableAmount')}:{' '}
              <span class="zs-num text-fg">
                {r.payable_amount} {r.currency}
              </span>
            </div>
            <div class="mt-1">
              {t('admin.walletRecharges.feeAmount')}:{' '}
              <span class="zs-num text-fg">
                {r.fee_amount} {r.currency}
              </span>
            </div>
          </div>
        ),
      },
      { key: 'paidAt', title: t('admin.walletRecharges.table.paidAt'), class: 'min-w-[88px] text-xs text-muted', render: (r) => formatDate(r.paid_at) || '-' },
      { key: 'createdAt', title: t('admin.walletRecharges.table.createdAt'), class: 'min-w-[88px] text-xs text-muted', render: (r) => formatDate(r.created_at) },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.walletRecharges.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={filters.rechargeNo} placeholder={t('admin.walletRecharges.filterRechargeNo')} onEnter={list.handleSearch} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.userId} placeholder={t('admin.walletRecharges.filterUserId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.userKeyword} placeholder={t('admin.walletRecharges.filterUserKeyword')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.paymentId} placeholder={t('admin.walletRecharges.filterPaymentId')} onUpdate:modelValue={list.debouncedSearch} />
                <Input v-model={filters.channelId} placeholder={t('admin.walletRecharges.filterChannelId')} onUpdate:modelValue={list.debouncedSearch} />
                <Select
                  v-model={filters.providerType}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('admin.walletRecharges.filterProviderAll'), value: '__all__' },
                    ...RECHARGE_PROVIDER_TYPES.map((v) => ({ label: providerTypeLabel(t, v), value: v })),
                  ]}
                />
                <Select
                  v-model={filters.status}
                  onChange={list.handleSearch}
                  options={[
                    { label: t('admin.walletRecharges.filterStatusAll'), value: '__all__' },
                    ...RECHARGE_STATUSES.map((v) => ({ label: t(`payment.status.${v}`), value: v })),
                  ]}
                />
                <div class="hidden lg:block" />
                <RangeFilter
                  label={t('admin.walletRecharges.filterCreatedRange')}
                  from={filters.createdFrom}
                  to={filters.createdTo}
                  fromPlaceholder={t('admin.walletRecharges.filterCreatedFrom')}
                  toPlaceholder={t('admin.walletRecharges.filterCreatedTo')}
                  onUpdate:from={(v: string) => (filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (filters.createdTo = v)}
                  onChange={list.handleSearch}
                />
                <RangeFilter
                  label={t('admin.walletRecharges.filterPaidRange')}
                  from={filters.paidFrom}
                  to={filters.paidTo}
                  fromPlaceholder={t('admin.walletRecharges.filterPaidFrom')}
                  toPlaceholder={t('admin.walletRecharges.filterPaidTo')}
                  onUpdate:from={(v: string) => (filters.paidFrom = v)}
                  onUpdate:to={(v: string) => (filters.paidTo = v)}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => (
              <Button size="sm" loading={refreshing.value} onClick={refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>
        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.walletRecharges.empty')}
            minWidth="1080px"
            dense
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>
      </div>
    )
  },
})
