export type PaymentResetReason = 'generic' | 'route_change' | 'change_payment_method'

export interface PaymentResetPolicy {
  resumeLatestPayment: boolean
  clearSelectedChannel: boolean
  stopActivePaymentWatch: boolean
}

export interface CachedPaymentRestorePolicy {
  startActivePaymentWatch: boolean
  autoOpenPayLink: boolean
}

export type PaymentPresentationMode = 'qr' | 'redirect'
export type PaymentLinkNavigationTarget = 'current-tab' | 'new-window'
export type PaymentInteractionLabelKey =
  | 'payment.modeQr'
  | 'payment.modeRedirect'
  | 'payment.modeWap'
  | 'payment.modePage'
export type PaymentResultTitleKey =
  | 'payment.resultTitle'
  | 'payment.resultRedirectTitle'
  | 'payment.modeWap'
  | 'payment.modePage'

const REDIRECT_PAYMENT_INTERACTION_MODES = new Set(['redirect', 'wap', 'page'])
const normalizeInteractionMode = (mode: unknown) => String(mode || '').trim().toLowerCase()
const normalizeProviderType = (provider: unknown) => String(provider || '').trim().toLowerCase()

export const isMobilePaymentDevice = (
  viewportWidth = typeof window === 'undefined' ? 1024 : window.innerWidth,
  userAgent = typeof navigator === 'undefined' ? '' : navigator.userAgent,
  maxTouchPoints = typeof navigator === 'undefined' ? 0 : navigator.maxTouchPoints,
) => viewportWidth < 768
  || /Android|iPhone|iPad|iPod|Mobile/i.test(userAgent)
  || (/Macintosh/i.test(userAgent) && maxTouchPoints > 1)

export const isCustomerSurchargePayment = (payment?: { fee_policy?: unknown } | null) => {
  const policy = String(payment?.fee_policy || '').trim().toLowerCase()
  return policy === 'customer_surcharge' || policy === 'legacy_customer_surcharge'
}

export const isRedirectPaymentInteractionMode = (mode: unknown) => {
  return REDIRECT_PAYMENT_INTERACTION_MODES.has(normalizeInteractionMode(mode))
}

export const resolvePaymentPresentationMode = (mode: unknown, providerType?: unknown, mobile = false): PaymentPresentationMode => {
  if (normalizeProviderType(providerType) === 'huifu' && normalizeInteractionMode(mode) === 'qr') {
    return mobile ? 'redirect' : 'qr'
  }
  return isRedirectPaymentInteractionMode(mode) ? 'redirect' : 'qr'
}

export const resolvePaymentLinkNavigationTarget = (automatic: boolean): PaymentLinkNavigationTarget => {
  return automatic ? 'current-tab' : 'new-window'
}

export const resolvePaymentInteractionLabelKey = (mode: unknown): PaymentInteractionLabelKey | null => {
  switch (normalizeInteractionMode(mode)) {
    case 'qr':
      return 'payment.modeQr'
    case 'redirect':
      return 'payment.modeRedirect'
    case 'wap':
      return 'payment.modeWap'
    case 'page':
      return 'payment.modePage'
    default:
      return null
  }
}

export const resolvePaymentResultTitleKey = (mode: unknown): PaymentResultTitleKey => {
  switch (normalizeInteractionMode(mode)) {
    case 'wap':
      return 'payment.modeWap'
    case 'page':
      return 'payment.modePage'
    case 'redirect':
      return 'payment.resultRedirectTitle'
    default:
      return 'payment.resultTitle'
  }
}

export const getPaymentResetPolicy = (reason: PaymentResetReason = 'generic'): PaymentResetPolicy => {
  if (reason === 'change_payment_method') {
    return {
      resumeLatestPayment: false,
      clearSelectedChannel: true,
      stopActivePaymentWatch: true,
    }
  }

  return {
    resumeLatestPayment: true,
    clearSelectedChannel: false,
    stopActivePaymentWatch: false,
  }
}

export const getCachedPaymentRestorePolicy = (): CachedPaymentRestorePolicy => ({
  startActivePaymentWatch: true,
  autoOpenPayLink: false,
})

export const shouldAutoOpenPaymentLink = (
  payment?: { interaction_mode?: unknown; provider_type?: unknown; pay_url?: unknown; fee_policy?: unknown } | null,
  mobile = isMobilePaymentDevice(),
) => {
  if (!payment || isCustomerSurchargePayment(payment)) return false
  const payURL = String(payment.pay_url || '').trim()
  if (normalizeProviderType(payment.provider_type) === 'huifu') {
    return payURL !== '' && (normalizeInteractionMode(payment.interaction_mode) === 'redirect' || mobile)
  }
  return isRedirectPaymentInteractionMode(payment.interaction_mode) && payURL !== ''
}
