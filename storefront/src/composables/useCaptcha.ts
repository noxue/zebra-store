import { computed, ref, shallowRef } from 'vue'
import type { CaptchaPayload, CaptchaScene } from '@/api/types'
import { useAppStore } from '@/stores/app'

export interface CaptchaController {
  enabled: Readonly<{ value: boolean }>
  provider: Readonly<{ value: string }>
  siteKey: Readonly<{ value: string }>
  /** image: captcha_id + captcha_code; turnstile: turnstile_token */
  payload: { value: CaptchaPayload }
  /** true when the scene is disabled or the user filled the captcha */
  isComplete: () => boolean
  /** Payload to send (undefined when captcha disabled). */
  build: () => CaptchaPayload | undefined
  /** Refresh image / reset turnstile (after a failed submit). */
  reset: () => void
  /** Replace the payload (used by the widget). */
  setPayload: (next: CaptchaPayload) => void
  /** Internal: widget registers its refresh function. */
  bindRefresher: (fn: (() => void) | null) => void
}

/** Captcha state for a scene from `config.captcha` (provider none|image|turnstile). */
export function useCaptcha(scene: CaptchaScene): CaptchaController {
  const appStore = useAppStore()
  const provider = computed(() => String(appStore.config?.captcha?.provider || 'none'))
  const enabled = computed(() => provider.value !== 'none' && !!appStore.config?.captcha?.scenes?.[scene])
  const siteKey = computed(() => String(appStore.config?.captcha?.turnstile?.site_key || ''))
  const payload = ref<CaptchaPayload>({})
  const refresher = shallowRef<(() => void) | null>(null)

  const isComplete = () => {
    if (!enabled.value) return true
    if (provider.value === 'image') return !!payload.value.captcha_id && !!payload.value.captcha_code?.trim()
    if (provider.value === 'turnstile') return !!payload.value.turnstile_token
    return true
  }

  const build = (): CaptchaPayload | undefined => {
    if (!enabled.value) return undefined
    if (provider.value === 'image') {
      return { captcha_id: payload.value.captcha_id || '', captcha_code: (payload.value.captcha_code || '').trim() }
    }
    if (provider.value === 'turnstile') return { turnstile_token: payload.value.turnstile_token || '' }
    return undefined
  }

  const reset = () => {
    payload.value = {}
    refresher.value?.()
  }

  return {
    enabled,
    provider,
    siteKey,
    payload,
    isComplete,
    build,
    reset,
    setPayload: (next) => {
      payload.value = next
    },
    bindRefresher: (fn) => {
      refresher.value = fn
    },
  }
}
