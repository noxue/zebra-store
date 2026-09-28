import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { resellerAPI } from '@/api/reseller'
import type { AlertTone } from '@/components/ui'
import { isActiveVerifiedDomain, isPendingDomain } from '@/utils/reseller/console'
import { RESELLER_DOMAIN_TYPE_SUBDOMAIN } from '@/utils/reseller/constants'
import { isResellerProfileActive } from '@/utils/reseller/management'
import { useResellerProfileContext } from './useResellerProfile'

/** Very small hostname sanity check (backend validates authoritatively). */
export const isLikelyDomain = (value: string): boolean => /^(?=.{3,253}$)([a-z0-9-]+\.)+[a-z]{2,}$/i.test(value.trim())

export const useResellerDomains = () => {
  const { t } = useI18n()
  const profile = useResellerProfileContext()
  const domainInput = ref('')
  const submitting = ref(false)
  const alert = ref<{ tone: AlertTone; message: string } | null>(null)

  const domains = computed(() => profile.snapshot.value?.domains || [])
  const systemDomain = computed(() => domains.value.find((d) => d.type === RESELLER_DOMAIN_TYPE_SUBDOMAIN) || null)
  const customDomains = computed(() => domains.value.filter((d) => d.type !== RESELLER_DOMAIN_TYPE_SUBDOMAIN))
  const activeDomains = computed(() => domains.value.filter(isActiveVerifiedDomain))
  const primaryDomain = computed(() => activeDomains.value.find((d) => d.is_primary) || null)
  const pendingDomains = computed(() => domains.value.filter(isPendingDomain))
  const canSubmitDomain = computed(() => isResellerProfileActive(profile.snapshot.value?.profile))

  const submitDomain = async () => {
    const domain = domainInput.value.trim()
    if (!domain) {
      alert.value = { tone: 'error', message: t('personalCenter.reseller.errors.domainRequired') }
      return
    }
    submitting.value = true
    alert.value = null
    try {
      await resellerAPI.submitDomain({ domain })
      domainInput.value = ''
      await profile.load()
      alert.value = { tone: 'success', message: t('personalCenter.reseller.domainSubmitSuccess') }
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.errors.domainSubmitFailed')) }
    } finally {
      submitting.value = false
    }
  }

  return { profile, domainInput, submitting, alert, domains, systemDomain, customDomains, activeDomains, primaryDomain, pendingDomains, canSubmitDomain, submitDomain }
}
