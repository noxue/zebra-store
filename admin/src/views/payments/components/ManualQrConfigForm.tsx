import { defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import QRCode from 'qrcode'
import { Button, FormField, Input, MultiSelect, Select, Switch } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { copyText } from '@/utils/clipboard'
import type { ManualQrConfig } from '../paymentChannelRules'

/** Merchant collection QR and human confirmation delivery settings. */
export const ManualQrConfigForm = defineComponent({
  name: 'ManualQrConfigForm',
  props: {
    config: { type: Object as PropType<ManualQrConfig>, required: true },
    savedId: { type: Number as PropType<number | null>, default: null },
    keyValue: { type: String, default: '' },
    customKey: { type: String, default: '' },
    bindUrl: { type: String, default: '' },
    errors: { type: Object as PropType<Record<string, string>>, default: () => ({}) },
    busy: Boolean,
  },
  emits: {
    'update:customKey': (_v: string) => true,
    setKey: () => true,
    test: () => true,
    beginBind: () => true,
    finishBind: () => true,
  },
  setup(props, { emit }) {
    const { t } = useI18n()
    const label = (key: string) => t(`admin.paymentChannels.manualQr.${key}`)
    const bindQr = ref('')
    watch(() => props.bindUrl, async (url) => {
      try {
        bindQr.value = url ? await QRCode.toDataURL(url, { margin: 1, width: 180 }) : ''
      } catch {
        bindQr.value = ''
      }
    }, { immediate: true })
    const field = (key: keyof ManualQrConfig, title: string, type = 'text') => (
      <FormField label={label(title)} error={props.errors[key]} data-manual-error={props.errors[key] ? 'true' : undefined}>
        <Input
          type={type}
          modelValue={String(props.config[key] ?? '')}
          onUpdate:modelValue={(v: string | number) => { (props.config as unknown as Record<string, string>)[key] = String(v) }}
        />
      </FormField>
    )
    return () => (
      <section class="space-y-5 rounded-zs-lg border border-line bg-surface-muted p-4">
        <div>
          <h4 class="zs-display text-sm text-fg">{label('title')}</h4>
          <p class="mt-1 text-xs text-muted">{label('hint')}</p>
        </div>
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <FormField label={label('paymentMethods')} required error={props.errors.supported_channel_types} data-manual-error={props.errors.supported_channel_types ? 'true' : undefined}>
            <MultiSelect modelValue={props.config.supported_channel_types}
              onUpdate:modelValue={(v) => { props.config.supported_channel_types = v.map(String) }}
              options={[{ value: 'alipay', label: label('alipay') }, { value: 'wechat', label: label('wechat') }]}
              searchable={false} />
          </FormField>
          {field('review_base_url', 'reviewBase')}
        </div>
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          {props.config.supported_channel_types.includes('alipay') && <FormField label={label('alipayQr')} required error={props.errors.alipay_qr_image_url} data-manual-error={props.errors.alipay_qr_image_url ? 'true' : undefined}>
            <MediaPicker modelValue={props.config.alipay_qr_image_url} scene="common" onUpdate:modelValue={(v) => { props.config.alipay_qr_image_url = String(v) }} />
          </FormField>}
          {props.config.supported_channel_types.includes('wechat') && <FormField label={label('wechatQr')} required error={props.errors.wechat_qr_image_url} data-manual-error={props.errors.wechat_qr_image_url ? 'true' : undefined}>
            <MediaPicker modelValue={props.config.wechat_qr_image_url} scene="common" onUpdate:modelValue={(v) => { props.config.wechat_qr_image_url = String(v) }} />
          </FormField>}
        </div>
        <div class="border-t border-line pt-4">
          <FormField label={label('notificationType')} required error={props.errors.notification_type} data-manual-error={props.errors.notification_type ? 'true' : undefined}>
            <Select modelValue={props.config.notification_type} onUpdate:modelValue={(v) => { props.config.notification_type = String(v) }}
              options={['telegram', 'wecom', 'dingtalk', 'email'].map((v) => ({ value: v, label: label(v) }))} />
          </FormField>
        </div>
        {props.config.notification_type === 'telegram' && <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          {field('telegram_token', 'telegramToken', 'password')}
          <div class="space-y-2">
            <p class="text-xs font-medium text-fg/85">{label('boundChats')}</p>
            {props.config.telegram_chat_ids.length === 0 && <p class="text-xs text-muted">{label('noBoundChats')}</p>}
            {props.config.telegram_chat_ids.map((chat) => <div key={chat.id} class="flex items-center justify-between gap-2 rounded-zs border border-line bg-surface p-2 text-xs">
              <span class="min-w-0 truncate">{chat.name || chat.id} <span class="text-muted">({chat.id})</span></span>
              <Button size="sm" variant="ghost" onClick={() => { props.config.telegram_chat_ids = props.config.telegram_chat_ids.filter((item) => item.id !== chat.id) }}>{label('removeChat')}</Button>
            </div>)}
          </div>
          {props.savedId && <div class="space-y-3 md:col-span-2">
            <Button disabled={props.busy} onClick={() => emit('beginBind')}>{label('bindTelegram')}</Button>
            {props.bindUrl && <div class="flex flex-wrap items-center gap-4 rounded-zs border border-line bg-surface p-3">
              {bindQr.value && <img src={bindQr.value} alt={label('bindTelegram')} class="h-36 w-36 rounded-zs" />}
              <div class="min-w-0 space-y-2">
                <a href={props.bindUrl} target="_blank" rel="noopener noreferrer" class="block break-all text-sm text-primary underline">{props.bindUrl}</a>
                <div class="flex gap-2"><Button size="sm" onClick={() => { void copyText(props.bindUrl) }}>{label('copyLink')}</Button>
                  <Button size="sm" disabled={props.busy} onClick={() => emit('finishBind')}>{label('checkBinding')}</Button></div>
              </div>
            </div>}
          </div>}
          {!props.savedId && <p class="text-xs text-muted md:col-span-2">{label('saveToBind')}</p>}
        </div>}
        {props.config.notification_type === 'wecom' && <div class="border-t border-line pt-4">{field('wecom_webhook_url', 'wecomWebhook')}</div>}
        {props.config.notification_type === 'dingtalk' && <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          {field('dingtalk_webhook_url', 'dingtalkWebhook')}
          {field('dingtalk_secret', 'dingtalkSecret', 'password')}
        </div>}
        {props.config.notification_type === 'email' && <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          {field('smtp_host', 'smtpHost')}
          {field('smtp_port', 'smtpPort', 'number')}
          {field('smtp_username', 'smtpUsername')}
          {field('smtp_password', 'smtpPassword', 'password')}
          {field('smtp_from', 'smtpFrom', 'email')}
          {field('smtp_to', 'smtpTo', 'email')}
          <FormField error={props.errors.smtp_tls} data-manual-error={props.errors.smtp_tls ? 'true' : undefined}>
            <Switch modelValue={props.config.smtp_tls} onUpdate:modelValue={(v: boolean) => { props.config.smtp_tls = v }} label={label('smtpTls')} />
          </FormField>
          <Switch modelValue={props.config.smtp_ssl} onUpdate:modelValue={(v: boolean) => { props.config.smtp_ssl = v }} label={label('smtpSsl')} />
        </div>}
        {props.savedId && <div class="space-y-3 border-t border-line pt-4">
          <div class="text-sm font-semibold text-fg">{label('apiKey')}</div>
          <p class="text-xs text-muted">{label('keyHint')}</p>
          <Input modelValue={props.customKey} onUpdate:modelValue={(v: string | number) => emit('update:customKey', String(v))} placeholder={label('customKey')} />
          <div class="flex flex-wrap gap-2">
            <Button disabled={props.busy} onClick={() => emit('setKey')}>{label('setKey')}</Button>
            <Button disabled={props.busy} onClick={() => emit('test')}>{label('testSend')}</Button>
          </div>
          {props.keyValue && <div class="flex items-center gap-2 rounded-zs border border-line bg-surface-strong p-3">
            <span class="min-w-0 flex-1 break-all font-mono text-xs text-fg">{props.keyValue}</span>
            <Button onClick={() => { void copyText(props.keyValue) }}>{label('copyKey')}</Button>
          </div>}
        </div>}
      </section>
    )
  },
})
