import { defineComponent, onMounted, type PropType, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown } from 'lucide-vue-next'
import { Button, cn, Dialog, FormField, Input, MultiSelect, Select, Switch, Textarea } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { PROVIDER_TYPES } from '../paymentChannelRules'
import { usePaymentChannelModal } from '../usePaymentChannelModal'
import { AlipayConfigForm } from './AlipayConfigForm'
import { BepusdtConfigForm } from './BepusdtConfigForm'
import { DujiaopayConfigForm } from './DujiaopayConfigForm'
import { EpayConfigForm } from './EpayConfigForm'
import { EpusdtConfigForm } from './EpusdtConfigForm'
import { HuifuConfigForm } from './HuifuConfigForm'
import { OkpayConfigForm } from './OkpayConfigForm'
import { PaypalConfigForm } from './PaypalConfigForm'
import { StripeConfigForm } from './StripeConfigForm'
import { TokenpayConfigForm } from './TokenpayConfigForm'
import { WechatConfigForm } from './WechatConfigForm'

const CONFIG_JSON_PLACEHOLDER = '{ "key": "value" }'

/** Create / edit payment channel dialog (v-model + channelId, emits success). */
export const PaymentChannelModal = defineComponent({
  name: 'PaymentChannelModal',
  props: {
    modelValue: Boolean,
    channelId: { type: Number as PropType<number | null>, default: null },
  },
  emits: { 'update:modelValue': (_v: boolean) => true, success: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const close = () => emit('update:modelValue', false)
    const s = usePaymentChannelModal({
      isOpen: () => props.modelValue,
      channelId: () => props.channelId ?? null,
      close,
      onSuccess: () => emit('success'),
    })
    const { form, configs } = s
    onMounted(() => void s.loadMemberLevels())

    const pc = (suffix: string) => t(`admin.paymentChannels.modal.${suffix}`)
    const providerOptions = () =>
      PROVIDER_TYPES.map((p) => ({
        value: p,
        label:
          p === 'dujiaopay'
            ? `${t('admin.paymentChannels.providerTypes.dujiaopay')}（${t('admin.paymentChannels.providerOfficialCertified')}）`
            : t(`admin.paymentChannels.providerTypes.${p}`),
      }))

    const money = (key: 'fee_rate' | 'fixed_fee' | 'min_amount' | 'max_amount', label: string, extra: { max?: string } = {}) => (
      <FormField label={pc(label)}>
        <Input
          type="number"
          step="0.01"
          min="0"
          max={extra.max}
          modelValue={form[key]}
          onUpdate:modelValue={(v: string | number) => (form[key] = v)}
          placeholder={pc(`${label}Placeholder`)}
        />
      </FormField>
    )

    const renderSection = (): VNodeChild => {
      switch (s.section.value) {
        case 'epay':
          return <EpayConfigForm config={configs.epay} />
        case 'paypal':
          return <PaypalConfigForm config={configs.paypal} />
        case 'stripe':
          return <StripeConfigForm config={configs.stripe} />
        case 'alipay':
          return <AlipayConfigForm config={configs.alipay} />
        case 'wechat':
          return (
            <WechatConfigForm
              config={configs.wechat}
              isEditing={s.isEditing.value}
              testing={s.testingWechatPublicKey.value}
              testResult={s.wechatTestResult.value}
              onTest={s.testWechatPublicKey}
            />
          )
        case 'bepusdt':
          return <BepusdtConfigForm config={configs.bepusdt} />
        case 'epusdt':
          return <EpusdtConfigForm config={configs.epusdt} />
        case 'tokenpay':
          return <TokenpayConfigForm config={configs.tokenpay} />
        case 'okpay':
          return <OkpayConfigForm config={configs.okpay} />
        case 'dujiaopay':
          return <DujiaopayConfigForm config={configs.dujiaopay} v-model:channelType={form.channel_type} />
        case 'huifu':
          return <HuifuConfigForm config={configs.huifu} />
        default:
          return null
      }
    }

    const renderBody = () => (
      <form
        class={cn('space-y-5', s.loading.value && 'pointer-events-none opacity-60')}
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.submit()
        }}
      >
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2 [&>*]:min-w-0">
          <FormField label={pc('name')} required>
            <Input v-model={form.name} placeholder={pc('namePlaceholder')} />
          </FormField>
          <FormField label={pc('icon')}>
            <MediaPicker v-model={form.icon} scene="common" size="sm" />
          </FormField>
          <FormField label={pc('providerType')}>
            <Select modelValue={form.provider_type} onUpdate:modelValue={(v) => (form.provider_type = String(v))} options={providerOptions()} />
          </FormField>
          {s.showChannelSelect.value && form.provider_type !== 'huifu' && (
            <FormField label={pc('channelType')}>
              <Select modelValue={form.channel_type} onUpdate:modelValue={(v) => (form.channel_type = String(v))} options={s.channelTypeOptions.value} />
            </FormField>
          )}
          <FormField label={pc('interactionMode')}>
            <Select modelValue={form.interaction_mode} onUpdate:modelValue={(v) => (form.interaction_mode = String(v))} options={s.interactionOptions.value} />
          </FormField>
          <FormField label={pc('sortOrder')}>
            <Input type="number" v-model={form.sort_order} placeholder="10" />
          </FormField>
          {money('fee_rate', 'feeRate', { max: '100' })}
          {money('fixed_fee', 'fixedFee')}
          {money('min_amount', 'minAmount')}
          {money('max_amount', 'maxAmount')}
          <div class="flex items-center md:pt-6">
            <Switch v-model={form.hide_amount_out_range} label={pc('hideAmountOutRange')} />
          </div>
          <div class="md:col-span-2">
            <FormField label={pc('paymentTypes')}>
              <MultiSelect
                modelValue={form.payment_types}
                onUpdate:modelValue={(v) => (form.payment_types = v.map(String))}
                options={s.paymentTypeOptions.value}
                placeholder={pc('paymentTypesPlaceholder')}
                searchable={false}
              />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={pc('paymentRoles')}>
              <MultiSelect
                modelValue={form.payment_roles}
                onUpdate:modelValue={(v) => (form.payment_roles = v.map(String))}
                options={s.paymentRoleOptions.value}
                placeholder={pc('paymentRolesPlaceholder')}
                searchable={false}
              />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={pc('memberLevels')}>
              <MultiSelect
                modelValue={form.member_levels}
                onUpdate:modelValue={(v) => (form.member_levels = v.map(Number))}
                options={s.memberLevelOptions.value}
                placeholder={pc('memberLevelsPlaceholder')}
                disabled={s.memberLevels.value.length === 0}
              />
            </FormField>
          </div>
          <div class="flex items-center">
            <Switch v-model={form.is_active} label={t('admin.common.enabled')} />
          </div>
        </div>

        {renderSection()}

        <div>
          <div class="mb-1.5 flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-fg/85">{pc('configJson')}</span>
            <button
              type="button"
              class="inline-flex items-center gap-1 text-xs text-muted hover:text-primary"
              onClick={() => (s.showAdvanced.value = !s.showAdvanced.value)}
            >
              {s.showAdvanced.value ? pc('advancedHide') : pc('advancedShow')}
              <ChevronDown class={cn('h-3.5 w-3.5 transition-transform', s.showAdvanced.value && 'rotate-180')} />
            </button>
          </div>
          {s.showAdvanced.value && (
            <>
              <Textarea v-model={form.config_json} rows={8} mono placeholder={CONFIG_JSON_PLACEHOLDER} />
              <p class="mt-1.5 text-xs text-muted">{pc('advancedClearHint')}</p>
            </>
          )}
        </div>

        {s.error.value && <div class="rounded-zs border border-line bg-danger-soft p-3 text-sm text-danger-text">{s.error.value}</div>}
        {/* hidden submit so Enter in a text field submits like the original <form> */}
        <button type="submit" class="hidden" aria-hidden="true" tabindex={-1} />
      </form>
    )

    return () => (
      <Dialog
        modelValue={props.modelValue}
        onUpdate:modelValue={(v: boolean) => !v && close()}
        title={s.isEditing.value ? pc('editTitle') : pc('createTitle')}
        size="lg"
        closeOnOverlay={false}
      >
        {{
          default: renderBody,
          footer: () => (
            <>
              <Button onClick={close}>{t('admin.common.cancel')}</Button>
              <Button variant="primary" loading={s.submitting.value} disabled={s.loading.value} onClick={() => void s.submit()}>
                {t('admin.common.save')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )
  },
})

export default PaymentChannelModal
