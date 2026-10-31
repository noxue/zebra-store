import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { localizedText } from '@/utils/localized'
import { usePageTitle } from './usePageTitle'

export function useContact() {
  const { t } = useI18n()
  const app = useAppStore()
  usePageTitle(() => t('customerSupport.title'))
  const contact = computed(() => app.config?.contact || {})
  const guidance = computed(() => app.isResellerTenant ? undefined : app.config?.about?.contact)
  const title = computed(() => localizedText(guidance.value?.title, app.locale) || t('customerSupport.title'))
  const text = computed(() => localizedText(guidance.value?.text, app.locale))
  const hasContact = computed(() => Object.values(contact.value).some((v) => typeof v === 'string' && v.trim()))
  const qrPreview = ref<{ label: string; image: string } | null>(null)
  const openQrPreview = (code: { label: string; image: string }) => { qrPreview.value = code }
  const closeQrPreview = () => { qrPreview.value = null }
  return { contact, title, text, hasContact, qrPreview, openQrPreview, closeQrPreview }
}
