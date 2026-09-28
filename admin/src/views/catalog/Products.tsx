import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, RotateCcw, Search } from 'lucide-vue-next'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, Select, type DataTableColumn } from '@/components/ui'
import type { AdminProduct } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { getFirstImageUrl } from '@/utils/image'
import { toRowSelection } from '@/composables/useSelection'
import ProductEditModal from './components/ProductEditModal'
import { PRODUCT_PAGE_SIZE_OPTIONS, useProducts } from './useProducts'

export default defineComponent({
  name: 'ProductsView',
  setup() {
    const { t } = useI18n()
    const p = useProducts()
    onMounted(p.init)

    const pill = 'inline-flex max-w-full cursor-pointer break-words rounded-full border border-line px-2.5 py-1 text-xs leading-5 text-muted transition-colors hover:border-primary hover:text-primary'

    const renderName = (r: AdminProduct) => {
      const img = getFirstImageUrl(r.images)
      const manual = r.fulfillment_type === 'manual' ? p.manualStockSummary(r) : null
      const auto = r.fulfillment_type === 'auto' ? p.autoStockSummary(r) : null
      return (
        <div class="flex min-w-[300px] items-center gap-4">
          <div class="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-zs-sm border border-line bg-surface-muted text-[10px] text-muted">
            {img ? <img src={img} alt="" class="h-full w-full object-cover" /> : <span>{t('admin.common.noImage')}</span>}
          </div>
          <div class="min-w-0 flex-1">
            <div class="break-words font-medium text-fg">{getLocalizedText(r.title)}</div>
            <div class="break-all font-mono text-xs text-muted">{r.slug}</div>
            <div class="mt-2 flex flex-wrap gap-1.5">
              <Badge tone={r.purchase_type === 'guest' ? 'info' : 'secondary'}>
                {r.purchase_type === 'guest' ? t('admin.products.purchaseType.guest') : t('admin.products.purchaseType.member')}
              </Badge>
              <Badge tone={r.fulfillment_type === 'auto' ? 'primary' : 'warning'}>
                {r.fulfillment_type === 'auto' ? t('admin.products.fulfillmentType.auto') : t('admin.products.fulfillmentType.manual')}
              </Badge>
              {r.is_mapped && <Badge tone="secondary">{t('admin.products.mappedProduct')}</Badge>}
              <Badge tone={r.is_affiliate_enabled ? 'info' : 'neutral'}>
                {r.is_affiliate_enabled ? t('admin.products.affiliate.enabled') : t('admin.products.affiliate.disabled')}
              </Badge>
              {manual && <Badge tone={manual.tone}>{manual.text}</Badge>}
              {auto && <Badge tone={auto.tone}>{auto.text}</Badge>}
              {(r.tags || []).slice(0, 3).map((tag, i) => (
                <Badge key={i} tone="neutral">
                  # {tag}
                </Badge>
              ))}
            </div>
          </div>
        </div>
      )
    }

    const columns = (): DataTableColumn<AdminProduct>[] => [
      { key: 'id', title: t('admin.products.table.id'), render: (r) => <IdCell value={r.id} /> },
      { key: 'name', title: t('admin.products.table.name'), class: 'min-w-[320px]', render: renderName },
      {
        key: 'price',
        title: t('admin.products.table.price'),
        render: (r) => {
          const wholesale = p.formatWholesaleSummary(r)
          return (
            <div>
              <div class="zs-num whitespace-nowrap font-semibold text-fg">{p.formatPrice(r.price_amount)}</div>
              {wholesale && <div class="mt-1 text-xs text-success-text">{wholesale}</div>}
            </div>
          )
        },
      },
      {
        key: 'category',
        title: t('admin.products.table.category'),
        class: 'min-w-[200px]',
        render: (r) =>
          p.editingCategoryId.value === r.id ? (
            <div class="min-w-[160px]" onFocusout={() => setTimeout(() => p.editingCategoryId.value === r.id && p.cancelEditCategory(), 150)}>
              <Select
                size="sm"
                modelValue={r.category_id || ''}
                placeholder={t('admin.products.uncategorized')}
                options={p.categoryOptions.value}
                onChange={(v) => void p.saveCategory(r, v)}
              />
            </div>
          ) : (
            <span class={pill} title={t('admin.common.clickToEdit')} onClick={() => p.startEditCategory(r)}>
              {r.category ? p.categoryLabel(r.category) : t('admin.products.uncategorized')}
            </span>
          ),
      },
      {
        key: 'sort',
        title: t('admin.products.table.sort'),
        render: (r) =>
          p.editingSortId.value === r.id ? (
            <div class="w-20">
              <Input
                id={`sort-input-${r.id}`}
                size="sm"
                type="number"
                min={0}
                v-model={p.editingSortValue.value}
                onEnter={() => void p.saveSort(r)}
                onKeydown={(e: KeyboardEvent) => e.key === 'Escape' && p.cancelEditSort()}
                onBlur={() => void p.saveSort(r)}
              />
            </div>
          ) : (
            <span class={[pill, 'zs-num']} title={t('admin.common.clickToEdit')} onClick={() => p.startEditSort(r)}>
              {r.sort_order || 0}
            </span>
          ),
      },
      {
        key: 'status',
        title: t('admin.products.table.status'),
        render: (r) => (
          <button
            type="button"
            title={r.is_active ? t('admin.products.status.clickToDeactivate') : t('admin.products.status.clickToActivate')}
            onClick={() => void p.toggleStatus(r)}
          >
            <Badge tone={r.is_active ? 'success' : 'neutral'} dot>
              {r.is_active ? t('admin.products.status.active') : t('admin.products.status.inactive')}
            </Badge>
          </button>
        ),
      },
      {
        key: 'action',
        title: t('admin.products.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex flex-wrap justify-end gap-2">
            <Button size="sm" onClick={() => p.openEditById(r.id)}>
              {t('admin.products.actions.edit')}
            </Button>
            <Button size="sm" variant="danger" onClick={() => void p.remove(r)}>
              {t('admin.products.actions.delete')}
            </Button>
          </div>
        ),
      },
    ]

    const renderBatchBar = () => {
      const count = p.selection.selectedIds.value.length
      if (!count) return null
      const busy = p.batchOperating.value
      return (
        <div class="flex flex-wrap items-center gap-3 rounded-zs border border-line bg-primary-soft px-4 py-3">
          <span class="text-sm font-medium text-primary">{t('admin.products.batch.selected', { count })}</span>
          <div class="flex flex-wrap items-center gap-2">
            <Button size="sm" disabled={busy} onClick={() => void p.batchStatus(true)}>
              {t('admin.products.batch.activate')}
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void p.batchStatus(false)}>
              {t('admin.products.batch.deactivate')}
            </Button>
            <div class="flex items-center gap-1">
              <div class="w-44">
                <Select size="sm" v-model={p.batchCategoryId.value} placeholder={t('admin.products.batch.categoryPlaceholder')} options={p.categoryOptions.value} />
              </div>
              <Button size="sm" disabled={busy || !p.batchCategoryId.value} onClick={() => void p.batchCategory()}>
                {t('admin.products.batch.moveCategory')}
              </Button>
            </div>
            <Button size="sm" variant="danger" disabled={busy} onClick={() => void p.batchDelete()}>
              {t('admin.products.batch.delete')}
            </Button>
          </div>
          <button type="button" class="ml-auto text-xs text-muted hover:text-fg" onClick={p.selection.clear}>
            {t('admin.products.batch.clearSelection')}
          </button>
        </div>
      )
    }

    return () => {
      const sel = p.selection
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.products.title')}>
            {{
              actions: () => (
                <Button variant="primary" onClick={p.openCreate}>
                  <Plus class="h-4 w-4" />
                  {t('admin.products.create')}
                </Button>
              ),
            }}
          </PageHeader>

          <FilterBar cols={5}>
            {{
              default: () => (
                <>
                  <Input
                    id="admin-products-search"
                    icon={Search}
                    v-model={p.filters.search}
                    placeholder={t('admin.products.searchPlaceholder')}
                    onUpdate:modelValue={p.list.debouncedSearch}
                    onEnter={p.list.handleSearch}
                  />
                  <Select
                    v-model={p.filters.stockStatus}
                    onChange={p.list.handleSearch}
                    options={[
                      { value: 'all', label: t('admin.products.filters.stockStatusAll') },
                      { value: 'low', label: t('admin.products.stockStatus.low') },
                      { value: 'normal', label: t('admin.products.stockStatus.normal') },
                      { value: 'unlimited', label: t('admin.products.stockStatus.unlimited') },
                    ]}
                  />
                  <Select
                    v-model={p.filters.category}
                    onChange={p.list.handleSearch}
                    options={[{ value: 'all', label: t('admin.products.filters.categoryAll') }, ...p.categoryOptions.value]}
                  />
                  <Select
                    v-model={p.filters.status}
                    onChange={p.list.handleSearch}
                    options={[
                      { value: 'all', label: t('admin.products.filters.statusAll') },
                      { value: 'active', label: t('admin.products.filters.statusActive') },
                      { value: 'inactive', label: t('admin.products.filters.statusInactive') },
                    ]}
                  />
                  <Select
                    v-model={p.filters.wholesale}
                    onChange={p.handleWholesaleChange}
                    options={[
                      { value: 'all', label: t('admin.products.filters.wholesaleAll') },
                      { value: 'enabled', label: t('admin.products.filters.wholesaleEnabled') },
                      { value: 'disabled', label: t('admin.products.filters.wholesaleDisabled') },
                    ]}
                  />
                </>
              ),
              actions: () => (
                <Button size="sm" onClick={p.resetFilters}>
                  <RotateCcw class="h-3.5 w-3.5" />
                  {t('admin.common.reset')}
                </Button>
              ),
            }}
          </FilterBar>

          {renderBatchBar()}

          <div>
            <DataTable
              columns={columns()}
              rows={p.rows.value}
              rowKey={(r) => r.id}
              loading={p.list.loading.value}
              emptyText={t('admin.products.empty')}
              minWidth="1000px"
              selection={toRowSelection(sel)}
            />
            <ListPagination
              pagination={p.list.pagination.value}
              pageSizeOptions={PRODUCT_PAGE_SIZE_OPTIONS}
              onChangePage={p.list.changePage}
              onChangePageSize={p.list.changePageSize}
            />
          </div>

          <ProductEditModal
            v-model={p.showModal.value}
            productId={p.editingProductId.value}
            categories={p.orderedCategories.value}
            onSuccess={() => void p.list.refresh()}
          />
        </div>
      )
    }
  },
})
