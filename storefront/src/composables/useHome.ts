import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { postAPI, productAPI } from '@/api/catalog'
import type { AnnouncementConfig, Post, PostListParams, Product } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { useAnnouncement } from './useAnnouncement'
import { useBannerCarousel } from './useBannerCarousel'
import { useLocalized } from './useLocalized'
import { useNavConfig } from './useNavConfig'
import { usePageTitle } from './usePageTitle'
import { useProductList, useProductListGroups } from './useProductList'
import { useQuickBuy } from './useQuickBuy'

/** Featured products on the card-mode home page (original: 15). */
const FEATURED_PAGE_SIZE = 15
/** Latest posts on the home page (original: 3). */
const LATEST_POSTS = 3
/** List-mode page size (original: 20). */
const LIST_PAGE_SIZE = 20

/** Home page: banner carousel, featured / latest (card mode) or grouped list (list mode), announcement. */
export function useHome() {
  const router = useRouter()
  const route = useRoute()
  const { t } = useI18n()
  const appStore = useAppStore()
  const { getLocalizedText } = useLocalized()
  const { blogEnabled, noticeEnabled } = useNavConfig()

  const isListMode = computed(() => appStore.isListMode)
  const latestVisible = computed(() => blogEnabled.value || noticeEnabled.value)

  const banner = useBannerCarousel()
  const quickBuy = useQuickBuy()
  const list = useProductList({ pageSize: LIST_PAGE_SIZE, homeRouteName: 'home' })
  const groups = useProductListGroups(list.products, list.categoryMap)

  const featured = ref<Product[]>([])
  const featuredLoading = ref(true)
  const posts = ref<Post[]>([])

  const announcement = ref<AnnouncementConfig | null>(null)
  const announcementOpen = ref(false)
  const { shouldShow } = useAnnouncement()

  usePageTitle(() => {
    if (route.name === 'category-products' && list.selectedCategory.value) {
      const cat = list.categoryMap.value.get(list.selectedCategory.value)
      return cat ? getLocalizedText(cat.name as Record<string, string>) : t('nav.products')
    }
    if (route.name === 'products') return t('nav.products')
    return ''
  })

  const loadFeatured = async () => {
    featuredLoading.value = true
    try {
      const res = await productAPI.list({ page: 1, page_size: FEATURED_PAGE_SIZE })
      featured.value = Array.isArray(res.data) ? res.data : []
    } catch {
      featured.value = []
    } finally {
      featuredLoading.value = false
    }
  }

  const loadLatestPosts = async () => {
    if (!latestVisible.value) return
    const params: PostListParams = { page: 1, page_size: LATEST_POSTS }
    if (blogEnabled.value && !noticeEnabled.value) params.type = 'blog'
    if (!blogEnabled.value && noticeEnabled.value) params.type = 'notice'
    try {
      const res = await postAPI.list(params)
      posts.value = Array.isArray(res.data) ? res.data : []
    } catch {
      posts.value = []
    }
  }

  const goToProduct = (slug: string) => void router.push(`/products/${slug}`)
  const formatDate = (value?: string) => (value ? new Date(value).toLocaleDateString(appStore.locale) : '')

  onMounted(async () => {
    await appStore.loadConfig()
    if (isListMode.value) await Promise.all([banner.loadBanners(), list.initialize()])
    else await Promise.all([banner.loadBanners(), loadFeatured(), loadLatestPosts()])
    const a = appStore.config?.announcement
    if (a && shouldShow(a)) {
      announcement.value = a
      announcementOpen.value = true
    }
  })

  return {
    isListMode,
    latestVisible,
    blogEnabled,
    noticeEnabled,
    banner,
    quickBuy,
    list,
    groups,
    featured,
    featuredLoading,
    posts,
    announcement,
    announcementOpen,
    goToProduct,
    formatDate,
    getLocalizedText,
  }
}
