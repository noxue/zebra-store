import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ExternalLink } from 'lucide-vue-next'
import { CopyButton } from '@/components/common/CopyButton'
import { QrCode } from '@/components/common/QrCode'
import { Button } from '@/components/ui'
import type { CryptoDetail } from '@/utils/orderPayment'

/** Crypto payment details (token/chain/amount/address) with copy action. */
export const CryptoDetails = defineComponent({
  name: 'CryptoDetails',
  props: {
    details: { type: Array as PropType<CryptoDetail[]>, required: true },
    walletAddress: { type: String, default: '' },
  },
  setup(props) {
    const { t } = useI18n()
    return () =>
      props.details.length === 0 ? null : (
        <div class="w-full space-y-2 rounded-zs border border-line bg-surface-strong p-3 text-left">
          {props.details.map((item) => (
            <div key={item.key} class="flex flex-col gap-1 border-b border-line pb-2 last:border-b-0 last:pb-0 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
              <span class="shrink-0 text-xs text-muted">{t(item.labelKey)}</span>
              <span class="min-w-0 break-all text-sm font-bold text-fg sm:text-right zs-num">
                {item.value}
                {item.detail && <span class="ml-1 font-normal text-muted">({item.detail})</span>}
              </span>
            </div>
          ))}
          {props.walletAddress && (
            <div class="flex justify-end pt-1">
              <CopyButton value={props.walletAddress} label={t('payment.copyWalletAddress')} />
            </div>
          )}
        </div>
      )
  },
})

/** QR code (local render or provider image) or open-link panel for a created payment. */
export const PaymentQrPanel = defineComponent({
  name: 'PaymentQrPanel',
  props: {
    showQr: Boolean,
    qrContent: { type: String, default: '' },
    qrImage: { type: String as PropType<string | null>, default: null },
    qrFallback: Boolean,
    title: { type: String, default: '' },
    payLink: { type: String, default: '' },
    details: { type: Array as PropType<CryptoDetail[]>, default: () => [] },
    walletAddress: { type: String, default: '' },
    opened: Boolean,
    openedTip: { type: String, default: '' },
    telegramHint: Boolean,
  },
  emits: { open: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => (
      <div class="flex flex-col items-center justify-center gap-4 rounded-zs-lg border border-line zs-soft-bg p-5 text-center sm:p-6">
        {props.showQr ? (
          <>
            {props.title && <div class="text-sm font-bold text-muted">{props.title}</div>}
            {props.qrImage ? (
              <div class="inline-flex rounded-zs-lg border-4 border-primary-soft bg-white p-3 shadow-zs">
                <img src={props.qrImage} alt="QR" class="size-52 object-contain" />
              </div>
            ) : (
              <QrCode value={props.qrContent} size={210} />
            )}
            {props.qrFallback && <div class="max-w-sm text-xs text-muted">{t('payment.qrFallbackHint')}</div>}
            <CryptoDetails details={props.details} walletAddress={props.walletAddress} />
            {props.payLink && <CopyButton value={props.payLink} label={t('payment.copyPayLink')} variant="secondary" />}
          </>
        ) : (
          <>
            <div class="zs-gradient-bg flex size-16 items-center justify-center rounded-full text-on-primary shadow-zs zs-float">
              <ExternalLink class="size-7" />
            </div>
            <div class="text-sm text-muted">{t('payment.redirectTip')}</div>
            <CryptoDetails details={props.details} walletAddress={props.walletAddress} />
            <div class="flex flex-wrap items-center justify-center gap-3">
              <Button size="lg" disabled={!props.payLink} onClick={() => emit('open')}>
                <ExternalLink class="size-4" />
                {t('payment.openPayLink')}
              </Button>
              {props.payLink && (
                <CopyButton value={props.payLink} label={t('payment.copyPayLink')} variant="secondary" size="md" />
              )}
            </div>
            {props.opened && <div class="text-xs font-bold text-success-text">{props.openedTip}</div>}
          </>
        )}
        {props.telegramHint && <div class="max-w-md text-xs text-muted">{t('payment.telegramExternalHint')}</div>}
      </div>
    )
  },
})
