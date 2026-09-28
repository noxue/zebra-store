import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { BadgePercent, CircleHelp, Plus, RotateCcw, Search, Trash2 } from 'lucide-vue-next'
import { Badge, Button, DataTable, Dialog, FilterBar, FormField, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminProduct } from '@/api/types'
import { getFirstImageUrl } from '@/utils/image'
import { formatWholesaleSkuLabel } from '@/utils/wholesalePricing'
import { useWholesalePrices } from './useWholesalePrices'

const RULE_NOTE_KEYS = ['admin.wholesalePrices.modal.ruleProductLevel', 'admin.wholesalePrices.modal.ruleQuantityShared', 'admin.wholesalePrices.modal.ruleNoRaise']

export default defineComponent({
  name: 'WholesalePricesView',
  setup() {
    const { t } = useI18n()
    const p = useWholesalePrices()
    onMounted(() => void p.init())

    let timer: ReturnType<typeof setTimeout> | undefined
    const debouncedSearch = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => void p.list.fetchData(1), 300)
    }

    const columns = (): DataTableColumn<AdminProduct>[] => [
      { key: 'id', title: t('admin.wholesalePrices.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'product',
        title: t('admin.wholesalePrices.table.product'),
        render: (r) => {
          const img = getFirstImageUrl(r.images)
          return (
            <div class="flex min-w-[280px] items-center gap-3">
              <div class="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-zs-sm border border-line bg-surface-muted text-[10px] text-muted">
                {img ? <img src={img} alt="" class="h-full w-full object-cover" /> : <span>{t('admin.common.noImage')}</span>}
              </div>
              <div class="min-w-0 flex-1">
                <div class="font-medium break-words">{p.productName(r)}</div>
                <div class="font-mono text-xs text-muted break-all">{r.slug}</div>
              </div>
            </div>
          )
        },
      },
      { key: 'price', title: t('admin.wholesalePrices.table.price'), class: 'zs-num whitespace-nowrap', render: (r) => p.formatPrice(r.price_amount) },
      {
        key: 'tiers',
        title: t('admin.wholesalePrices.table.tiers'),
        class: 'min-w-[240px]',
        render: (r) => <div class={['max-w-[360px] text-sm break-words', p.hasWholesalePrices(r) ? 'text-success-text' : 'text-muted']}>{p.formatWholesaleSummary(r)}</div>,
      },
      {
        key: 'status',
        title: t('admin.wholesalePrices.table.status'),
        render: (r) => (
          <Badge tone={p.hasWholesalePrices(r) ? 'success' : 'neutral'}>
            <BadgePercent class="h-3.5 w-3.5" />
            {p.hasWholesalePrices(r) ? t('admin.wholesalePrices.status.enabled') : t('admin.wholesalePrices.status.disabled')}
          </Badge>
        ),
      },
      {
        key: 'action',
        title: t('admin.wholesalePrices.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="sm" onClick={() => p.openConfigure(r)}>
              {t('admin.wholesalePrices.actions.configure')}
            </Button>
            {p.hasWholesalePrices(r) && (
              <Button size="sm" variant="danger" onClick={() => p.clear(r)}>
                {t('admin.wholesalePrices.actions.clear')}
              </Button>
            )}
          </div>
        ),
      },
    ]

    const scopeOptions = (current: string) => {
      const opts = [
        { label: t('admin.wholesalePrices.modal.skuAll'), value: 'all' },
        ...p.activeSkuOptions.value.map((sku) => ({ label: formatWholesaleSkuLabel(sku, p.locale()), value: p.skuScopeValue(sku) })),
      ]
      if (!p.isKnownSkuScope(current)) opts.push({ label: p.tierScopeLabel(current), value: current })
      return opts
    }

    const renderModalBody = () => {
      const product = p.editingProduct.value
      if (!product) return null
      return (
        <div class="space-y-5">
          <div class="rounded-zs border border-line bg-surface-muted p-4">
            <div class="text-xs text-muted">{t('admin.wholesalePrices.modal.productInfo')}</div>
            <div class="mt-2 flex flex-col gap-1 text-sm">
              <div class="font-medium">
                #{product.id} {p.productName(product)}
              </div>
              <div class="font-mono text-xs text-muted break-all">{product.slug}</div>
              <div class="zs-num text-xs text-muted">{p.formatPrice(product.price_amount)}</div>
            </div>
          </div>

          <div class="rounded-zs bg-info-soft p-4 text-info-text">
            <div class="flex items-start gap-3">
              <CircleHelp class="mt-0.5 h-4 w-4 shrink-0" />
              <div class="min-w-0">
                <div class="text-sm font-medium">{t('admin.wholesalePrices.modal.ruleTitle')}</div>
                <ul class="mt-2 list-disc space-y-1 pl-4 text-xs leading-5">
                  {RULE_NOTE_KEYS.map((k) => (
                    <li key={k}>{t(k)}</li>
                  ))}
                </ul>
              </div>
            </div>
          </div>

          {p.showSkuPriceReference.value && (
            <div class="rounded-zs border border-line bg-surface-strong p-4">
              <div class="flex items-start gap-3">
                <BadgePercent class="mt-0.5 h-4 w-4 shrink-0 text-success-text" />
                <div class="min-w-0">
                  <div class="text-sm font-medium">{t('admin.wholesalePrices.modal.skuReferenceTitle')}</div>
                  <div class="mt-1 text-xs leading-5 text-muted">{t('admin.wholesalePrices.modal.skuReferenceDesc')}</div>
                </div>
              </div>
              <div class="mt-3 grid gap-2">
                {p.skuPriceReferences.value.map((item) => (
                  <div key={item.id} class="grid grid-cols-[minmax(0,1fr)_auto] gap-3 rounded-zs-sm border border-line bg-surface-muted px-3 py-2">
                    <div class="min-w-0">
                      <div class="truncate text-sm font-medium">{item.label}</div>
                      {item.code && <div class="truncate font-mono text-[11px] text-muted">{item.code}</div>}
                    </div>
                    <div class="text-right">
                      <div class="zs-num text-sm">{item.priceText}</div>
                      <div class={['text-[11px]', item.tierApplies === true ? 'text-success-text' : 'text-muted']}>
                        {item.tierApplies === null
                          ? t('admin.wholesalePrices.modal.skuReferencePending')
                          : item.tierApplies
                            ? t('admin.wholesalePrices.modal.skuReferenceApplies')
                            : t('admin.wholesalePrices.modal.skuReferenceNotApplies')}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <div class="text-sm font-medium">{t('admin.wholesalePrices.table.tiers')}</div>
              <Button size="sm" onClick={p.addTier}>
                <Plus class="h-3.5 w-3.5" />
                {t('admin.wholesalePrices.actions.addTier')}
              </Button>
            </div>
            {p.tierForm.value.length === 0 && (
              <div class="rounded-zs border border-dashed border-line-strong p-4 text-xs text-muted">{t('admin.wholesalePrices.modal.empty')}</div>
            )}
            {p.tierForm.value.map((tier, index) => (
              <div key={`tier-${index}`} class="grid grid-cols-1 gap-3 rounded-zs border border-line bg-surface-strong p-4 md:grid-cols-[1.2fr_1fr_1fr_auto] md:items-end">
                <FormField label={t('admin.wholesalePrices.modal.skuScope')}>
                  <Select modelValue={tier.sku_id} onUpdate:modelValue={(v) => (tier.sku_id = String(v))} options={scopeOptions(tier.sku_id)} />
                </FormField>
                <FormField label={t('admin.wholesalePrices.modal.minQuantity')}>
                  <Input type="number" min={1} step={1} v-model={tier.min_quantity} placeholder={t('admin.wholesalePrices.modal.minQuantityPlaceholder')} />
                </FormField>
                <FormField label={t('admin.wholesalePrices.modal.unitPrice')}>
                  <Input type="number" min={0} step="0.01" v-model={tier.unit_price} placeholder={t('admin.wholesalePrices.modal.unitPricePlaceholder')} />
                </FormField>
                <Button variant="danger" onClick={() => p.removeTier(index)}>
                  <Trash2 class="h-3.5 w-3.5" />
                  {t('admin.wholesalePrices.actions.removeTier')}
                </Button>
              </div>
            ))}
          </div>
        </div>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.wholesalePrices.title')} />

        <FilterBar cols={3}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={p.searchQuery.value}
                  placeholder={t('admin.wholesalePrices.searchPlaceholder')}
                  onUpdate:modelValue={debouncedSearch}
                  onEnter={() => void p.list.fetchData(1)}
                />
                <Select
                  modelValue={p.wholesaleStatus.value}
                  onUpdate:modelValue={(v) => {
                    p.wholesaleStatus.value = v === 'enabled' || v === 'disabled' ? v : 'all'
                    void p.list.fetchData(1)
                  }}
                  placeholder={t('admin.wholesalePrices.statusFilter')}
                  options={[
                    { label: t('admin.wholesalePrices.statusAll'), value: 'all' },
                    { label: t('admin.wholesalePrices.statusEnabled'), value: 'enabled' },
                    { label: t('admin.wholesalePrices.statusDisabled'), value: 'disabled' },
                  ]}
                />
                <div>
                  <Button onClick={p.resetFilters}>
                    <RotateCcw class="h-3.5 w-3.5" />
                    {t('admin.common.reset')}
                  </Button>
                </div>
              </>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.wholesalePrices.empty')}
            minWidth="960px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          modelValue={p.showModal.value}
          onUpdate:modelValue={(v) => {
            if (!v) p.closeModal()
          }}
          title={t('admin.wholesalePrices.modal.title')}
          size="2xl"
        >
          {{
            default: renderModalBody,
            footer: () => (
              <>
                <Button onClick={p.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.submitting.value} onClick={p.save}>
                  {p.submitting.value ? t('admin.wholesalePrices.actions.saving') : t('admin.wholesalePrices.actions.save')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
