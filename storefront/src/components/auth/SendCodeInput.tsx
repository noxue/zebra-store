import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { MailCheck } from 'lucide-vue-next'
import { Button, Input } from '@/components/ui'

/** Email verification code input with a send button + cooldown. */
export const SendCodeInput = defineComponent({
  name: 'SendCodeInput',
  props: {
    modelValue: { type: String, default: '' },
    placeholder: { type: String, default: '' },
    countdown: { type: Number, default: 0 },
    sending: Boolean,
  },
  emits: { 'update:modelValue': (_v: string) => true, send: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => (
      <div class="flex gap-2.5">
        <Input
          class="flex-1"
          modelValue={props.modelValue}
          placeholder={props.placeholder}
          inputmode="numeric"
          autocomplete="one-time-code"
          maxlength={12}
          onUpdate:modelValue={(v: string) => emit('update:modelValue', v)}
        >
          {{ prefix: () => <MailCheck class="size-4" /> }}
        </Input>
        <Button variant="soft" class="min-w-28 shrink-0" loading={props.sending} disabled={props.countdown > 0} onClick={() => emit('send')}>
          {props.countdown > 0 ? t('auth.common.countdown', { seconds: props.countdown }) : t('auth.common.sendCode')}
        </Button>
      </div>
    )
  },
})
