import { reactive } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminWalletRecharge } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import type { TranslateFn } from '@/utils/status'

export const RECHARGE_PROVIDER_TYPES = ['official', 'epay', 'epusdt', 'tokenpay', 'wallet'] as const
export const RECHARGE_STATUSES = ['pending', 'success', 'failed', 'expired'] as const

const CHANNEL_TYPE_KEYS: Record<string, string> = {
  wechat: 'wechat',
  alipay: 'alipay',
  qqpay: 'qqpay',
  paypal: 'paypal',
  stripe: 'stripe',
  usdt: 'usdt',
  'usdt-trc20': 'usdtTrc20',
  'usdc-trc20': 'usdcTrc20',
  trx: 'trx',
  balance: 'balance',
}

export const providerTypeLabel = (t: TranslateFn, value?: string) => {
  if (!value) return '-'
  return (RECHARGE_PROVIDER_TYPES as readonly string[]).includes(value) ? t(`admin.paymentChannels.providerTypes.${value}`) : value
}

export const channelTypeLabel = (t: TranslateFn, value?: string) => {
  if (!value) return '-'
  const key = CHANNEL_TYPE_KEYS[value]
  return key ? t(`admin.paymentChannels.channelTypes.${key}`) : value
}

/** Page logic for 钱包充值记录. */
export function useWalletRecharges() {
  const filters = reactive({
    rechargeNo: '',
    userId: '',
    userKeyword: '',
    paymentId: '',
    channelId: '',
    providerType: '__all__',
    status: '__all__',
    createdFrom: '',
    createdTo: '',
    paidFrom: '',
    paidTo: '',
  })

  const list = useListPage<AdminWalletRecharge>({
    fetchFn: (page, pageSize) =>
      adminAPI.getWalletRecharges(
        cleanParams({
          page,
          page_size: pageSize,
          recharge_no: filters.rechargeNo,
          user_id: filters.userId,
          user_keyword: filters.userKeyword,
          payment_id: filters.paymentId,
          channel_id: filters.channelId,
          provider_type: filters.providerType,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
          paid_from: toRFC3339(filters.paidFrom),
          paid_to: toRFC3339(filters.paidTo),
        }),
      ),
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  return { filters, list, refreshing, refresh }
}
