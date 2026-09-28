import { defineComponent, type PropType } from 'vue'
import { Button } from '@/components/ui'
import type { AdminGatewaySecurityTestResult } from '@/api/types'
import type { WechatConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Official WeChat Pay v3 config + "test WeChat Pay public key" box (edit mode only). */
export const WechatConfigForm = defineComponent({
  name: 'WechatConfigForm',
  props: {
    config: { type: Object as PropType<WechatConfig>, required: true },
    isEditing: Boolean,
    testing: Boolean,
    testResult: { type: Object as PropType<AdminGatewaySecurityTestResult | null>, default: null },
  },
  emits: { test: () => true },
  setup(props, { emit }) {
    const { m, text, select } = useConfigFields()
    const passFail = (ok: boolean) => (ok ? m('wechatPublicKeyTestPassed') : m('wechatPublicKeyTestFailed'))

    const renderTestBox = () => {
      const platformOnly = props.config.verification_mode === 'platform_certificate'
      const r = props.testResult
      return (
        <div class="mt-4 rounded-zs border border-line bg-surface-solid p-3">
          <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <div class="text-sm font-medium text-fg">{m('wechatPublicKeyTestTitle')}</div>
              <p class="mt-1 text-xs text-muted">{m('wechatPublicKeyTestHint')}</p>
            </div>
            <Button
              size="sm"
              class="w-full shrink-0 sm:w-auto"
              disabled={!props.isEditing || platformOnly}
              loading={props.testing}
              onClick={() => emit('test')}
            >
              {props.testing ? m('wechatPublicKeyTesting') : m('wechatPublicKeyTest')}
            </Button>
          </div>
          {!props.isEditing ? (
            <p class="mt-2 text-xs text-warning-text">{m('wechatPublicKeyTestSaveFirst')}</p>
          ) : platformOnly ? (
            <p class="mt-2 text-xs text-warning-text">{m('wechatPublicKeyTestModeRequired')}</p>
          ) : null}
          {r && (
            <div class="mt-3 rounded-zs-sm border border-line bg-success-soft p-3 text-xs text-success-text">
              <div class="font-medium">{m('wechatPublicKeyTestSuccess')}</div>
              <div class="mt-2 grid gap-1 sm:grid-cols-2">
                <span>{m('wechatPublicKeyTestSerial')}</span>
                <span class="break-all font-mono sm:text-right">{r.response_serial}</span>
                <span>{m('wechatPublicKeyTestRequestSignature')}</span>
                <span class="sm:text-right">{passFail(r.request_signature_accepted)}</span>
                <span>{m('wechatPublicKeyTestResponseSignature')}</span>
                <span class="sm:text-right">{passFail(r.response_signature_valid)}</span>
                <span>{m('wechatPublicKeyTestEcho')}</span>
                <span class="sm:text-right">{passFail(r.echo_message_matched)}</span>
              </div>
            </div>
          )}
        </div>
      )
    }

    return () => {
      const c = props.config
      const usesPublicKey = c.verification_mode !== 'platform_certificate'
      return (
        <ProviderSection title="wechatSection" hint="wechatHint">
          {{
            default: () => (
              <>
                {text(c, 'appid', 'wechatAppId')}
                {text(c, 'mchid', 'wechatMerchantId')}
                {text(c, 'merchant_serial_no', 'wechatMerchantSerialNo')}
                {text(c, 'api_v3_key', 'wechatApiV3Key')}
                {select(
                  c,
                  'verification_mode',
                  'wechatVerificationMode',
                  [
                    { value: 'platform_certificate', label: 'wechatVerificationModePlatformCertificate' },
                    { value: 'wechatpay_public_key', label: 'wechatVerificationModePublicKey' },
                    { value: 'combined', label: 'wechatVerificationModeCombined' },
                  ],
                  { wide: true, hint: 'wechatVerificationModeHint' },
                )}
                {usesPublicKey && text(c, 'wechatpay_public_key_id', 'wechatPublicKeyId')}
                {usesPublicKey && text(c, 'wechatpay_public_key', 'wechatPublicKey', { wide: true, rows: 4 })}
                {text(c, 'merchant_private_key', 'wechatMerchantPrivateKey', { wide: true, rows: 4 })}
                {text(c, 'notify_url', 'wechatNotifyUrl', { hint: 'wechatNotifyUrlHint' })}
                {text(c, 'h5_redirect_url', 'wechatH5RedirectUrl')}
                {text(c, 'h5_type', 'wechatH5Type')}
                {text(c, 'h5_wap_url', 'wechatH5WapUrl')}
                {text(c, 'h5_wap_name', 'wechatH5WapName', { wide: true })}
                {text(c, 'target_currency', 'targetCurrency')}
                {text(c, 'exchange_rate', 'exchangeRate')}
              </>
            ),
            after: renderTestBox,
          }}
        </ProviderSection>
      )
    }
  },
})
