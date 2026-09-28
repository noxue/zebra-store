import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { resellerAPI } from '@/api/reseller'
import type { AlertTone } from '@/components/ui'
import { getResellerProfileStatusKey } from '@/utils/reseller/management'
import { useResellerProfileContext } from './useResellerProfile'

export type StepState = 'done' | 'current' | 'pending' | 'failed'

/** Pure step computation for the apply stepper. */
export const buildApplySteps = (status: string): StepState[] => {
  const submitted = status !== 'not_opened'
  const active = status === 'active'
  return [submitted ? 'done' : 'current', active ? 'done' : status === 'pending_review' ? 'current' : status === 'rejected' ? 'failed' : 'pending', active ? 'done' : 'pending']
}

export const REASON_MAX = 500

export const useResellerApply = () => {
  const { t } = useI18n()
  const profile = useResellerProfileContext()
  const reason = ref('')
  const submitting = ref(false)
  const alert = ref<{ tone: AlertTone; message: string } | null>(null)

  const statusText = computed(() => {
    if (!profile.snapshot.value?.opened) return t('personalCenter.reseller.managementStatus.notOpened')
    return t(`personalCenter.reseller.profileStatusMap.${getResellerProfileStatusKey(profile.snapshot.value?.profile?.status)}`)
  })

  const steps = computed(() => {
    const status = profile.state.value.profileStatus
    const states = buildApplySteps(status)
    const labels = [
      t('resellerConsole.apply.stepSubmit'),
      status === 'rejected' ? t('resellerConsole.apply.stepRejected') : t('resellerConsole.apply.stepReview'),
      t('resellerConsole.apply.stepActive'),
    ]
    return labels.map((label, i) => ({ label, state: states[i] }))
  })

  /** Apply form shows only when the backend allows applying. */
  const canSubmit = computed(() => profile.state.value.canApply)
  const rejectReason = computed(() => profile.snapshot.value?.profile?.reject_reason || '')
  const benefits = computed(() => [t('resellerConsole.dashboard.benefit1'), t('resellerConsole.dashboard.benefit2'), t('resellerConsole.dashboard.benefit3')])

  const submit = async () => {
    if (submitting.value) return
    alert.value = null
    submitting.value = true
    try {
      await resellerAPI.apply({ reason: reason.value.trim() })
      reason.value = ''
      await profile.load()
      alert.value = { tone: 'success', message: t('personalCenter.reseller.applySuccess') }
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.errors.applyFailed')) }
    } finally {
      submitting.value = false
    }
  }

  return { profile, reason, submitting, alert, statusText, steps, canSubmit, rejectReason, benefits, submit }
}
