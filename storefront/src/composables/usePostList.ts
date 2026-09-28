import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { postAPI } from '@/api/catalog'
import type { Post, PostListParams, PostType } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { debounce } from '@/utils/debounce'
import { usePageTitle } from './usePageTitle'

/** Long localized date, e.g. 2026年9月24日 / September 24, 2026. */
export const formatPostDate = (value: string | undefined, locale: string): string => {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  return date.toLocaleDateString(locale, { year: 'numeric', month: 'long', day: 'numeric' })
}

/** Blog / Notice list: pagination (12 per page) and debounced search (blog only). */
export function usePostList(type: PostType) {
  const { t } = useI18n()
  const appStore = useAppStore()
  usePageTitle(() => (type === 'blog' ? t('nav.blog') : t('nav.notice')))

  const loading = ref(true)
  const error = ref(false)
  const posts = ref<Post[]>([])
  const currentPage = ref(1)
  const pageSize = 12
  const total = ref(0)
  const totalPages = ref(0)
  const searchKeyword = ref('')
  const hasKeyword = computed(() => searchKeyword.value.trim() !== '')

  const loadPosts = async () => {
    loading.value = true
    error.value = false
    try {
      const params: PostListParams = { type, page: currentPage.value, page_size: pageSize }
      const keyword = searchKeyword.value.trim()
      if (keyword) params.search = keyword
      const res = await postAPI.list(params)
      posts.value = Array.isArray(res.data) ? res.data : []
      total.value = res.pagination?.total ?? posts.value.length
      totalPages.value = res.pagination?.total_page ?? 1
    } catch {
      error.value = true
      posts.value = []
    } finally {
      loading.value = false
    }
  }

  const debouncedLoad = debounce(() => void loadPosts(), 300)

  const changePage = (page: number) => {
    if (page < 1 || page > totalPages.value) return
    currentPage.value = page
    void loadPosts()
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const clearSearch = () => {
    searchKeyword.value = ''
  }

  watch(searchKeyword, () => {
    currentPage.value = 1
    loading.value = true
    debouncedLoad()
  })

  onMounted(() => void loadPosts())
  onBeforeUnmount(() => debouncedLoad.cancel())

  const formatDate = (value?: string) => formatPostDate(value, appStore.locale)

  return { loading, error, posts, currentPage, totalPages, total, searchKeyword, hasKeyword, changePage, clearSearch, reload: loadPosts, formatDate }
}
