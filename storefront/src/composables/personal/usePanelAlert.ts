import { ref } from 'vue'
import type { AlertTone } from '@/components/ui'

export interface PanelAlert {
  tone: AlertTone
  message: string
}

/** Inline alert state shared by personal-center panels. */
export function usePanelAlert() {
  const alert = ref<PanelAlert | null>(null)
  const show = (tone: AlertTone, message: string) => {
    alert.value = { tone, message }
  }
  const clear = () => {
    alert.value = null
  }
  return { alert, show, clear }
}
