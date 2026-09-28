import { userApi } from './client'
import type {
  PageParams,
  ResellerApplyPayload,
  ResellerBalanceData,
  ResellerCustomDomainPayload,
  ResellerDashboardData,
  ResellerDomainData,
  ResellerLedgerData,
  ResellerManagementSnapshotData,
  ResellerOrderData,
  ResellerOrderDetailData,
  ResellerOrderListParams,
  ResellerOrderStatsData,
  ResellerOrderStatsParams,
  ResellerProductSettingDetailData,
  ResellerProductSettingListParams,
  ResellerProductSettingPreviewData,
  ResellerProductSettingUpdatePayload,
  ResellerSiteConfigPayload,
  ResellerSiteConfigSnapshotData,
  ResellerUploadResult,
  ResellerWithdrawApplyPayload,
  ResellerWithdrawData,
} from './types'

export interface ResellerLedgerParams extends PageParams {
  type?: string
  status?: string
  order_id?: number | string
  currency?: string
}

export const resellerAPI = {
  managementProfile: () => userApi.get<ResellerManagementSnapshotData>('/reseller/profile'),
  apply: (data: ResellerApplyPayload) => userApi.post<ResellerManagementSnapshotData>('/reseller/apply', data),
  domains: () => userApi.get<ResellerDomainData[]>('/reseller/domains'),
  submitDomain: (data: ResellerCustomDomainPayload) => userApi.post<ResellerDomainData>('/reseller/domains', data),
  siteConfig: () => userApi.get<ResellerSiteConfigSnapshotData>('/reseller/site-config'),
  updateSiteConfig: (data: ResellerSiteConfigPayload) => userApi.put<ResellerSiteConfigSnapshotData>('/reseller/site-config', data),
  uploadImage: (file: File) => {
    const form = new FormData()
    form.append('file', file)
    return userApi.post<ResellerUploadResult>('/reseller/upload', form)
  },
  productSettings: (params?: ResellerProductSettingListParams) =>
    userApi.get<ResellerProductSettingDetailData[]>('/reseller/product-settings', { params }),
  productSettingDetail: (productId: number) =>
    userApi.get<ResellerProductSettingDetailData>(`/reseller/product-settings/${productId}`),
  updateProductSettings: (productId: number, data: ResellerProductSettingUpdatePayload) =>
    userApi.put<ResellerProductSettingDetailData>(`/reseller/product-settings/${productId}`, data),
  previewProductSettings: (productId: number, data: ResellerProductSettingUpdatePayload) =>
    userApi.post<ResellerProductSettingPreviewData>(`/reseller/product-settings/${productId}/preview`, data),
  resetProductSetting: (productId: number, skuId = 0) =>
    userApi.delete<null>(`/reseller/product-settings/${productId}`, { params: { sku_id: skuId } }),
  dashboard: () => userApi.get<ResellerDashboardData>('/reseller/dashboard'),
  orders: (params?: ResellerOrderListParams) => userApi.get<ResellerOrderData[]>('/reseller/orders', { params }),
  orderStats: (params?: ResellerOrderStatsParams) => userApi.get<ResellerOrderStatsData>('/reseller/orders/stats', { params }),
  orderDetail: (orderNo: string) => userApi.get<ResellerOrderDetailData>(`/reseller/orders/${encodeURIComponent(orderNo)}`),
  balanceAccounts: (params?: PageParams) => userApi.get<ResellerBalanceData[]>('/reseller/balance-accounts', { params }),
  ledgerEntries: (params?: ResellerLedgerParams) => userApi.get<ResellerLedgerData[]>('/reseller/ledger-entries', { params }),
  withdraws: (params?: PageParams & { status?: string }) => userApi.get<ResellerWithdrawData[]>('/reseller/withdraws', { params }),
  applyWithdraw: (data: ResellerWithdrawApplyPayload) => userApi.post<ResellerWithdrawData>('/reseller/withdraws', data),
}
