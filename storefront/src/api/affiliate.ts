import { api, userApi } from './client'
import type {
  AffiliateClickPayload,
  AffiliateCommissionData,
  AffiliateDashboardData,
  AffiliateWithdrawApplyPayload,
  AffiliateWithdrawData,
  PageParams,
} from './types'

export const affiliateAPI = {
  trackClick: (data: AffiliateClickPayload) => api.post<null>('/public/affiliate/click', data),
  open: () => userApi.post<AffiliateDashboardData>('/affiliate/open'),
  dashboard: () => userApi.get<AffiliateDashboardData>('/affiliate/dashboard'),
  commissions: (params?: PageParams & { status?: string }) =>
    userApi.get<AffiliateCommissionData[]>('/affiliate/commissions', { params }),
  withdraws: (params?: PageParams & { status?: string }) =>
    userApi.get<AffiliateWithdrawData[]>('/affiliate/withdraws', { params }),
  applyWithdraw: (data: AffiliateWithdrawApplyPayload) => userApi.post<AffiliateWithdrawData>('/affiliate/withdraws', data),
}
