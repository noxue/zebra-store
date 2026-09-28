import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { bannerAPI } from '@/api/catalog'
import type { Banner } from '@/api/types'
import { getImageUrl } from '@/utils/image'
import { useLocalized } from './useLocalized'

/** Autoplay interval for the home hero carousel (original: 5s). */
const AUTOPLAY_MS = 5000
/** Minimum horizontal swipe distance in px (original: 50). */
const SWIPE_THRESHOLD = 50

export function useBannerCarousel() {
  const router = useRouter()
  const { t } = useI18n()
  const { getLocalizedText } = useLocalized()

  const banners = ref<Banner[]>([])
  const bannerLoading = ref(true)
  const currentIndex = ref(0)
  const isMobile = ref(typeof window !== 'undefined' && window.innerWidth < 768)
  let timer: ReturnType<typeof setInterval> | null = null
  let touchStartX = 0

  const bannerCount = computed(() => banners.value.length)
  const current = computed<Banner | null>(() => banners.value[currentIndex.value] ?? banners.value[0] ?? null)

  const goTo = (index: number) => {
    const total = banners.value.length
    if (total === 0) return
    currentIndex.value = ((index % total) + total) % total
  }
  const stop = () => {
    if (timer) clearInterval(timer)
    timer = null
  }
  const start = () => {
    stop()
    if (banners.value.length <= 1) return
    timer = setInterval(() => goTo(currentIndex.value + 1), AUTOPLAY_MS)
  }
  const next = () => {
    goTo(currentIndex.value + 1)
    start()
  }
  const prev = () => {
    goTo(currentIndex.value - 1)
    start()
  }
  const select = (index: number) => {
    goTo(index)
    start()
  }

  const onTouchStart = (e: TouchEvent) => {
    touchStartX = e.touches[0]?.clientX ?? 0
  }
  const onTouchEnd = (e: TouchEvent) => {
    const diff = touchStartX - (e.changedTouches[0]?.clientX ?? 0)
    if (Math.abs(diff) > SWIPE_THRESHOLD) {
      if (diff > 0) next()
      else prev()
    }
  }

  const imageOf = (banner: Banner) =>
    isMobile.value && banner.mobile_image ? getImageUrl(banner.mobile_image) : getImageUrl(banner.image || banner.mobile_image || '')

  const titleOf = (banner: Banner | null) => (banner ? getLocalizedText(banner.title) : '') || t('home.hero.title')
  const subtitleOf = (banner: Banner | null) => (banner ? getLocalizedText(banner.subtitle) : '') || t('home.hero.subtitle')
  const linkOf = (banner: Banner | null) => (!banner || banner.link_type === 'none' ? '' : String(banner.link_value || '').trim())

  const heroLink = computed(() => linkOf(current.value))
  const primaryButtonText = computed(() => {
    if (!heroLink.value) return t('home.hero.cta')
    return String(current.value?.link_type || '').toLowerCase() === 'external' ? t('common.learnMore') : t('common.viewDetails')
  })

  const goToHeroLink = () => {
    const link = heroLink.value
    if (!link) {
      void router.push('/products')
      return
    }
    const newTab = Boolean(current.value?.open_in_new_tab)
    if (/^https?:\/\//i.test(link) || newTab) {
      window.open(link, newTab ? '_blank' : '_self')
      return
    }
    void router.push(link)
  }

  const loadBanners = async () => {
    bannerLoading.value = true
    try {
      const res = await bannerAPI.list({ position: 'home_hero', limit: 5 })
      banners.value = Array.isArray(res.data) ? res.data : []
      currentIndex.value = 0
      start()
    } catch {
      banners.value = []
      stop()
    } finally {
      bannerLoading.value = false
    }
  }

  const onResize = () => {
    isMobile.value = window.innerWidth < 768
  }
  onMounted(() => window.addEventListener('resize', onResize))
  onBeforeUnmount(() => {
    stop()
    window.removeEventListener('resize', onResize)
  })

  return {
    banners,
    bannerLoading,
    currentIndex,
    bannerCount,
    current,
    heroLink,
    primaryButtonText,
    imageOf,
    titleOf,
    subtitleOf,
    loadBanners,
    next,
    prev,
    select,
    goToHeroLink,
    onTouchStart,
    onTouchEnd,
    stop,
  }
}
