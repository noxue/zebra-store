import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { postAPI } from '@/api/catalog'
import type { Post, Product } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { localizedText } from '@/utils/localized'
import { usePageTitle } from './usePageTitle'
import { formatPostDate } from './usePostList'

/** Post (blog or notice) detail with related products. */
export function useBlogDetail() {
  const route = useRoute()
  const { t } = useI18n()
  const appStore = useAppStore()
  const loading = ref(true)
  const post = ref<Post | null>(null)

  const title = computed(() => (post.value ? localizedText(post.value.title, appStore.locale) : ''))
  const content = computed(() => (post.value ? localizedText(post.value.content, appStore.locale) : ''))
  const summary = computed(() => (post.value ? localizedText(post.value.summary, appStore.locale) : ''))
  const publishedAt = computed(() => formatPostDate(post.value?.published_at, appStore.locale))
  const relatedProducts = computed<Product[]>(() => post.value?.related_products || [])
  const isNotice = computed(() => post.value?.type === 'notice')
  const backLink = computed(() => (isNotice.value ? '/notice' : '/blog'))
  const backText = computed(() => (isNotice.value ? t('blogDetail.backToNotice') : t('blogDetail.backToBlog')))
  const sectionLabel = computed(() => (isNotice.value ? t('nav.notice') : t('nav.blog')))

  usePageTitle(() => title.value)

  const loadPost = async () => {
    const slug = String(route.params.slug || '')
    loading.value = true
    try {
      const res = await postAPI.detail(slug)
      post.value = res.data || null
    } catch {
      post.value = null
    } finally {
      loading.value = false
    }
  }

  onMounted(() => void loadPost())
  watch(
    () => route.params.slug,
    (slug, old) => {
      if (slug && slug !== old) void loadPost()
    },
  )

  return { loading, post, title, content, summary, publishedAt, relatedProducts, isNotice, backLink, backText, sectionLabel }
}
