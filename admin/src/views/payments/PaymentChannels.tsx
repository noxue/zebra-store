import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Plus, RefreshCw, Save } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, FilterBar, IdCell, ListPagination, PageHeader, Select, Switch, type DataTableColumn } from '@/components/ui'
import type { AdminPaymentChannel } from '@/api/types'
import { getImageUrl } from '@/utils/image'
import { PROVIDER_TYPES } from './paymentChannelRules'
import { CHANNEL_FILTER_TYPES, channelTypeLabel, formatFeeRate, interactionModeLabel, providerTypeLabel, resolveChannelTypeDisplay } from './paymentLabels'
import { usePaymentChannels } from './usePaymentChannels'
import { PaymentChannelModal } from './components/PaymentChannelModal'

export default defineComponent({
  name: 'PaymentChannelsView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const p = usePaymentChannels()
    const { filters, list, feeConfig } = p

    onMounted(() => {
      void list.fetchData(1)
      void p.loadFeeConfig()
      p.openEditById(route.query.channel_id)
    })
    watch(
      () => route.query.channel_id,
      (v) => v && p.openEditById(v),
    )

    const providerFilterOptions = () => [
      { label: t('admin.paymentChannels.filterProviderAll'), value: '__all__' },
      ...PROVIDER_TYPES.map((v) => ({
        value: v,
        label: v === 'dujiaopay' ? `${providerTypeLabel(t, v)}（${t('admin.paymentChannels.providerOfficialCertified')}）` : providerTypeLabel(t, v),
      })),
    ]
    const channelFilterOptions = () => [
      { label: t('admin.paymentChannels.filterChannelAll'), value: '__all__' },
      ...CHANNEL_FILTER_TYPES.map((v) => ({ value: v, label: channelTypeLabel(t, v) })),
    ]

    const columns = (): DataTableColumn<AdminPaymentChannel>[] => [
      { key: 'id', title: t('admin.paymentChannels.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'name',
        title: t('admin.paymentChannels.table.name'),
        class: 'min-w-[220px]',
        render: (r) => (
          <div class="flex items-center gap-2">
            {r.icon && <img src={getImageUrl(r.icon)} alt="" class="h-6 w-6 shrink-0 rounded-md object-contain" />}
            <span class="break-words font-medium text-fg">{r.name}</span>
          </div>
        ),
      },
      {
        key: 'type',
        title: t('admin.paymentChannels.table.type'),
        class: 'min-w-[200px] text-xs',
        render: (r) => (
          <div class="space-y-0.5">
            <div class="break-words text-fg">{providerTypeLabel(t, r.provider_type)}</div>
            <div class="break-words text-muted">{resolveChannelTypeDisplay(t, r)}</div>
          </div>
        ),
      },
      { key: 'interaction', title: t('admin.paymentChannels.table.interaction'), class: 'text-xs text-muted', render: (r) => interactionModeLabel(t, r.interaction_mode) },
      { key: 'feeRate', title: t('admin.paymentChannels.table.feeRate'), class: 'zs-num text-xs whitespace-nowrap', render: (r) => formatFeeRate(r.fee_rate, r.fixed_fee) },
      {
        key: 'status',
        title: t('admin.paymentChannels.table.status'),
        render: (r) => (
          <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
            {r.is_active ? t('admin.common.enabled') : t('admin.common.disabled')}
          </Badge>
        ),
      },
      { key: 'sort', title: t('admin.paymentChannels.table.sort'), class: 'zs-num text-xs', render: (r) => r.sort_order },
      {
        key: 'action',
        title: t('admin.paymentChannels.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex flex-wrap justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r.id)}>
              {t('admin.common.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r)}>
              {t('admin.common.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const feeRow = (label: string, tip: string, key: 'customer_fee_enabled' | 'reuse_legacy_order_fee_payment') => (
      <div class="flex items-start justify-between gap-4 border-t border-line pt-4">
        <div class="min-w-0">
          <div class="text-sm font-medium text-fg">{label}</div>
          <p class="mt-1 text-xs leading-relaxed text-muted">{tip}</p>
        </div>
        <Switch modelValue={feeConfig[key]} onUpdate:modelValue={(v: boolean) => (feeConfig[key] = v)} />
      </div>
    )

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.paymentChannels.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.paymentChannels.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <Card title={t('admin.paymentChannels.feePolicy.title')} description={t('admin.paymentChannels.feePolicy.subtitle')}>
          <div class="space-y-4">
            {feeRow(t('admin.paymentChannels.feePolicy.customerFee'), t('admin.paymentChannels.feePolicy.customerFeeTip'), 'customer_fee_enabled')}
            {feeRow(t('admin.paymentChannels.feePolicy.reuseLegacy'), t('admin.paymentChannels.feePolicy.reuseLegacyTip'), 'reuse_legacy_order_fee_payment')}
            <div class="flex justify-end border-t border-line pt-4">
              <Button variant="primary" loading={p.feeConfigSaving.value} onClick={p.saveFeeConfig}>
                <Save class="h-4 w-4" />
                {p.feeConfigSaving.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
              </Button>
            </div>
          </div>
        </Card>

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Select v-model={filters.providerType} options={providerFilterOptions()} onChange={list.handleSearch} />
                <Select v-model={filters.channelType} options={channelFilterOptions()} onChange={list.handleSearch} />
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
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.paymentChannels.empty')}
            minWidth="980px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>

        <PaymentChannelModal v-model={p.showModal.value} channelId={p.editingId.value} onSuccess={p.onModalSuccess} />
      </div>
    )
  },
})
