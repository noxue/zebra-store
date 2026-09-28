import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { usePageTitle } from './usePageTitle'

export function useNotFound() {
  const router = useRouter()
  const { t } = useI18n()
  const appStore = useAppStore()
  usePageTitle(() => t('notFoundPage.title'))
  const goBack = () => {
    if (window.history.length > 1) router.back()
    else void router.push('/')
  }
  return { siteName: () => appStore.siteName, goBack }
}
