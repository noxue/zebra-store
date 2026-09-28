import { computed, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { giftCardAPI } from '@/api/wallet'
import type { GiftCardRedeemResult } from '@/api/types'
import { useCaptcha } from '@/composables/useCaptcha'
import { useLocalized } from '@/composables/useLocalized'
import { usePanelAlert } from './usePanelAlert'

/** Gift card redemption with optional captcha. */
export function useGiftCardPanel() {
  const { t } = useI18n()
  const { formatPrice, siteCurrency } = useLocalized()
  const { alert, show, clear } = usePanelAlert()
  const captcha = useCaptcha('gift_card_redeem')
  const form = reactive({ code: '' })
  const submitting = ref(false)
  const result = ref<GiftCardRedeemResult | null>(null)

  const currency = computed(() => result.value?.gift_card?.currency || siteCurrency.value)
  const redeemedAmount = computed(() => {
    const raw = String(result.value?.wallet_delta || result.value?.gift_card?.amount || '').trim()
    return raw ? formatPrice(raw, currency.value) : '-'
  })
  const balance = computed(() => {
    const raw = String(result.value?.wallet?.balance || '').trim()
    return raw ? formatPrice(raw, currency.value) : '-'
  })

  const reset = () => {
    form.code = ''
    result.value = null
    clear()
    captcha.reset()
  }

  const submit = async () => {
    clear()
    const code = form.code.trim().toUpperCase()
    if (!code) return show('warning', t('personalCenter.giftCard.errors.codeRequired'))
    if (!captcha.isComplete()) return show('warning', t('auth.common.captchaRequired'))
    submitting.value = true
    try {
      const res = await giftCardAPI.redeem({ code, captcha_payload: captcha.build() })
      result.value = res.data
      show(
        'success',
        t('personalCenter.giftCard.redeemSuccess', {
          amount: String(res.data?.wallet_delta || res.data?.gift_card?.amount || ''),
          currency: currency.value,
        }),
      )
      form.code = ''
      captcha.reset()
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.giftCard.errors.redeemFailed')))
      captcha.reset()
    } finally {
      submitting.value = false
    }
  }

  return { alert, captcha, form, submitting, result, redeemedAmount, balance, submit, reset }
}
