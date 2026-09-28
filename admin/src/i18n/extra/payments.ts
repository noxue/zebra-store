import type { ExtraMessages } from '../index'

// Zebra Store additions for the payments group (payments / payment-channels / callback-routes).
const messages: ExtraMessages = {
  'zh-CN': {
    admin: {
      payments: {
        filterChannelTypeAll: '全部支付方式',
        detailPaymentNo: '支付单号',
        detailPayableAmount: '应付金额',
        detailProviderTradeNo: '平台交易号',
        detailSupersededAt: '被替换时间',
        detailSupersededBy: '替换为支付',
      },
    },
  },
  'zh-TW': {
    admin: {
      payments: {
        filterChannelTypeAll: '全部支付方式',
        detailPaymentNo: '支付單號',
        detailPayableAmount: '應付金額',
        detailProviderTradeNo: '平台交易號',
        detailSupersededAt: '被替換時間',
        detailSupersededBy: '替換為支付',
      },
    },
  },
  'en-US': {
    admin: {
      payments: {
        filterChannelTypeAll: 'All payment methods',
        detailPaymentNo: 'Payment No.',
        detailPayableAmount: 'Payable amount',
        detailProviderTradeNo: 'Provider trade No.',
        detailSupersededAt: 'Superseded at',
        detailSupersededBy: 'Superseded by payment',
      },
    },
  },
}

export default messages
