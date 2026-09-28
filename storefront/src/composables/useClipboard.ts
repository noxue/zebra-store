import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { copyText } from '@/utils/clipboard'
import { toast } from './useToast'

/** Copy with toast feedback; `copiedKey` tracks the last copied id for 1.5s. */
export function useClipboard() {
  const { t } = useI18n()
  const copiedKey = ref('')
  let timer: ReturnType<typeof setTimeout> | undefined
  const copy = async (value: string, key = 'default') => {
    if (!value) return false
    try {
      await copyText(value)
      copiedKey.value = key
      clearTimeout(timer)
      timer = setTimeout(() => {
        copiedKey.value = ''
      }, 1500)
      toast.success(t('zs.copied'))
      return true
    } catch {
      toast.error(t('zs.copyFailed'))
      return false
    }
  }
  return { copy, copiedKey }
}
