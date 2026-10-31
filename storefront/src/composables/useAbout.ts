import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { localizedText } from '@/utils/localized'
import { usePageTitle } from './usePageTitle'

/** Main-site introduction; customer support has its own page. */
export function useAbout() {
  const { t } = useI18n()
  const appStore = useAppStore()
  usePageTitle(() => t('nav.about'))

  const about = computed(() => appStore.isResellerTenant ? undefined : appStore.config?.about)
  const text = (raw: Parameters<typeof localizedText>[0]) => localizedText(raw, appStore.locale)

  const configuredHeroTitle = computed(() => text(about.value?.hero?.title))
  /** Falls back to the generic "About us" heading when nothing is configured. */
  const heroTitle = computed(() => configuredHeroTitle.value || t('about.title'))
  const heroSubtitle = computed(() => text(about.value?.hero?.subtitle))
  const introductionText = computed(() => text(about.value?.introduction))
  const servicesTitle = computed(() => text(about.value?.services?.title))
  const serviceItems = computed(() => {
    const raw = about.value?.services?.items
    return Array.isArray(raw) ? raw.map((item) => text(item)).filter(Boolean) : []
  })
  const hasIntroduction = computed(() => introductionText.value !== '')
  const hasServices = computed(() => servicesTitle.value !== '' || serviceItems.value.length > 0)

  return {
    heroTitle,
    heroSubtitle,
    introductionText,
    servicesTitle,
    serviceItems,
    hasIntroduction,
    hasServices,
  }
}
