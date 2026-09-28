import { computed, ref, type Ref } from 'vue'
import type { PaymentCreateResult } from '@/api/types'
import { resolvePaymentLinkNavigationTarget, resolvePaymentPresentationMode } from '@/utils/paymentResumePolicy'
import { buildCryptoDetails, qrImageSource } from '@/utils/orderPayment'
import { isTelegramMiniApp, openTelegramLink } from '@/utils/telegramWebApp'

/**
 * Shared presentation state of a created payment (QR / pay link / crypto).
 * Used by Payment and RechargeOrderDetail.
 */
export function usePaymentLink(payment: Ref<PaymentCreateResult | null>) {
  const openedPayWindow = ref(false)
  const payLink = computed(() => String(payment.value?.pay_url || '').trim())
  const interactionMode = computed(() => String(payment.value?.interaction_mode || '').trim().toLowerCase())
  const presentationMode = computed(() => resolvePaymentPresentationMode(interactionMode.value))
  const qrContent = computed(() => String(payment.value?.qr_code || '').trim())
  /** In QR mode a missing qr_code falls back to encoding the pay link. */
  const qrDisplayContent = computed(() => {
    if (qrContent.value) return qrContent.value
    return presentationMode.value === 'qr' ? payLink.value : ''
  })
  const qrUsingPayLinkFallback = computed(() => !qrContent.value && !!qrDisplayContent.value)
  const showQRCode = computed(() => presentationMode.value === 'qr' && !!qrDisplayContent.value)
  const qrDirectImage = computed(() => (qrDisplayContent.value ? qrImageSource(qrDisplayContent.value) : null))
  const cryptoDetails = computed(() => buildCryptoDetails(payment.value))
  const cryptoWalletAddress = computed(() => String(payment.value?.wallet_address || '').trim())
  const telegramMiniApp = isTelegramMiniApp()
  const showTelegramPayHint = computed(() => telegramMiniApp && !!payLink.value)

  const openPayLink = (automatic = false) => {
    if (!payLink.value) return
    if (telegramMiniApp) {
      openTelegramLink(payLink.value)
    } else if (resolvePaymentLinkNavigationTarget(automatic) === 'current-tab') {
      window.location.assign(payLink.value)
      return
    } else {
      const w = window.open('', '_blank')
      if (!w) return
      w.opener = null
      w.location.replace(payLink.value)
    }
    openedPayWindow.value = true
  }

  return {
    openedPayWindow,
    payLink,
    interactionMode,
    presentationMode,
    qrDisplayContent,
    qrUsingPayLinkFallback,
    showQRCode,
    qrDirectImage,
    cryptoDetails,
    cryptoWalletAddress,
    telegramMiniApp,
    showTelegramPayHint,
    openPayLink,
  }
}
