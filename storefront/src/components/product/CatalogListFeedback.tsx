import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import type { useProductList } from '@/composables/useProductList'
import { Button, EmptyState } from '@/components/ui'
export const CatalogListFeedback = defineComponent({
  name: 'CatalogListFeedback',
  props: { list: { type: Object as PropType<ReturnType<typeof useProductList>>, required: true } },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const l = props.list
      if (l.loading.value || (!l.error.value && !l.blocked.value)) return null
      return <div class="zs-card" role="alert" data-testid="catalog-list-feedback"><EmptyState variant="error" title={t(l.error.value ? `catalogBrowse.errors.${l.error.value}` : l.categoryError.value ? 'catalogBrowse.categoriesFailed' : l.routeQueryError.value || l.facets.routeError.value ? 'catalogBrowse.invalidLink' : 'catalogBrowse.unsupported')}>
        {(l.error.value || l.facets.capabilityError.value) && <Button variant="secondary" onClick={() => void (l.facets.capabilityError.value ? l.retryFilters() : l.loadProducts())}>{t('catalogBrowse.retry')}</Button>}
        {l.categoryError.value && <Button variant="secondary" onClick={() => void l.retryCategories()}>{t('catalogBrowse.retryCategories')}</Button>}
        {l.hasFilters.value && <Button variant="secondary" onClick={l.clearFilters}>{t('products.clearFilters')}</Button>}
      </EmptyState></div>
    }
  },
})
