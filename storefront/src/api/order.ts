import { userApi, type RequestOptions } from './client'
import type {
  CreateAndPayPayload,
  CreateOrderResult,
  CreatePaymentPayload,
  GuestCreateAndPayPayload,
  GuestOrderPreviewPayload,
  Order,
  OrderListParams,
  OrderPreview,
  OrderPreviewPayload,
  PaymentCaptureResult,
  PaymentChannel,
  PaymentChannelsPayload,
  PaymentCreateResult,
  StatusStats,
} from './types'

export interface GuestCredentials {
  email: string
  order_password: string
}

/** `Guest base64url(lower(trim(email)) + "\n" + trim(order_password))` */
export const encodeGuestAuthorization = (email: string, orderPassword: string): string => {
  const bytes = new TextEncoder().encode(`${email.trim().toLowerCase()}\n${orderPassword.trim()}`)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  const token = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '')
  return `Guest ${token}`
}

const guestOptions = (auth: GuestCredentials, options: RequestOptions = {}): RequestOptions => ({
  ...options,
  headers: { ...(options.headers || {}), Authorization: encodeGuestAuthorization(auth.email, auth.order_password) },
})

export const userOrderAPI = {
  preview: (data: OrderPreviewPayload) => userApi.post<OrderPreview>('/orders/preview', data),
  getPaymentChannels: (data: PaymentChannelsPayload) => userApi.post<PaymentChannel[]>('/order/payment-channels', data),
  create: (data: CreateAndPayPayload) => userApi.post<CreateOrderResult>('/orders', data),
  createAndPay: (data: CreateAndPayPayload) => userApi.post<CreateOrderResult>('/orders/create-and-pay', data),
  list: (params?: OrderListParams) => userApi.get<Order[]>('/orders', { params }),
  stats: (params?: OrderListParams) => userApi.get<StatusStats>('/orders/stats', { params }),
  detail: (orderNo: string, options?: RequestOptions) => userApi.get<Order>(`/orders/${encodeURIComponent(orderNo)}`, options),
  cancel: (orderNo: string) => userApi.post<Order | null>(`/orders/${encodeURIComponent(orderNo)}/cancel`),
  downloadFulfillment: (orderNo: string) => userApi.blob(`/orders/${encodeURIComponent(orderNo)}/fulfillment/download`),
}

export const guestOrderAPI = {
  preview: (data: GuestOrderPreviewPayload) => userApi.post<OrderPreview>('/guest/orders/preview', data),
  create: (data: GuestCreateAndPayPayload) => userApi.post<CreateOrderResult>('/guest/orders', data),
  createAndPay: (data: GuestCreateAndPayPayload) => userApi.post<CreateOrderResult>('/guest/orders/create-and-pay', data),
  list: (auth: GuestCredentials, params?: OrderListParams) =>
    userApi.get<Order[]>('/guest/orders', guestOptions(auth, { params })),
  detail: (orderNo: string, auth: GuestCredentials, options?: RequestOptions) =>
    userApi.get<Order>(`/guest/orders/${encodeURIComponent(orderNo)}`, guestOptions(auth, options)),
  downloadFulfillment: (orderNo: string, auth: GuestCredentials) =>
    userApi.blob(`/guest/orders/${encodeURIComponent(orderNo)}/fulfillment/download`, guestOptions(auth)),
  createPayment: (auth: GuestCredentials, data: CreatePaymentPayload) =>
    userApi.post<PaymentCreateResult>('/guest/payments', data, guestOptions(auth)),
  capturePayment: (id: number, auth: GuestCredentials) =>
    userApi.post<PaymentCaptureResult>(`/guest/payments/${id}/capture`, {}, guestOptions(auth)),
  latestPayment: (auth: GuestCredentials, params: { order_no: string }) =>
    userApi.get<PaymentCreateResult>('/guest/payments/latest', guestOptions(auth, { params, silentBusinessError: true })),
}

export const paymentAPI = {
  create: (data: CreatePaymentPayload) => userApi.post<PaymentCreateResult>('/payments', data),
  capture: (id: number) => userApi.post<PaymentCaptureResult>(`/payments/${id}/capture`),
  latest: (params: { order_no: string }) =>
    userApi.get<PaymentCreateResult>('/payments/latest', { params, silentBusinessError: true }),
}
