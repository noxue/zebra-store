import { defineComponent, type PropType } from 'vue'
import { cn, type IconComponent } from './cn'

export const inputBase =
  'w-full rounded-zs-sm border border-line-strong bg-surface-strong text-fg placeholder:text-muted/70 transition-colors focus:border-primary focus:outline-none focus:shadow-[var(--zs-focus-ring)] disabled:cursor-not-allowed disabled:opacity-60'

export const Input = defineComponent({
  name: 'ZsInput',
  props: {
    modelValue: { type: [String, Number] as PropType<string | number | null | undefined>, default: '' },
    type: { type: String, default: 'text' },
    placeholder: String,
    disabled: Boolean,
    readonly: Boolean,
    icon: { type: [Object, Function] as PropType<IconComponent>, default: undefined },
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
    min: [String, Number],
    max: [String, Number],
    step: [String, Number],
    maxlength: [String, Number],
    autocomplete: String,
    name: String,
    id: String,
    inputClass: String,
    mono: Boolean,
  },
  emits: {
    'update:modelValue': (_v: string | number) => true,
    enter: () => true,
    blur: (_e: FocusEvent) => true,
    focus: (_e: FocusEvent) => true,
    paste: (_e: ClipboardEvent) => true,
    keydown: (_e: KeyboardEvent) => true,
  },
  setup(props, { emit, slots }) {
    const onInput = (e: Event) => {
      const el = e.target as HTMLInputElement
      if (props.type === 'number') {
        emit('update:modelValue', el.value === '' ? '' : Number(el.value))
      } else {
        emit('update:modelValue', el.value)
      }
    }
    return () => {
      const Icon = props.icon
      return (
        <div class="relative flex w-full items-center">
          {Icon && <Icon class="pointer-events-none absolute left-3 h-4 w-4 text-muted" />}
          <input
            id={props.id}
            name={props.name}
            type={props.type}
            value={props.modelValue ?? ''}
            placeholder={props.placeholder}
            disabled={props.disabled}
            readonly={props.readonly}
            min={props.min}
            max={props.max}
            step={props.step}
            maxlength={props.maxlength}
            autocomplete={props.autocomplete}
            class={cn(
              inputBase,
              props.size === 'sm' ? 'h-8 px-2.5 text-xs' : 'h-9 px-3 text-sm',
              Icon && 'pl-9',
              slots.suffix && 'pr-10',
              props.mono && 'font-mono',
              props.inputClass,
            )}
            onInput={onInput}
            onKeydown={(e: KeyboardEvent) => {
              emit('keydown', e)
              if (e.key === 'Enter' && !e.isComposing) emit('enter')
            }}
            onBlur={(e: FocusEvent) => emit('blur', e)}
            onFocus={(e: FocusEvent) => emit('focus', e)}
            onPaste={(e: ClipboardEvent) => emit('paste', e)}
          />
          {slots.suffix && <div class="absolute right-2 flex items-center">{slots.suffix()}</div>}
        </div>
      )
    }
  },
})

export default Input
