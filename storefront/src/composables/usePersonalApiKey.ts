import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { personalApiKeyAPI } from '@/api/personalApiKey'
import type { PersonalApiKeyStatus } from '@/api/types'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
export function usePersonalApiKey() {
  const { t } = useI18n()
  const { confirm } = useConfirmDialog()
  const status = ref<PersonalApiKeyStatus | null>(null)
  const secret = ref('')
  const error = ref('')
  const busy = ref(false)
  const loaded = ref(false)
  const copied = ref(false)
  const message = (err: unknown) => err instanceof Error ? err.message : t('personalApiKey.failed')
  async function load() {
    error.value = ''
    loaded.value = false
    try { status.value = (await personalApiKeyAPI.status()).data; loaded.value = true }
    catch (err) { error.value = message(err) }
  }
  async function act(action: 'create' | 'rotate' | 'delete') {
    if (busy.value || !loaded.value) return
    if (action !== 'create' && !(await confirm({ title: t('personalApiKey.title'), message: t(action === 'rotate' ? 'personalApiKey.confirmRotate' : 'personalApiKey.confirmDelete') }))) return
    busy.value = true
    error.value = ''
    copied.value = false
    try {
      if (action === 'delete') { await personalApiKeyAPI.remove(); secret.value = '' }
      else { secret.value = (await personalApiKeyAPI[action]()).data.api_key }
      await load()
    } catch (err) { error.value = message(err) }
    finally { busy.value = false }
  }
  async function copy() {
    try { await navigator.clipboard.writeText(secret.value); copied.value = true }
    catch (err) { error.value = message(err) }
  }
  onMounted(() => void load())
  return { status, secret, error, busy, loaded, copied, load, act, copy }
}
