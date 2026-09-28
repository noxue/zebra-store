import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Download, Plus, RefreshCw, Search } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  DateTimeInput,
  Dialog,
  FilterBar,
  FormField,
  IdCell,
  Input,
  ListPagination,
  PageHeader,
  Select,
  type BadgeTone,
  type DataTableColumn,
} from '@/components/ui'
import { formatDate, formatMoney } from '@/utils/format'
import { formatRedeemedUser, giftCardDisplayStatus } from './marketingUtils'
import { useGiftCards, type GiftCardRow } from './useGiftCards'

const STATUS_TONE: Record<string, BadgeTone> = { active: 'success', expired: 'warning', redeemed: 'info', disabled: 'neutral' }

export default defineComponent({
  name: 'GiftCardsView',
  setup() {
    const { t, te } = useI18n()
    const p = useGiftCards()
    onMounted(() => void p.fetch(1))

    const statusLabel = (card: GiftCardRow) => {
      const s = giftCardDisplayStatus(card)
      const key = `admin.giftCards.status.${s}`
      return te(key) ? t(key) : s || '-'
    }

    const columns = (): DataTableColumn<GiftCardRow>[] => [
      { key: 'id', title: t('admin.giftCards.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'name', title: t('admin.giftCards.table.name'), class: 'min-w-[110px] font-medium break-words', render: (r) => r.name || '-' },
      { key: 'code', title: t('admin.giftCards.table.code'), class: 'min-w-[140px] font-mono text-xs break-all', render: (r) => r.code || '-' },
      { key: 'amount', title: t('admin.giftCards.table.amount'), class: 'zs-num text-xs whitespace-nowrap', render: (r) => (r.amount ? formatMoney(r.amount, r.currency) : '-') },
      {
        key: 'status',
        title: t('admin.giftCards.table.status'),
        render: (r) => (
          <Badge tone={STATUS_TONE[giftCardDisplayStatus(r)] ?? 'neutral'} dot>
            {statusLabel(r)}
          </Badge>
        ),
      },
      { key: 'batchNo', title: t('admin.giftCards.table.batchNo'), class: 'min-w-[96px] font-mono text-xs text-muted break-all', render: (r) => r.batch?.batch_no || '-' },
      { key: 'redeemedUser', title: t('admin.giftCards.table.redeemedUser'), class: 'min-w-[140px] text-xs text-muted break-words', render: (r) => formatRedeemedUser(r.redeemed_user) },
      { key: 'redeemedAt', title: t('admin.giftCards.table.redeemedAt'), class: 'min-w-[88px] text-xs text-muted', render: (r) => formatDate(r.redeemed_at) || '-' },
      {
        key: 'expiresAt',
        title: t('admin.giftCards.table.expiresAt'),
        class: 'min-w-[72px] text-xs text-muted',
        render: (r) => (r.expires_at ? formatDate(r.expires_at) : t('admin.giftCards.neverExpire')),
      },
      { key: 'createdAt', title: t('admin.giftCards.table.createdAt'), class: 'min-w-[88px] text-xs text-muted', render: (r) => formatDate(r.created_at) || '-' },
      {
        key: 'action',
        title: t('admin.giftCards.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex flex-col items-end gap-1.5">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.giftCards.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" disabled={String(r.status || '').toLowerCase() === 'redeemed'} onClick={() => p.remove(r)}>
              {t('admin.giftCards.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const statusOptions = () => [
      { label: t('admin.giftCards.status.active'), value: 'active' },
      { label: t('admin.giftCards.status.disabled'), value: 'disabled' },
    ]

    const selection = () => ({
      allSelected: p.selection.allSelected.value,
      someSelected: p.selection.someSelected.value,
      isSelected: p.selection.isSelected,
      toggle: p.selection.toggle,
      toggleAll: p.selection.toggleAll,
    })

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.giftCards.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openGenerate}>
                <Plus class="h-4 w-4" />
                {t('admin.giftCards.generate')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={5}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.code} placeholder={t('admin.giftCards.filterCode')} onEnter={p.handleSearch} />
                <Input type="number" min={1} v-model={p.filters.redeemedUserID} placeholder={t('admin.giftCards.filterUserID')} onEnter={p.handleSearch} />
                <Select
                  v-model={p.filters.status}
                  onChange={p.handleSearch}
                  options={[
                    { label: t('admin.giftCards.filterStatusAll'), value: '__all__' },
                    { label: t('admin.giftCards.status.active'), value: 'active' },
                    { label: t('admin.giftCards.status.expired'), value: 'expired' },
                    { label: t('admin.giftCards.status.redeemed'), value: 'redeemed' },
                    { label: t('admin.giftCards.status.disabled'), value: 'disabled' },
                  ]}
                />
                <DateTimeInput v-model={p.filters.createdFrom} placeholder={t('admin.giftCards.filterCreatedFrom')} />
                <DateTimeInput v-model={p.filters.createdTo} placeholder={t('admin.giftCards.filterCreatedTo')} />
              </>
            ),
            actions: () => (
              <>
                <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
                <Button size="sm" variant="primary" onClick={p.handleSearch}>
                  <Search class="h-3.5 w-3.5" />
                  {t('admin.giftCards.search')}
                </Button>
              </>
            ),
          }}
        </FilterBar>

        {p.hasSelection.value && (
          <div class="zs-glass flex flex-col gap-3 rounded-zs-lg p-4 shadow-zs-sm lg:flex-row lg:items-center lg:justify-between">
            <span class="text-xs text-muted">{t('admin.giftCards.batch.selectedCount', { count: p.selection.selectedIds.value.length })}</span>
            <div class="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
              <div class="sm:w-[170px]">
                <Select size="sm" v-model={p.batchStatusTarget.value} options={statusOptions()} placeholder={t('admin.giftCards.batch.statusPlaceholder')} />
              </div>
              <Button size="sm" disabled={p.batchLoading.value} onClick={p.applyBatchStatus}>
                {t('admin.giftCards.batch.applyStatus')}
              </Button>
              <Button size="sm" disabled={p.batchLoading.value} onClick={() => p.exportSelected('txt')}>
                <Download class="h-3.5 w-3.5" />
                {t('admin.giftCards.batch.exportTxt')}
              </Button>
              <Button size="sm" disabled={p.batchLoading.value} onClick={() => p.exportSelected('csv')}>
                <Download class="h-3.5 w-3.5" />
                {t('admin.giftCards.batch.exportCsv')}
              </Button>
            </div>
          </div>
        )}

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            selection={selection()}
            emptyText={t('admin.giftCards.empty')}
            minWidth="1060px"
            dense
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.changePage} onChangePageSize={p.changePageSize} />
        </div>

        <Dialog v-model={p.showGenerate.value} title={t('admin.giftCards.modal.generateTitle')} size="lg" closeOnOverlay={false}>
          {{
            default: () => (
              <form
                class="grid grid-cols-1 gap-4 md:grid-cols-2"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  void p.submitGenerate()
                }}
              >
                <div class="md:col-span-2">
                  <FormField label={t('admin.giftCards.form.name')} required>
                    <Input v-model={p.generateForm.name} maxlength={120} />
                  </FormField>
                </div>
                <FormField label={t('admin.giftCards.form.quantity')} required>
                  <Input type="number" min={1} max={10000} v-model={p.generateForm.quantity} />
                </FormField>
                <FormField label={t('admin.giftCards.form.amount')} required>
                  <Input v-model={p.generateForm.amount} mono placeholder="0.00" />
                </FormField>
                <FormField label={t('admin.giftCards.form.expiresAt')} hint={t('admin.giftCards.form.expiresHint')}>
                  <DateTimeInput v-model={p.generateForm.expiresAt} />
                </FormField>
                {p.generateError.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-sm text-danger-text md:col-span-2">{p.generateError.value}</p>}
              </form>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showGenerate.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.generateSubmitting.value} onClick={p.submitGenerate}>
                  {t('admin.common.confirm')}
                </Button>
              </>
            ),
          }}
        </Dialog>

        <Dialog v-model={p.showEdit.value} title={t('admin.giftCards.modal.editTitle')} size="lg" closeOnOverlay={false}>
          {{
            default: () => (
              <form
                class="grid grid-cols-1 gap-4 md:grid-cols-2"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  void p.submitEdit()
                }}
              >
                <div class="md:col-span-2">
                  <FormField label={t('admin.giftCards.form.name')} required>
                    <Input v-model={p.editForm.name} maxlength={120} />
                  </FormField>
                </div>
                <FormField label={t('admin.giftCards.form.status')} hint={p.editReadonly.value ? t('admin.giftCards.form.statusReadonly') : undefined}>
                  <Select
                    modelValue={p.editForm.status}
                    onUpdate:modelValue={(v) => (p.editForm.status = v === 'disabled' ? 'disabled' : 'active')}
                    options={statusOptions()}
                    disabled={p.editReadonly.value}
                  />
                </FormField>
                <FormField label={t('admin.giftCards.form.expiresAt')} hint={t('admin.giftCards.form.editExpiresHint')}>
                  <DateTimeInput v-model={p.editForm.expiresAt} />
                </FormField>
                {p.editError.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-sm text-danger-text md:col-span-2">{p.editError.value}</p>}
              </form>
            ),
            footer: () => (
              <>
                <Button onClick={() => (p.showEdit.value = false)}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.editSubmitting.value} onClick={p.submitEdit}>
                  {t('admin.common.confirm')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
