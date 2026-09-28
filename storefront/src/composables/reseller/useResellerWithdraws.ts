import { computed, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import type { AlertTone } from '@/components/ui'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { exceedsAvailable, getResellerWithdrawDisabledReasonKey, isResellerWithdrawEnabled } from '@/utils/reseller/finance'
import { useResellerFinance } from './useResellerFinance'

export interface WithdrawForm {
  amount: string
  currency: string
  channel: string
  account: string
}

/** Returns the i18n key of the first validation error, or null. */
export const validateWithdrawForm = (form: WithdrawForm): string | null => {
  const amount = Number(form.amount)
  if (!form.amount.trim() || !Number.isFinite(amount) || amount <= 0) return 'withdrawAmountRequired'
  if (!form.currency.trim()) return 'withdrawCurrencyRequired'
  if (!form.channel.trim()) return 'withdrawChannelRequired'
  if (!form.account.trim()) return 'withdrawAccountRequired'
  return null
}

export const useResellerWithdraws = () => {
  const { t } = useI18n()
  const { confirm } = useConfirmDialog()
  const finance = useResellerFinance()
  const form = reactive<WithdrawForm>({ amount: '', currency: '', channel: '', account: '' })
  const alert = ref<{ tone: AlertTone; message: string } | null>(null)

  const currencies = computed(() => Array.from(new Set(finance.balances.value.map((b) => b.currency).filter(Boolean))))
  const withdrawEnabled = computed(() => isResellerWithdrawEnabled(finance.dashboard.value))
  const disabledReasonText = computed(() =>
    t(`personalCenter.reseller.withdrawDisabledReason.${getResellerWithdrawDisabledReasonKey(finance.dashboard.value?.withdraw_disabled_reason)}`),
  )
  const selectedAvailable = computed<number | null>(() => {
    const match = finance.balances.value.find((b) => b.currency === form.currency)
    return match ? Number(match.available_amount) || 0 : null
  })
  const amountExceeded = computed(() => exceedsAvailable(form.amount, selectedAvailable.value))

  watch(currencies, (values) => {
    if (!form.currency && values[0]) form.currency = values[0]
  })

  const load = () => Promise.all([finance.loadDashboard(), finance.loadBalances(), finance.loadWithdraws({ page: 1 })])

  const submit = async () => {
    alert.value = null
    const errKey = validateWithdrawForm(form)
    if (errKey) {
      alert.value = { tone: 'error', message: t(`personalCenter.reseller.errors.${errKey}`) }
      return
    }
    if (amountExceeded.value) {
      alert.value = { tone: 'error', message: t('resellerConsole.withdraws.exceedAvailable') }
      return
    }
    const ok = await confirm({
      title: t('resellerConsole.withdraws.confirmTitle'),
      message: t('resellerConsole.withdraws.confirmMessage', { amount: form.amount, currency: form.currency }),
    })
    if (!ok) return
    try {
      await finance.applyWithdraw({ amount: form.amount.trim(), currency: form.currency.trim(), channel: form.channel.trim(), account: form.account.trim() })
      Object.assign(form, { amount: '', channel: '', account: '' })
      alert.value = { tone: 'success', message: t('personalCenter.reseller.withdrawSuccess') }
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.errors.withdrawFailed')) }
    }
  }

  return { ...finance, form, alert, currencies, withdrawEnabled, disabledReasonText, selectedAvailable, amountExceeded, load, submit }
}
