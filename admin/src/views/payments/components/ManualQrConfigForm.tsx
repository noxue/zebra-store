import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, FormField, Input, Switch } from '@/components/ui'
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
    const field = (key: keyof ManualQrConfig, title: string, type = 'text') => (
      <FormField label={label(title)}>
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
          <FormField label={label('qrImage')} required>
            <MediaPicker modelValue={props.config.qr_image_url} scene="common" onUpdate:modelValue={(v) => { props.config.qr_image_url = String(v) }} />
          </FormField>
          {field('review_base_url', 'reviewBase')}
        </div>
        <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          <div class="md:col-span-2 text-sm font-semibold text-fg">Telegram</div>
          {field('telegram_token', 'telegramToken', 'password')}
          {field('telegram_chat_id', 'telegramChatId')}
          {props.savedId && <div class="flex flex-wrap gap-2 md:col-span-2">
            <Button disabled={props.busy} onClick={() => emit('beginBind')}>{label('bindTelegram')}</Button>
            {props.bindUrl && <><a href={props.bindUrl} target="_blank" rel="noopener noreferrer" class="self-center text-sm text-primary underline">{label('openTelegram')}</a>
              <Button disabled={props.busy} onClick={() => emit('finishBind')}>{label('checkBinding')}</Button></>}
          </div>}
        </div>
        <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          <div class="md:col-span-2 text-sm font-semibold text-fg">{label('webhooks')}</div>
          {field('wecom_webhook_url', 'wecomWebhook')}
          {field('dingtalk_webhook_url', 'dingtalkWebhook')}
          {field('dingtalk_secret', 'dingtalkSecret', 'password')}
        </div>
        <div class="grid grid-cols-1 gap-4 border-t border-line pt-4 md:grid-cols-2">
          <div class="md:col-span-2 text-sm font-semibold text-fg">{label('email')}</div>
          {field('smtp_host', 'smtpHost')}
          {field('smtp_port', 'smtpPort', 'number')}
          {field('smtp_username', 'smtpUsername')}
          {field('smtp_password', 'smtpPassword', 'password')}
          {field('smtp_from', 'smtpFrom', 'email')}
          {field('smtp_to', 'smtpTo', 'email')}
          <Switch modelValue={props.config.smtp_tls} onUpdate:modelValue={(v: boolean) => { props.config.smtp_tls = v }} label={label('smtpTls')} />
          <Switch modelValue={props.config.smtp_ssl} onUpdate:modelValue={(v: boolean) => { props.config.smtp_ssl = v }} label={label('smtpSsl')} />
        </div>
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
