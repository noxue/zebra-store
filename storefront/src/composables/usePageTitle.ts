import { onBeforeUnmount, watchEffect } from 'vue'
import { useAppStore } from '@/stores/app'

/** Sets `document.title` to `<title> - <site name>` while the page is mounted. */
export function usePageTitle(title: () => string) {
  const appStore = useAppStore()
  watchEffect(() => {
    appStore.pageTitle = title()
  })
  onBeforeUnmount(() => {
    appStore.pageTitle = ''
  })
}
