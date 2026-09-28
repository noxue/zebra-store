import { userApi } from './client'
import type {
  CaptchaPayload,
  GiftCardRedeemResult,
  PageParams,
  PaymentCaptureResult,
  PaymentChannel,
  StatusStats,
  WalletAccountData,
  WalletRechargeOrderData,
  WalletRechargePayload,
  WalletRechargeResult,
  WalletTransactionData,
} from './types'

export interface RechargeListParams extends PageParams {
  status?: string
  recharge_no?: string
}

export const walletAPI = {
  getPaymentChannels: (amount: string) => userApi.post<PaymentChannel[]>('/wallet/payment-channels', { amount }),
  account: () => userApi.get<WalletAccountData>('/wallet'),
  transactions: (params?: PageParams) => userApi.get<WalletTransactionData[]>('/wallet/transactions', { params }),
  recharge: (data: WalletRechargePayload) => userApi.post<WalletRechargeResult>('/wallet/recharge', data),
  rechargeOrders: (params?: RechargeListParams) => userApi.get<WalletRechargeOrderData[]>('/wallet/recharges', { params }),
  rechargeStats: (params?: RechargeListParams) => userApi.get<StatusStats>('/wallet/recharges/stats', { params }),
  rechargeDetail: (rechargeNo: string) =>
    userApi.get<WalletRechargeResult>(`/wallet/recharges/${encodeURIComponent(rechargeNo)}`),
  captureRechargePayment: (paymentId: number) =>
    userApi.post<PaymentCaptureResult>(`/wallet/recharge/payments/${paymentId}/capture`),
}

export const giftCardAPI = {
  redeem: (data: { code: string; captcha_payload?: CaptchaPayload }) =>
    userApi.post<GiftCardRedeemResult>('/gift-cards/redeem', data),
}
