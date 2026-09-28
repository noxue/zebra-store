import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Plus, RefreshCw, Search } from 'lucide-vue-next'
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
  Switch,
  type DataTableColumn,
} from '@/components/ui'
import type { AdminPromotion } from '@/api/types'
import { formatDate, labelSeparator } from '@/utils/format'
import { buildProductLabel, formatProductScope } from './marketingUtils'
import { usePromotions } from './usePromotions'

export default defineComponent({
  name: 'PromotionsView',
  setup() {
    const { t, locale } = useI18n()
    const p = usePromotions()
    onMounted(() => void p.init())

    const discountTypeLabel = (type: string) =>
      type === 'percent' ? t('admin.common.discountTypes.percent') : type === 'fixed' ? t('admin.common.discountTypes.fixed') : type

    const periodLine = (label: string, value?: string) => (
      <div class="break-words">
        <span class="text-muted/80">
          {label}
          {labelSeparator(locale.value)}
        </span>
        <span class="text-fg/80">{formatDate(value) || '-'}</span>
      </div>
    )

    const columns = (): DataTableColumn<AdminPromotion>[] => [
      {
        key: 'id',
        title: t('admin.promotions.table.id'),
        render: (r) => <IdCell value={r.id} />,
      },
      {
        key: 'name',
        title: t('admin.promotions.table.name'),
        class: 'font-medium break-words',
        render: (r) => r.name,
      },
      {
        key: 'type',
        title: t('admin.promotions.table.type'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => discountTypeLabel(r.type),
      },
      {
        key: 'value',
        title: t('admin.promotions.table.value'),
        class: 'zs-num',
        render: (r) => String(r.value),
      },
      {
        key: 'scope',
        title: t('admin.promotions.table.scope'),
        class: 'min-w-[140px] text-xs text-muted break-words',
        render: (r) => (r.scope_ref_id > 0 ? formatProductScope([Math.floor(r.scope_ref_id)], p.productOpts.products.value) : '-'),
      },
      {
        key: 'minAmount',
        title: t('admin.promotions.table.minAmount'),
        class: 'zs-num text-xs text-muted',
        render: (r) => (r.min_amount ? String(r.min_amount) : '-'),
      },
      {
        key: 'period',
        title: t('admin.promotions.table.period'),
        class: 'min-w-[170px] text-xs',
        render: (r) => (
          <div class="space-y-0.5">
            {periodLine(t('admin.promotions.period.startsAt'), r.starts_at)}
            {periodLine(t('admin.promotions.period.endsAt'), r.ends_at)}
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.promotions.table.status'),
        render: (r) => (
          <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
            {r.is_active ? t('admin.common.enabled') : t('admin.common.disabled')}
          </Badge>
        ),
      },
      {
        key: 'action',
        title: t('admin.promotions.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="sm" onClick={() => p.openEdit(r)}>
              {t('admin.promotions.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => p.remove(r)}>
              {t('admin.promotions.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const productOptions = (first: { label: string; value: string }) => [
      first,
      ...p.productOpts.products.value.map((prod) => ({
        label: buildProductLabel(prod),
        value: String(prod.id),
      })),
    ]
    const searchProducts = () => p.productOpts.load(p.productKeyword.value)

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.promotions.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.promotions.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.name} placeholder={t('admin.promotions.filterNamePlaceholder')} onEnter={p.handleSearch} />
                <div class="flex gap-2">
                  <div class="min-w-0 flex-1">
                    <Input v-model={p.productKeyword.value} placeholder={t('admin.promotions.filterScopeSearchPlaceholder')} onEnter={searchProducts} />
                  </div>
                  <Button loading={p.productOpts.loading.value} onClick={searchProducts}>
                    {t('admin.promotions.filterScopeSearchAction')}
                  </Button>
                </div>
                <Select
                  v-model={p.filters.scopeRefId}
                  onChange={p.handleSearch}
                  options={productOptions({
                    label: t('admin.promotions.filterScopeAll'),
                    value: '__all__',
                  })}
                  placeholder={t('admin.promotions.filterScope')}
                />
                <Select
                  v-model={p.filters.isActive}
                  onChange={p.handleSearch}
                  options={[
                    {
                      label: t('admin.promotions.filterStatusAll'),
                      value: '__all__',
                    },
                    { label: t('admin.common.enabled'), value: 'true' },
                    { label: t('admin.common.disabled'), value: 'false' },
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
            emptyText={t('admin.promotions.empty')}
            minWidth="960px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.promotions.modal.editTitle') : t('admin.promotions.modal.title')}
          size="3xl"
          closeOnOverlay={false}
        >
          {{
            default: () => (
              <form
                class="grid grid-cols-1 gap-4 md:grid-cols-2"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  p.submit()
                }}
              >
                <FormField label={t('admin.promotions.modal.name')} required error={p.errors.name}>
                  <Input v-model={p.form.name} placeholder={t('admin.promotions.modal.namePlaceholder')} />
                </FormField>
                <FormField label={t('admin.promotions.modal.type')} required error={p.errors.type}>
                  <Select
                    v-model={p.form.type}
                    options={[
                      {
                        label: t('admin.common.discountTypes.percent'),
                        value: 'percent',
                      },
                      {
                        label: t('admin.common.discountTypes.fixed'),
                        value: 'fixed',
                      },
                    ]}
                  />
                </FormField>
                <div class="space-y-2">
                  <FormField label={t('admin.promotions.modal.value')} required error={p.errors.value} hint={p.valueHint.value}>
                    <Input type="number" step="0.01" v-model={p.form.value} placeholder="10" />
                  </FormField>
                  {p.risk.value && (
                    <div class="rounded-zs-sm bg-warning-soft px-3 py-2 text-[11px] leading-5 text-warning-text">
                      <div class="flex items-center gap-1.5 font-medium">
                        <AlertTriangle class="h-3.5 w-3.5" />
                        {t('admin.promotions.modal.riskHintTitle')}
                      </div>
                      <div class="mt-1">
                        {t('admin.promotions.modal.riskHintFixedMayZero', {
                          value: p.risk.value.discountValue,
                          price: p.risk.value.referencePrice,
                        })}
                      </div>
                      <div class="mt-1">{t('admin.promotions.modal.riskHintReferencePrice')}</div>
                    </div>
                  )}
                </div>
                <div class="hidden md:block" />
                <div class="md:col-span-2">
                  <FormField label={t('admin.promotions.modal.scope')} required>
                    <div class="space-y-2">
                      <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
                        <div class="flex-1">
                          <Input
                            icon={Search}
                            v-model={p.productKeyword.value}
                            placeholder={t('admin.promotions.modal.scopeSearchPlaceholder')}
                            onEnter={searchProducts}
                          />
                        </div>
                        <Button size="sm" loading={p.productOpts.loading.value} onClick={searchProducts}>
                          {t('admin.promotions.modal.scopeSearchAction')}
                        </Button>
                      </div>
                      <Select
                        modelValue={p.form.scope}
                        onUpdate:modelValue={(v) => (p.form.scope = String(v))}
                        options={productOptions({
                          label: t('admin.promotions.modal.scopePlaceholder'),
                          value: '__none__',
                        })}
                      />
                    </div>
                  </FormField>
                </div>
                <FormField label={t('admin.promotions.modal.minAmount')} hint={t('admin.promotions.modal.minAmountHint')}>
                  <Input type="number" step="0.01" v-model={p.form.min_amount} placeholder="0" />
                </FormField>
                <div class="hidden md:block" />
                <FormField label={t('admin.promotions.modal.startsAt')}>
                  <DateTimeInput v-model={p.form.starts_at} />
                </FormField>
                <FormField label={t('admin.promotions.modal.endsAt')}>
                  <DateTimeInput v-model={p.form.ends_at} />
                </FormField>
                <div class="md:col-span-2">
                  <Switch v-model={p.form.is_active} label={t('admin.common.enabled')} />
                </div>
                {p.modal.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-sm text-danger-text md:col-span-2">{p.modal.error.value}</p>}
              </form>
            ),
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {t('admin.common.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
