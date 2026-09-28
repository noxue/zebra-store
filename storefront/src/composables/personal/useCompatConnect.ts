import { computed, ref } from 'vue'
import i18n from '@/i18n'
import { errorMessage } from '@/api/client'
import { apiCredentialAPI } from '@/api/credential'
import type { ApiCompatKeyData } from '@/api/types'
import { toast } from '@/composables/useToast'

const MASK_HEAD = 4
const MASK_TAIL = 4

/** Hide the middle of an app key: `AB12••••••••YZ89`. */
export const maskAppKey = (key: string): string => {
  if (!key) return ''
  if (key.length <= MASK_HEAD + MASK_TAIL) return '•'.repeat(key.length)
  return `${key.slice(0, MASK_HEAD)}${'•'.repeat(8)}${key.slice(-MASK_TAIL)}`
}

/** Allowlist text as the server stores it (comma separated, blanks dropped). */
export const normalizeAllowlist = (raw: string): string =>
  raw
    .split(/[\s,;]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .join(',')

/**
 * 个人中心 → API 对接 → 异次元 / 萌次元 对接: the compat key (app_id + app_key) that
 * acg-faka / mcy-shop downstream sites sign with, its on/off switch and IP allowlist.
 */
export function useCompatConnect() {
  const t = (key: string) => i18n.global.t(key)
  const data = ref<ApiCompatKeyData | null>(null)
  const loading = ref(false)
  const busy = ref(false)
  const error = ref('')
  const confirmingIssue = ref(false)
  const keyRevealed = ref(false)
  const allowlist = ref('')

  const hasKey = computed(() => !!data.value?.app_key)
  const displayedKey = computed(() => {
    const key = data.value?.app_key ?? ''
    return keyRevealed.value ? key : maskAppKey(key)
  })
  const enabledProtocols = computed(() => (data.value?.protocols ?? []).filter((p) => p.enabled))
  const allowlistDirty = computed(() => normalizeAllowlist(allowlist.value) !== (data.value?.ip_allowlist ?? ''))

  const accept = (next: ApiCompatKeyData) => {
    data.value = next
    allowlist.value = next.ip_allowlist.split(',').filter(Boolean).join('\n')
  }

  const load = async () => {
    loading.value = true
    error.value = ''
    try {
      accept((await apiCredentialAPI.getCompat()).data)
    } catch (err) {
      data.value = null
      error.value = errorMessage(err, t('apiCompat.loadFailed'))
    } finally {
      loading.value = false
    }
  }

  const askIssue = () => {
    error.value = ''
    confirmingIssue.value = true
  }
  const cancelIssue = () => {
    if (!busy.value) confirmingIssue.value = false
  }

  /** Issues (or resets) the app key; a reset stops the old key at once. */
  const issue = async (): Promise<boolean> => {
    busy.value = true
    error.value = ''
    try {
      accept((await apiCredentialAPI.issueCompat()).data)
      keyRevealed.value = true
      toast.success(t('apiCompat.issueSuccess'))
      return true
    } catch (err) {
      error.value = errorMessage(err, t('apiCompat.issueFailed'))
      return false
    } finally {
      busy.value = false
      confirmingIssue.value = false
    }
  }

  const update = async (body: { is_active?: boolean; ip_allowlist?: string }, success: string): Promise<boolean> => {
    busy.value = true
    error.value = ''
    try {
      accept((await apiCredentialAPI.updateCompat(body)).data)
      toast.success(t(success))
      return true
    } catch (err) {
      error.value = errorMessage(err, t('apiCompat.updateFailed'))
      return false
    } finally {
      busy.value = false
    }
  }

  const toggleActive = (active: boolean) => update({ is_active: active }, active ? 'apiCompat.enabledToast' : 'apiCompat.disabledToast')
  const saveAllowlist = () => update({ ip_allowlist: normalizeAllowlist(allowlist.value) }, 'apiCompat.allowlistSaved')
  const toggleReveal = () => {
    keyRevealed.value = !keyRevealed.value
  }

  return {
    data,
    loading,
    busy,
    error,
    confirmingIssue,
    keyRevealed,
    allowlist,
    hasKey,
    displayedKey,
    enabledProtocols,
    allowlistDirty,
    load,
    askIssue,
    cancelIssue,
    issue,
    toggleActive,
    saveAllowlist,
    toggleReveal,
  }
}

export type CompatConnect = ReturnType<typeof useCompatConnect>
