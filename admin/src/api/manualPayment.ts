import { buildUrl, type ApiResponse } from './client'
import type { ManualPaymentReview } from './types'

async function request(id: number, key: string, body?: { received_amount: string; force: boolean; note: string }): Promise<ManualPaymentReview> {
  const response = await fetch(buildUrl(`/manual-payments/${id}${body ? '/confirm' : ''}`), {
    method: body ? 'POST' : 'GET',
    headers: { Authorization: `Bearer ${key}`, ...(body ? { 'Content-Type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined,
    cache: 'no-store',
    referrerPolicy: 'no-referrer',
  })
  const payload = await response.json() as ApiResponse<ManualPaymentReview>
  if (!response.ok || payload.status_code !== 0 || !payload.data) throw new Error(payload.msg || 'Unable to review payment')
  return payload.data
}

export const manualPaymentAPI = {
  review: (id: number, key: string) => request(id, key),
  confirm: (id: number, key: string, received_amount: string, force: boolean, note: string) =>
    request(id, key, { received_amount, force, note }),
}
