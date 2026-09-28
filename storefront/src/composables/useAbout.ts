import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { localizedText } from '@/utils/localized'
import { usePageTitle } from './usePageTitle'

/** About page content from `config.about` + `config.contact`. */
export function useAbout() {
  const { t } = useI18n()
  const appStore = useAppStore()
  usePageTitle(() => t('nav.about'))

  const about = computed(() => appStore.config?.about)
  const contactConfig = computed(() => appStore.config?.contact || {})
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
  const contactTitle = computed(() => text(about.value?.contact?.title))
  const contactText = computed(() => text(about.value?.contact?.text))
  const hasIntroduction = computed(() => introductionText.value !== '')
  const hasServices = computed(() => servicesTitle.value !== '' || serviceItems.value.length > 0)
  const hasContactLinks = computed(() => !!(contactConfig.value.telegram || contactConfig.value.whatsapp))
  const hasContact = computed(() => contactTitle.value !== '' || contactText.value !== '' || hasContactLinks.value)

  return {
    contactConfig,
    heroTitle,
    heroSubtitle,
    introductionText,
    servicesTitle,
    serviceItems,
    contactTitle,
    contactText,
    hasIntroduction,
    hasServices,
    hasContactLinks,
    hasContact,
  }
}
