import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { usePageTitle } from './usePageTitle'

export type LegalType = 'terms' | 'privacy'

/** Terms / privacy HTML from `config.legal[type][locale]`. */
export function useLegal(type: () => LegalType) {
  const { t } = useI18n()
  const appStore = useAppStore()
  const title = computed(() => (type() === 'terms' ? t('footer.terms') : t('footer.privacy')))
  usePageTitle(() => title.value)
  const loading = computed(() => appStore.loading)
  const content = computed(() => {
    const legal = appStore.config?.legal
    const block = type() === 'terms' ? legal?.terms : legal?.privacy
    return block?.[appStore.locale as keyof typeof block] || ''
  })
  return { loading, title, content }
}
