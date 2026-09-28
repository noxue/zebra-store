import { defineComponent, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { CategorySidebar } from '@/components/product/CategorySidebar'
import { ProductGrid } from '@/components/product/ProductGrid'
import { ProductQuickBuy } from '@/components/product/ProductQuickBuy'
import { useLocalized } from '@/composables/useLocalized'
import { usePageTitle } from '@/composables/usePageTitle'
import { useProductList } from '@/composables/useProductList'
import { useQuickBuy } from '@/composables/useQuickBuy'
import { Button, EmptyState, Pagination, SectionTitle } from '@/components/ui'

/** Product page size (original: 12). */
const PAGE_SIZE = 12

export default defineComponent({
  name: 'Products',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const route = useRoute()
    const { getLocalizedText } = useLocalized()
    const l = useProductList({ pageSize: PAGE_SIZE, homeRouteName: 'products' })
    const quickBuy = useQuickBuy()
    const categoryTitle = () => {
      const cat = l.selectedCategory.value ? l.categoryMap.value.get(l.selectedCategory.value) : undefined
      return cat ? getLocalizedText(cat.name as Record<string, string>) : ''
    }
    usePageTitle(() => (route.name === 'category-products' ? categoryTitle() || t('nav.products') : t('nav.products')))
    onMounted(() => void l.initialize())

    return () => (
      <div class="zs-page space-y-6 py-6">
        <div class="zs-card zs-soft-bg relative overflow-hidden px-6 py-8 sm:px-10">
          <SectionTitle as="h1" size="xl" title={categoryTitle() || t('nav.products')} subtitle={t('products.subtitle')} />
          <div class="pointer-events-none absolute -right-6 -top-10 size-40 rounded-full bg-secondary/20 blur-3xl" />
        </div>
        <div class="flex flex-col gap-6 lg:flex-row">
          <CategorySidebar
            categories={l.categoryGroups.value}
            selectedCategory={l.selectedCategory.value}
            expandedParentIds={l.expandedParentIds.value}
            showDrawer={l.showFilterDrawer.value}
            showSearch
            searchQuery={l.searchQuery.value}
            onSelectCategory={(id: number | null, close?: boolean) => l.selectCategory(id, close)}
            onToggleParent={l.toggleParentCategory}
            onUpdate:showDrawer={(v: boolean) => {
              l.showFilterDrawer.value = v
            }}
            onUpdate:searchQuery={(v: string) => {
              l.searchQuery.value = v
            }}
            onClearSearch={l.clearSearch}
          />
          <main class="min-w-0 flex-1 space-y-6">
            {!l.loading.value && l.products.value.length === 0 ? (
              <div class="zs-card">
                <EmptyState variant={l.hasFilters.value ? 'search' : 'empty'} title={l.hasFilters.value ? t('products.emptyFiltered') : t('products.empty')}>
                  {l.hasFilters.value ? (
                    <Button variant="secondary" onClick={l.clearFilters}>
                      {t('products.clearFilters')}
                    </Button>
                  ) : null}
                </EmptyState>
              </div>
            ) : (
              <ProductGrid
                cols="narrow"
                products={l.products.value}
                loading={l.loading.value}
                skeletonCount={6}
                onOpen={(slug: string) => void router.push(`/products/${slug}`)}
                onQuickBuy={quickBuy.show}
              />
            )}
            <Pagination page={l.currentPage.value} totalPages={l.totalPages.value} onChange={l.changePage} />
          </main>
        </div>
        <ProductQuickBuy product={quickBuy.product.value} open={quickBuy.open.value} onClose={quickBuy.close} />
      </div>
    )
  },
})
