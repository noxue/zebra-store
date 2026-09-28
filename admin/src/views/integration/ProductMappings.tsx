import { defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Download, RefreshCw, Search } from 'lucide-vue-next'
import { Badge, Button, Checkbox, EmptyState, FilterBar, Input, ListPagination, PageHeader, Select, TableSkeleton, cn } from '@/components/ui'
import { formatSpecValues, formatTime, skuPriceDiff } from './integrationUtils'
import { localPriceRange, localProductTitle, useProductMappings, type MappingRow } from './useProductMappings'
import { ImportModal } from './components/ImportModal'

export default defineComponent({
  name: 'ProductMappingsView',
  setup() {
    const { t } = useI18n()
    const p = useProductMappings()
    const showImport = ref(false)
    onMounted(p.init)

    const onImported = (opts: { categoriesChanged: boolean }) => {
      void p.list.fetchData(1)
      if (opts.categoriesChanged) void p.fetchCategories()
    }

    const skuTable = (m: MappingRow) => {
      const skus = m.product?.skus || []
      const rate = p.exchangeRate(m.connection_id)
      const th = 'px-3 py-2.5 font-medium'
      return (
        <div class="overflow-x-auto rounded-zs border border-line">
          <table class="w-full min-w-[860px] text-xs">
            <thead>
              <tr class="bg-surface-muted text-muted">
                <th class={cn(th, 'min-w-[160px] text-left')}>{t('productMappings.detail.skuCode')}</th>
                <th class={cn(th, 'min-w-[220px] text-left')}>{t('productMappings.import.skuSpec')}</th>
                <th class={cn(th, 'text-right')}>{t('productMappings.detail.localPrice')}</th>
                <th class={cn(th, 'text-right')}>{t('productMappings.detail.upstreamPrice')}</th>
                <th class={cn(th, 'text-right')}>{t('productMappings.detail.costPrice')}</th>
                <th class={cn(th, 'text-center')}>{t('productMappings.detail.priceDiff')}</th>
                <th class={cn(th, 'text-center')}>{t('productMappings.detail.upstreamStock')}</th>
                <th class={cn(th, 'text-center')}>{t('productMappings.detail.upstreamActive')}</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-line">
              {skus.length === 0 && (
                <tr>
                  <td colspan={8} class="px-3 py-6 text-center text-muted">
                    {t('productMappings.detail.noSkus')}
                  </td>
                </tr>
              )}
              {skus.map((sku) => {
                const sm = p.skuMappingByLocalId.value.get(sku.id)
                const calc = sm ? skuPriceDiff(sku.price_amount, sm.upstream_price, rate) : null
                const dash = <span class="text-muted">-</span>
                const stock = sm?.upstream_stock ?? 0
                return (
                  <tr key={sku.id} class="hover:bg-primary-soft/40">
                    <td class="break-all px-3 py-2.5 font-mono text-muted">{sku.sku_code}</td>
                    <td class="break-words px-3 py-2.5 text-fg">{formatSpecValues(sku.spec_values)}</td>
                    <td class="zs-num px-3 py-2.5 text-right font-mono text-fg">{sku.price_amount}</td>
                    <td class="zs-num px-3 py-2.5 text-right font-mono">
                      {sm ? (
                        <>
                          <span class="text-fg">{sm.upstream_price}</span>
                          {rate !== 1 && <span class="ml-0.5 text-[10px] text-muted">×{rate}</span>}
                        </>
                      ) : (
                        dash
                      )}
                    </td>
                    <td class="zs-num px-3 py-2.5 text-right font-mono text-fg">{calc ? calc.cost.toFixed(2) : dash}</td>
                    <td class="px-3 py-2.5 text-center">
                      {calc ? (
                        calc.equal ? (
                          <span class="text-success-text">-</span>
                        ) : (
                          <Badge tone={calc.diff > 0 ? 'success' : 'danger'}>
                            {calc.diff > 0 ? '+' : ''}
                            {calc.diff.toFixed(2)}
                          </Badge>
                        )
                      ) : (
                        dash
                      )}
                    </td>
                    <td class="px-3 py-2.5 text-center">
                      {sm ? (
                        <Badge tone={stock !== 0 ? 'success' : 'danger'}>
                          {stock !== 0 ? (stock < 0 ? t('productMappings.import.unlimited') : t('productMappings.import.inStock')) : t('productMappings.import.outOfStock')}
                        </Badge>
                      ) : (
                        dash
                      )}
                    </td>
                    <td class="px-3 py-2.5 text-center">
                      {sm ? <span class={cn('inline-block h-2 w-2 rounded-full', sm.upstream_is_active ? 'bg-success' : 'bg-line-strong')} /> : dash}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )
    }

    const expandedDetail = (m: MappingRow) => (
      <div class="border-t border-line bg-surface-muted/60">
        {p.detailLoading.value ? (
          <div class="px-6 py-8 text-center text-sm text-muted">{t('admin.common.loading')}</div>
        ) : !p.detail.value ? (
          <div class="px-6 py-8 text-center text-sm text-muted">{t('productMappings.detail.loadFailed')}</div>
        ) : (
          <div class="px-5 py-4">
            <h4 class="mb-3 text-xs font-semibold uppercase tracking-wider text-muted">{t('productMappings.detail.skuComparison')}</h4>
            {skuTable(m)}
            <div class="mt-4 flex flex-wrap gap-x-6 gap-y-1 text-xs text-muted">
              <span>
                {t('productMappings.detail.mappingId')}: <span class="font-mono text-fg">{m.id}</span>
              </span>
              <span>
                {t('productMappings.detail.createdAt')}: {formatTime(m.created_at)}
              </span>
              <span>
                {t('productMappings.detail.updatedAt')}: {formatTime(m.updated_at)}
              </span>
              {m.product?.fulfillment_type && (
                <span>
                  {t('productMappings.detail.fulfillmentType')}: <span class="text-fg">{m.product.fulfillment_type}</span>
                </span>
              )}
            </div>
          </div>
        )}
      </div>
    )

    const mappingCard = (m: MappingRow) => {
      const open = p.expandedId.value === m.id
      return (
        <div key={m.id} class={cn('zs-glass overflow-hidden rounded-zs-lg transition-shadow', open ? 'shadow-zs ring-1 ring-primary/30' : 'shadow-zs-sm')}>
          <div class="flex cursor-pointer flex-col gap-3 px-5 py-4 transition-colors hover:bg-primary-soft/40 sm:flex-row sm:items-center sm:gap-4" onClick={() => p.toggleExpand(m)}>
            <div class="flex items-center gap-3" onClick={(e: Event) => e.stopPropagation()}>
              <Checkbox modelValue={p.selected.value.has(m.id)} onUpdate:modelValue={() => p.toggleSelect(m.id)} />
              <ChevronRight
                class={cn('h-4 w-4 shrink-0 cursor-pointer text-muted transition-transform', open && 'rotate-90 text-primary')}
                onClick={() => p.toggleExpand(m)}
              />
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <span class="break-words text-sm font-semibold text-fg sm:truncate">{localProductTitle(m)}</span>
                <span class="shrink-0 font-mono text-[10px] text-muted">#{m.local_product_id}</span>
                <Badge tone={m.is_active ? 'success' : 'neutral'} dot>
                  {m.is_active ? t('productMappings.status.active') : t('productMappings.status.inactive')}
                </Badge>
                {m.upstream_status === 'inactive' && <Badge tone="warning">{t('productMappings.upstreamStatus.inactive')}</Badge>}
                {m.upstream_status === 'deleted' && <Badge tone="danger">{t('productMappings.upstreamStatus.deleted')}</Badge>}
              </div>
              <div class="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted">
                <span>
                  {t('productMappings.columns.connection')}: <span class="text-fg">{p.connectionName(m.connection_id)}</span>
                </span>
                <span>
                  {t('productMappings.detail.upstreamId')}: <span class="font-mono text-fg">{m.upstream_product_id}</span>
                </span>
                <span>
                  {t('productMappings.detail.localPrice')}: <span class="zs-num font-mono text-fg">{localPriceRange(m)}</span>
                </span>
                <span>
                  SKU: <span class="text-fg">{m.product?.skus?.length || 0}</span>
                </span>
                <span>
                  {t('productMappings.columns.lastSynced')}: {formatTime(m.last_synced_at ?? m.last_sync_at)}
                </span>
              </div>
            </div>
            <div class="flex w-full shrink-0 flex-col gap-2 sm:w-auto sm:flex-row sm:items-center" onClick={(e: Event) => e.stopPropagation()}>
              <Button size="sm" loading={p.syncingId.value === m.id} onClick={() => p.sync(m)}>
                {p.syncingId.value === m.id ? t('productMappings.actions.syncing') : t('productMappings.actions.sync')}
              </Button>
              <Button size="sm" onClick={() => p.toggleStatus(m)}>
                {m.is_active ? t('productMappings.actions.disable') : t('productMappings.actions.enable')}
              </Button>
              <Button size="sm" variant="danger" onClick={() => p.remove(m)}>
                {t('admin.common.delete')}
              </Button>
            </div>
          </div>
          {open && expandedDetail(m)}
        </div>
      )
    }

    const listBody = () => {
      const items = p.list.items.value
      if (p.list.loading.value && items.length === 0)
        return (
          <div class="zs-glass overflow-hidden rounded-zs-lg">
            <TableSkeleton cols={5} rows={5} />
          </div>
        )
      if (items.length === 0)
        return (
          <div class="zs-glass rounded-zs-lg">
            <EmptyState title={t('productMappings.empty')} />
          </div>
        )
      return (
        <>
          <div class="flex items-center gap-2 px-1">
            <Checkbox modelValue={p.allSelected.value} indeterminate={p.someSelected.value} onUpdate:modelValue={p.toggleAll} label={t('productMappings.batch.selectAll')} />
          </div>
          {items.map(mappingCard)}
        </>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('productMappings.title')}>
          {{
            actions: () => (
              <>
                <Button loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-4 w-4" />
                  {t('productMappings.refresh')}
                </Button>
                <Button variant="primary" onClick={() => (showImport.value = true)}>
                  <Download class="h-4 w-4" />
                  {t('productMappings.importButton')}
                </Button>
              </>
            ),
          }}
        </PageHeader>

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Select
                  v-model={p.filters.connection_id}
                  onChange={p.list.handleSearch}
                  placeholder={t('productMappings.filter.connectionPlaceholder')}
                  options={[{ label: t('productMappings.filter.allConnections'), value: '__all__' }, ...p.connections.value.map((c) => ({ label: c.name || `#${c.id}`, value: c.id }))]}
                />
                <Input icon={Search} v-model={p.filters.search} placeholder={t('productMappings.filter.searchPlaceholder')} onUpdate:modelValue={p.list.debouncedSearch} onEnter={p.list.handleSearch} />
                <Select
                  v-model={p.filters.upstream_status}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('productMappings.filter.allUpstreamStatus'), value: '__all__' },
                    { label: t('productMappings.filter.upstreamInactive'), value: 'inactive' },
                    { label: t('productMappings.filter.upstreamDeleted'), value: 'deleted' },
                  ]}
                />
                <Select
                  v-model={p.filters.product_status}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('productMappings.filter.allProductStatus'), value: '__all__' },
                    { label: t('productMappings.filter.productActive'), value: 'active' },
                    { label: t('productMappings.filter.productInactive'), value: 'inactive' },
                  ]}
                />
              </>
            ),
          }}
        </FilterBar>

        {p.selected.value.size > 0 && (
          <div class="flex flex-wrap items-center gap-3 rounded-zs-lg border border-primary/30 bg-primary-soft px-4 py-3">
            <span class="text-sm font-medium text-fg">{t('productMappings.batch.selected', { count: p.selected.value.size })}</span>
            <div class="flex flex-wrap gap-2">
              <Button size="sm" disabled={p.batchOperating.value} onClick={p.batchSync}>
                {t('productMappings.batch.sync')}
              </Button>
              <Button size="sm" disabled={p.batchOperating.value} onClick={() => p.batchStatus(true)}>
                {t('productMappings.batch.enable')}
              </Button>
              <Button size="sm" disabled={p.batchOperating.value} onClick={() => p.batchStatus(false)}>
                {t('productMappings.batch.disable')}
              </Button>
              <Button size="sm" variant="danger" disabled={p.batchOperating.value} onClick={p.batchDelete}>
                {t('productMappings.batch.delete')}
              </Button>
            </div>
            <button type="button" class="ml-auto text-xs text-muted hover:text-primary" onClick={p.clearSelection}>
              {t('productMappings.batch.clearSelection')}
            </button>
          </div>
        )}

        <div class="space-y-3">
          {listBody()}
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <ImportModal v-model={showImport.value} connections={p.connections.value} categories={p.categories.value} onImported={onImported} />
      </div>
    )
  },
})
