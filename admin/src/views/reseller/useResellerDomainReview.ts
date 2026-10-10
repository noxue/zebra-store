import { ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { ResellerDomainReviewSetting } from '@/api/types'
import i18n from '@/i18n'
import { notifySuccess } from '@/utils/notify'

const KEY = 'reseller_domain_review_config'

export function useResellerDomainReview() {
  const mode = ref<'manual' | 'auto'>('manual')
  const cnameTarget = ref('')
  const saving = ref(false)
  const loading = ref(false)

  const load = async () => {
    loading.value = true
    try {
      const response = await adminAPI.getSettings<ResellerDomainReviewSetting>({ key: KEY })
      mode.value = response.data.mode === 'auto' ? 'auto' : 'manual'
      cnameTarget.value = response.data.cname_target || ''
    } catch {
      mode.value = 'manual'
      cnameTarget.value = ''
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    saving.value = true
    try {
      const response = await adminAPI.updateSettings<ResellerDomainReviewSetting>({ key: KEY, value: { mode: mode.value, cname_target: cnameTarget.value.trim() } })
      mode.value = response.data.mode
      cnameTarget.value = response.data.cname_target
      notifySuccess(i18n.global.t('admin.resellerDomains.review.saved'))
    } catch {
      await load()
    } finally {
      saving.value = false
    }
  }

  return { mode, cnameTarget, saving, loading, load, save }
}
