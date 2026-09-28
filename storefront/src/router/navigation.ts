import type { Router } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { toast } from '@/composables/useToast'
import i18n from '@/i18n'

/** Every navigation exit, including failed lazy imports, releases the overlay. */
export function installNavigationFeedback(router: Router) {
  router.beforeEach(() => { useAppStore().navigating = true })
  router.afterEach(() => { useAppStore().navigating = false })
  router.onError(() => {
    useAppStore().navigating = false
    toast.error(i18n.global.t('navigation.loadFailed'))
  })
}
