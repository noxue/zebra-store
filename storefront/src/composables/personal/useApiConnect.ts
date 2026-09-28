import { computed, ref, type Ref } from 'vue'
import i18n from '@/i18n'
import { errorMessage } from '@/api/client'
import { apiCredentialAPI } from '@/api/credential'
import type { ApiCredentialData } from '@/api/types'
import { maskConnectionCode, resolveRotationState, supportsConnectionCode } from '@/utils/apiConnect'
import { copyText } from '@/utils/clipboard'
import { toast } from '@/composables/useToast'

export type ApiConnectAction = 'code' | 'rotate'

export interface UseApiConnectOptions {
  credential: Ref<ApiCredentialData | null>
  /** Reload the credential after a rotation (refreshes rotation_pending / masked secret). */
  reload?: () => Promise<void>
}

/**
 * 「一键对接」: connection code generation (shown once, masked on demand, copyable) and dual-secret
 * rotation. Both actions go through a confirm step because they create a new secret.
 */
export function useApiConnect(options: UseApiConnectOptions) {
  const t = (key: string, params?: Record<string, unknown>) => (params ? i18n.global.t(key, params) : i18n.global.t(key))
  const confirming = ref<ApiConnectAction | null>(null)
  const busy = ref(false)
  const error = ref('')
  /** The connection code; only kept in memory until dismissed (never re-fetchable). */
  const code = ref('')
  const codeRevealed = ref(true)
  const codeCopied = ref(false)
  const rotatedSecret = ref('')
  const localExpiry = ref<string | null>(null)
  let copiedTimer: ReturnType<typeof setTimeout> | undefined

  const available = computed(() => supportsConnectionCode(options.credential.value))
  const rotation = computed(() => resolveRotationState(options.credential.value, localExpiry.value))
  const displayedCode = computed(() => (codeRevealed.value ? code.value : maskConnectionCode(code.value)))

  const ask = (action: ApiConnectAction) => {
    error.value = ''
    confirming.value = action
  }
  const cancel = () => {
    if (!busy.value) confirming.value = null
  }

  const run = async (action: ApiConnectAction) => {
    busy.value = true
    error.value = ''
    try {
      if (action === 'code') {
        const data = (await apiCredentialAPI.createConnectionCode()).data
        rotatedSecret.value = ''
        code.value = data.code
        codeRevealed.value = true
        codeCopied.value = false
        localExpiry.value = data.rotation_expires_at
        toast.success(t('apiConnect.codeSuccess'))
      } else {
        const data = (await apiCredentialAPI.rotate()).data
        code.value = ''
        rotatedSecret.value = data.api_secret
        localExpiry.value = data.rotation_expires_at
        toast.success(t('apiConnect.rotateSuccess'))
      }
      await options.reload?.()
      return true
    } catch (err) {
      error.value = errorMessage(err, t(action === 'code' ? 'apiConnect.codeFailed' : 'apiConnect.rotateFailed'))
      return false
    } finally {
      busy.value = false
      confirming.value = null
    }
  }

  const confirm = () => (confirming.value ? run(confirming.value) : Promise.resolve(false))

  const toggleReveal = () => {
    codeRevealed.value = !codeRevealed.value
  }

  /** Always copies the full code, even while it is masked on screen. */
  const copyCode = async (): Promise<boolean> => {
    if (!code.value) return false
    try {
      await copyText(code.value)
      codeCopied.value = true
      clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => (codeCopied.value = false), 2000)
      toast.success(t('apiConnect.copied'))
      return true
    } catch {
      toast.error(t('apiConnect.copyFailed'))
      return false
    }
  }

  /** "我已保存": forget the code/secret; they cannot be shown again. */
  const dismissCode = () => {
    code.value = ''
    codeCopied.value = false
    codeRevealed.value = true
  }
  const dismissSecret = () => {
    rotatedSecret.value = ''
  }

  return {
    available,
    rotation,
    confirming,
    busy,
    error,
    code,
    displayedCode,
    codeRevealed,
    codeCopied,
    rotatedSecret,
    ask,
    cancel,
    confirm,
    toggleReveal,
    copyCode,
    dismissCode,
    dismissSecret,
  }
}

export type ApiConnect = ReturnType<typeof useApiConnect>
