import { defineComponent, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Eye, EyeOff, Lock } from 'lucide-vue-next'
import { Input } from '@/components/ui'

/** Password field with lock prefix and show/hide toggle. */
export const PasswordInput = defineComponent({
  name: 'PasswordInput',
  props: {
    modelValue: { type: String, default: '' },
    placeholder: { type: String, default: '' },
    invalid: Boolean,
    autocomplete: { type: String, default: 'current-password' },
    id: { type: String, default: undefined },
  },
  emits: { 'update:modelValue': (_v: string) => true, blur: () => true, enter: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const visible = ref(false)
    return () => (
      <Input
        id={props.id}
        type={visible.value ? 'text' : 'password'}
        modelValue={props.modelValue}
        placeholder={props.placeholder}
        invalid={props.invalid}
        autocomplete={props.autocomplete}
        onUpdate:modelValue={(v: string) => emit('update:modelValue', v)}
        onBlur={() => emit('blur')}
        onEnter={() => emit('enter')}
      >
        {{
          prefix: () => <Lock class="size-4" />,
          suffix: () => (
            <button
              type="button"
              class="flex size-8 items-center justify-center rounded-full text-muted transition hover:bg-primary-soft hover:text-primary-text"
              aria-label={visible.value ? t('auth.common.hidePassword') : t('auth.common.showPassword')}
              onClick={() => (visible.value = !visible.value)}
            >
              {visible.value ? <EyeOff class="size-4" /> : <Eye class="size-4" />}
            </button>
          ),
        }}
      </Input>
    )
  },
})
