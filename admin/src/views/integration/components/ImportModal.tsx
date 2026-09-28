import { defineComponent, toRef, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, ChevronRight } from 'lucide-vue-next'
import { Badge, Button, Checkbox, Dialog, FormField, Select, Switch, Tabs, cn } from '@/components/ui'
import type { AdminCategory, AdminSiteConnection } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { formatSpecValues, isSkuAvailable, skuStockLevel, upstreamPriceRange, type UpstreamProduct } from '../integrationUtils'
import { useImportModal } from '../useImportModal'

/** "Import from upstream" dialog of 商品映射. */
export const ImportModal = defineComponent({
  name: 'ImportModal',
  props: {
    modelValue: Boolean,
    connections: { type: Array as PropType<AdminSiteConnection[]>, default: () => [] },
    categories: { type: Array as PropType<AdminCategory[]>, default: () => [] },
  },
  emits: {
    'update:modelValue': (_v: boolean) => true,
    imported: (_opts: { categoriesChanged: boolean }) => true,
  },
  setup(props, { emit }) {
    const { t } = useI18n()
    const close = () => emit('update:modelValue', false)
    const m = useImportModal({
      categories: toRef(props, 'categories'),
      onImported: (opts) => emit('imported', opts),
      onClose: close,
    })
    watch(
      () => props.modelValue,
      (open) => open && m.reset(),
    )

    const stockText = (p: UpstreamProduct) => {
      const s = skuStockLevel(p)
      if (s.level === 'none') return { text: '-', cls: 'text-muted' }
      if (s.level === 'all') return { text: t('productMappings.import.stockAllInStock'), cls: 'text-success-text' }
      if (s.level === 'zero') return { text: t('productMappings.import.stockAllOutOfStock'), cls: 'text-danger-text' }
      return { text: t('productMappings.import.stockPartial', { inStock: s.inStock, total: s.total }), cls: 'text-warning-text' }
    }

    const placeholder = (text: string) => <div class="px-6 py-12 text-center text-sm text-muted">{text}</div>

    const loadMoreButton = () =>
      m.hasMore.value && (
        <div class="border-t border-line px-4 py-3 text-center">
          <Button size="sm" variant="ghost" loading={m.loadingMore.value} onClick={m.loadMore}>
            {m.loadingMore.value ? t('productMappings.import.loadingMore') : t('productMappings.import.loadMore', { remaining: m.remaining.value })}
          </Button>
        </div>
      )

    const productCheckbox = (p: UpstreamProduct) => (
      <div onClick={(e: Event) => e.stopPropagation()}>
        <Checkbox modelValue={m.selectedIds.value.has(p.id)} disabled={m.mappedIds.value.has(p.id)} onUpdate:modelValue={() => m.toggleProduct(p.id)} />
      </div>
    )

    const productMeta = (p: UpstreamProduct, withPriceLabel: boolean) => {
      const stock = stockText(p)
      return (
        <div class="mt-1 flex flex-wrap items-center gap-x-4 gap-y-0.5 text-xs text-muted">
          <span>
            {withPriceLabel && `${t('productMappings.import.colPrice')}: `}
            <span class="zs-num font-mono text-fg">{upstreamPriceRange(p)}</span>
            {p.currency && <span class="ml-0.5">{p.currency}</span>}
          </span>
          <span>
            SKU: <span class="text-fg">{p.skus?.length || 0}</span>
          </span>
          <span class={stock.cls}>{stock.text}</span>
        </div>
      )
    }

    const mappedBadge = (p: UpstreamProduct) =>
      m.mappedIds.value.has(p.id) && (
        <Badge tone="warning" class="shrink-0">
          {t('productMappings.import.alreadyMapped')}
        </Badge>
      )

    const rowClass = (p: UpstreamProduct) =>
      cn('transition-colors', m.selectedIds.value.has(p.id) && 'bg-primary-soft', m.mappedIds.value.has(p.id) && 'opacity-50')
    const clickClass = (p: UpstreamProduct) => (m.mappedIds.value.has(p.id) ? 'cursor-not-allowed' : 'cursor-pointer hover:bg-surface-muted')

    const skuGrid = (p: UpstreamProduct) => (
      <div class="border-t border-line bg-surface-muted px-4 py-2">
        <div class="ml-7 overflow-x-auto">
          <div class="grid min-w-[640px] grid-cols-[1fr_auto_auto_auto] gap-x-4 text-xs">
            <div class="border-b border-line py-1.5 font-medium text-muted">{t('productMappings.import.skuSpec')}</div>
            <div class="border-b border-line py-1.5 text-right font-medium text-muted">{t('productMappings.import.skuPrice')}</div>
            <div class="border-b border-line py-1.5 text-center font-medium text-muted">{t('productMappings.import.skuStock')}</div>
            <div class="border-b border-line py-1.5 text-center font-medium text-muted">{t('productMappings.import.skuActive')}</div>
            {(p.skus || []).map((sku) => [
              <div key={`s${sku.id}`} class="py-1.5 text-fg">
                {sku.sku_code && <span class="mr-2 break-all font-mono text-muted">{sku.sku_code}</span>}
                <span>{formatSpecValues(sku.spec_values)}</span>
              </div>,
              <div key={`p${sku.id}`} class="zs-num py-1.5 text-right font-mono text-fg">
                {sku.price_amount}
                {p.currency && <span class="ml-0.5 text-muted">{p.currency}</span>}
              </div>,
              <div key={`k${sku.id}`} class="py-1.5 text-center">
                <Badge tone={isSkuAvailable(sku.stock_status) ? 'success' : 'danger'}>
                  {isSkuAvailable(sku.stock_status) ? t('productMappings.import.inStock') : t('productMappings.import.outOfStock')}
                </Badge>
              </div>,
              <div key={`a${sku.id}`} class="py-1.5 text-center">
                <span class={cn('inline-block h-2 w-2 rounded-full', sku.is_active ? 'bg-success' : 'bg-line-strong')} />
              </div>,
            ])}
          </div>
        </div>
      </div>
    )

    const flatView = () => {
      if (!m.connectionId.value) return placeholder(t('productMappings.import.selectConnectionFirst'))
      if (m.loading.value) return placeholder(t('productMappings.import.upstreamProductLoading'))
      if (m.products.value.length === 0) return placeholder(t('productMappings.import.noUpstreamProducts'))
      return (
        <div>
          <div class="flex items-center gap-3 border-b border-line bg-surface-muted px-4 py-2.5">
            <Checkbox modelValue={m.allSelected.value} indeterminate={!m.allSelected.value && m.selectedIds.value.size > 0} onUpdate:modelValue={m.toggleSelectAll} />
            <span class="text-xs font-medium text-muted">
              {t('productMappings.import.selectAll')} ({m.products.value.length}/{m.total.value})
            </span>
          </div>
          <div class="max-h-[50vh] divide-y divide-line overflow-y-auto">
            {m.products.value.map((p) => (
              <div key={p.id} class={rowClass(p)}>
                <div class={cn('flex items-center gap-3 px-4 py-3', clickClass(p))} onClick={() => m.toggleProduct(p.id)}>
                  {productCheckbox(p)}
                  <div class="min-w-0 flex-1">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="break-words text-sm font-medium text-fg sm:truncate">{getLocalizedText(p.title)}</span>
                      <span class="shrink-0 font-mono text-[10px] text-muted">#{p.id}</span>
                      <Badge tone={p.is_active ? 'success' : 'neutral'} class="shrink-0">
                        {p.is_active ? t('productMappings.status.active') : t('productMappings.status.inactive')}
                      </Badge>
                      {mappedBadge(p)}
                    </div>
                    {productMeta(p, true)}
                  </div>
                  {p.skus && p.skus.length > 0 && (
                    <button
                      type="button"
                      class="shrink-0 rounded-zs-sm p-1 text-muted transition-colors hover:bg-primary-soft hover:text-primary"
                      title={t('productMappings.import.toggleSkuDetails')}
                      onClick={(e: Event) => {
                        e.stopPropagation()
                        m.toggleSkuExpand(p.id)
                      }}
                    >
                      <ChevronDown class={cn('h-4 w-4 transition-transform', m.expandedIds.value.has(p.id) && 'rotate-180')} />
                    </button>
                  )}
                </div>
                {m.expandedIds.value.has(p.id) && p.skus && p.skus.length > 0 && skuGrid(p)}
              </div>
            ))}
            {loadMoreButton()}
          </div>
        </div>
      )
    }

    const categoryView = () => {
      if (!m.connectionId.value) return placeholder(t('productMappings.import.selectConnectionFirst'))
      if (m.loading.value || m.loadingCategories.value) return placeholder(t('productMappings.import.upstreamProductLoading'))
      if (m.categoryDisplayList.value.length === 0) return placeholder(t('productMappings.import.noUpstreamProducts'))
      return (
        <div class="max-h-[55vh] divide-y divide-line overflow-y-auto">
          {m.categoryDisplayList.value.map((item) => {
            const catId = item.category.id
            const open = m.expandedCategoryIds.value.has(catId)
            return (
              <div key={catId}>
                <div class="flex cursor-pointer items-center gap-3 bg-surface-muted px-4 py-3 transition-colors hover:bg-primary-soft" onClick={() => m.toggleCategoryExpand(catId)}>
                  <ChevronRight class={cn('h-4 w-4 shrink-0 text-muted transition-transform', open && 'rotate-90')} />
                  <div class="min-w-0 flex-1">
                    <span class="text-sm font-medium text-fg">{item.path}</span>
                    <span class="ml-2 text-xs text-muted">{t('productMappings.import.productsCount', { count: item.productCount })}</span>
                    {item.nonMappedCount < item.productCount && (
                      <span class="ml-1 text-xs text-warning-text">
                        ({item.productCount - item.nonMappedCount} {t('productMappings.import.alreadyMapped')})
                      </span>
                    )}
                  </div>
                  <div class="flex shrink-0 items-center gap-2" onClick={(e: Event) => e.stopPropagation()}>
                    <Checkbox modelValue={m.isCategoryAllSelected(catId)} onUpdate:modelValue={(v) => m.selectAllInCategory(catId, v)} />
                    <Button size="xs" disabled={m.categoryImporting.value || item.nonMappedCount === 0} onClick={() => m.importCategory(catId)}>
                      {t('productMappings.import.importCategory')}
                    </Button>
                  </div>
                </div>
                {open &&
                  (m.byCategory.value.get(catId) || []).map((p) => (
                    <div key={p.id} class={cn(rowClass(p), 'border-t border-line')}>
                      <div class={cn('flex items-center gap-3 py-2.5 pl-11 pr-4', clickClass(p))} onClick={() => m.toggleProduct(p.id)}>
                        {productCheckbox(p)}
                        <div class="min-w-0 flex-1">
                          <div class="flex flex-wrap items-center gap-2">
                            <span class="break-words text-sm text-fg sm:truncate">{getLocalizedText(p.title)}</span>
                            <span class="shrink-0 font-mono text-[10px] text-muted">#{p.id}</span>
                            {mappedBadge(p)}
                          </div>
                          {productMeta(p, false)}
                        </div>
                      </div>
                    </div>
                  ))}
              </div>
            )
          })}
          {loadMoreButton()}
        </div>
      )
    }

    return () => (
      <Dialog modelValue={props.modelValue} onUpdate:modelValue={(v) => emit('update:modelValue', v)} title={t('productMappings.importTitle')} size="3xl" closeOnOverlay={false}>
        {{
          default: () => (
            <div class="space-y-5">
              <div class="flex flex-col gap-4 lg:flex-row lg:items-end">
                <div class="min-w-[200px] flex-1">
                  <FormField label={t('productMappings.import.selectConnection')}>
                    <Select
                      v-model={m.connectionId.value}
                      placeholder={t('productMappings.import.selectConnectionPlaceholder')}
                      options={props.connections.map((c) => ({ label: c.name || `#${c.id}`, value: c.id }))}
                    />
                  </FormField>
                </div>
                {m.connectionId.value !== '' && m.categoriesSupported.value && (
                  <Tabs
                    modelValue={m.viewMode.value}
                    onUpdate:modelValue={(v) => (m.viewMode.value = v === 'flat' ? 'flat' : 'category')}
                    items={[
                      { key: 'category', label: t('productMappings.import.byCategory') },
                      { key: 'flat', label: t('productMappings.import.flatList') },
                    ]}
                  />
                )}
              </div>

              {m.connectionId.value !== '' && (
                <div class="flex flex-col gap-4 lg:flex-row lg:items-end">
                  {m.showCategoryView.value && (
                    <div class="pb-2">
                      <Switch v-model={m.autoCreateCategory.value} label={t('productMappings.import.autoCreateCategory')} />
                    </div>
                  )}
                  {(!m.autoCreateCategory.value || m.viewMode.value === 'flat') && (
                    <div class="min-w-[200px] flex-1">
                      <FormField label={t('productMappings.import.category')}>
                        <Select v-model={m.categoryId.value} placeholder={t('productMappings.import.categoryPlaceholder')} options={m.categoryOptions.value} />
                      </FormField>
                    </div>
                  )}
                </div>
              )}

              <div class="overflow-hidden rounded-zs border border-line">{m.showCategoryView.value ? categoryView() : flatView()}</div>
            </div>
          ),
          footer: () => (
            <div class="flex w-full flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span class="text-sm text-muted">{m.selectedIds.value.size > 0 ? t('productMappings.import.selectedCount', { count: m.selectedIds.value.size }) : ''}</span>
              <div class="flex flex-col-reverse gap-3 sm:flex-row">
                <Button onClick={close}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={m.importing.value} disabled={m.selectedIds.value.size === 0 || !m.connectionId.value || m.importing.value} onClick={m.batchImport}>
                  {m.importing.value
                    ? t('productMappings.import.importingProgress', { done: m.progress.value.done, total: m.progress.value.total, success: m.progress.value.success })
                    : t('productMappings.import.submitBatch', { count: m.selectedIds.value.size })}
                </Button>
              </div>
            </div>
          ),
        }}
      </Dialog>
    )
  },
})

export default ImportModal
