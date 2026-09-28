import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { KeyRound } from 'lucide-vue-next'
import type { GuestCredentials } from '@/api/order'
import { Alert, Button, Card, Input, Kitty } from '@/components/ui'

/** Email + order password form for guest order access. */
export const GuestAuthForm = defineComponent({
  name: 'GuestAuthForm',
  props: {
    modelValue: { type: Object as PropType<GuestCredentials>, default: () => ({ email: '', order_password: '' }) },
    title: { type: String, required: true },
    hint: { type: String, default: '' },
    error: { type: String, default: '' },
    submitText: { type: String, required: true },
    clearText: { type: String, default: '' },
    submitVariant: { type: String as PropType<'primary' | 'secondary'>, default: 'primary' },
  },
  emits: { 'update:modelValue': (_v: GuestCredentials) => true, submit: () => true, clear: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const update = (patch: Partial<GuestCredentials>) => emit('update:modelValue', { ...props.modelValue, ...patch })
    return () => (
      <Card>
        <div class="flex items-start gap-4">
          <div class="hidden h-20 w-24 shrink-0 sm:block">
            <Kitty />
          </div>
          <div class="min-w-0 flex-1">
            <h2 class="zs-title flex items-center gap-2 text-xl text-fg">
              <KeyRound class="size-5 text-primary" />
              {props.title}
            </h2>
            {props.hint && <p class="mt-1 text-xs text-muted">{props.hint}</p>}
            <p class="mt-1 text-xs text-secondary-text">{t('orderZs.guestAuthMascot')}</p>
          </div>
        </div>
        <form
          class="mt-5"
          onSubmit={(e: Event) => {
            e.preventDefault()
            emit('submit')
          }}
        >
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <Input type="email" autocomplete="email" modelValue={props.modelValue.email} placeholder={t('guestOrders.emailPlaceholder')} onUpdate:modelValue={(v: string) => update({ email: v })} />
            <Input
              type="password"
              autocomplete="current-password"
              modelValue={props.modelValue.order_password}
              placeholder={t('guestOrders.passwordPlaceholder')}
              onUpdate:modelValue={(v: string) => update({ order_password: v })}
            />
          </div>
          {props.error && (
            <div class="mt-4">
              <Alert tone="error">{props.error}</Alert>
            </div>
          )}
          <div class="mt-5 flex flex-wrap items-center gap-3">
            <Button type="submit" variant={props.submitVariant}>
              {props.submitText}
            </Button>
            {props.clearText && (
              <Button variant="link" size="sm" onClick={() => emit('clear')}>
                {props.clearText}
              </Button>
            )}
          </div>
        </form>
      </Card>
    )
  },
})
