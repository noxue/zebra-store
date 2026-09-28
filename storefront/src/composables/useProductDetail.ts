import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { productAPI } from '@/api/catalog'
import type { Product, RelatedPost } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { getImageUrl } from '@/utils/image'
import { useLocalized } from './useLocalized'
import { usePageTitle } from './usePageTitle'
import { useProductPurchase } from './useProductPurchase'

/** Product detail page: loading + purchase engine + display helpers. */
export function useProductDetail() {
  const route = useRoute()
  const { t } = useI18n()
  const appStore = useAppStore()
  const { getLocalizedText } = useLocalized()

  const loading = ref(true)
  const error = ref(false)
  const product = ref<Product | null>(null)
  const currentImage = ref('')

  const purchase = useProductPurchase(product)

  const images = computed(() => (product.value?.images || []).map((img) => getImageUrl(img)).filter(Boolean))
  const title = computed(() => (product.value ? getLocalizedText(product.value.title) : ''))
  const description = computed(() => getLocalizedText(product.value?.description))
  const content = computed(() => getLocalizedText(product.value?.content))
  const categoryName = computed(() => (product.value?.category?.name ? getLocalizedText(product.value.category.name) : ''))
  const relatedPosts = computed<RelatedPost[]>(() => product.value?.related_posts || [])

  const formatPostDate = (value?: string) =>
    value ? new Date(value).toLocaleDateString(appStore.locale, { year: 'numeric', month: 'long', day: 'numeric' }) : ''

  usePageTitle(() => title.value)

  const loadProduct = async () => {
    const slug = route.params.slug
    if (typeof slug !== 'string') return
    loading.value = true
    error.value = false
    try {
      const res = await productAPI.detail(slug)
      product.value = res.data || null
      currentImage.value = images.value[0] || ''
      purchase.reset()
    } catch {
      product.value = null
      error.value = true
    } finally {
      loading.value = false
    }
  }

  onMounted(() => void loadProduct())
  watch(
    () => route.params.slug,
    (slug, old) => {
      if (slug && slug !== old && route.name === 'product-detail') void loadProduct()
    },
  )

  const breadcrumbs = computed(() => [
    { label: t('nav.home'), to: '/' },
    { label: t('nav.products'), to: '/products' },
    ...(product.value?.category?.slug ? [{ label: categoryName.value, to: `/categories/${product.value.category.slug}` }] : []),
    { label: title.value },
  ])

  return {
    ...purchase,
    loading,
    error,
    product,
    currentImage,
    images,
    title,
    description,
    content,
    categoryName,
    relatedPosts,
    breadcrumbs,
    formatPostDate,
    loadProduct,
  }
}
