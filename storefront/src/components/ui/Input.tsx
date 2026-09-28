import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'

const fieldBase =
  'w-full rounded-zs border bg-surface-strong text-fg placeholder:text-muted/70 transition-all outline-none ' +
  'focus:border-primary focus:shadow-[var(--zs-ring)] disabled:opacity-60 disabled:cursor-not-allowed'

/** Text input with optional prefix/suffix slots. Use with `v-model`. */
export const Input = defineComponent({
  name: 'ZsInput',
  inheritAttrs: false,
  props: {
    modelValue: { type: [String, Number] as PropType<string | number | null>, default: '' },
    type: { type: String, default: 'text' },
    placeholder: { type: String, default: '' },
    disabled: Boolean,
    readonly: Boolean,
    invalid: Boolean,
    size: { type: String as PropType<'sm' | 'md' | 'lg'>, default: 'md' },
    autocomplete: { type: String, default: undefined },
    inputmode: { type: String as PropType<'text' | 'numeric' | 'decimal' | 'email' | 'tel' | 'search' | 'url' | 'none'>, default: undefined },
    maxlength: { type: Number, default: undefined },
    min: { type: [Number, String], default: undefined },
    max: { type: [Number, String], default: undefined },
    step: { type: [Number, String], default: undefined },
    name: { type: String, default: undefined },
    id: { type: String, default: undefined },
    inputClass: { type: String, default: '' },
  },
  emits: {
    'update:modelValue': (_v: string) => true,
    enter: () => true,
    blur: (_e: FocusEvent) => true,
    focus: (_e: FocusEvent) => true,
  },
  setup(props, { emit, slots, attrs }) {
    const heights = { sm: 'h-9 text-sm', md: 'h-11 text-sm', lg: 'h-12 text-base' }
    return () => (
      <div class={cn('relative flex items-center', attrs.class as string)}>
        {slots.prefix && <span class="pointer-events-none absolute left-3.5 flex items-center text-muted">{slots.prefix()}</span>}
        <input
          id={props.id}
          name={props.name}
          type={props.type}
          value={props.modelValue ?? ''}
          placeholder={props.placeholder}
          disabled={props.disabled}
          readonly={props.readonly}
          autocomplete={props.autocomplete}
          inputmode={props.inputmode}
          maxlength={props.maxlength}
          min={props.min}
          max={props.max}
          step={props.step}
          aria-invalid={props.invalid || undefined}
          class={cn(
            fieldBase,
            heights[props.size],
            'px-4',
            slots.prefix && 'pl-10',
            slots.suffix && 'pr-11',
            props.invalid ? 'border-danger' : 'border-line',
            props.inputClass,
          )}
          onInput={(e: Event) => emit('update:modelValue', (e.target as HTMLInputElement).value)}
          onKeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter') emit('enter')
          }}
          onBlur={(e: FocusEvent) => emit('blur', e)}
          onFocus={(e: FocusEvent) => emit('focus', e)}
        />
        {slots.suffix && <span class="absolute right-2 flex items-center text-muted">{slots.suffix()}</span>}
      </div>
    )
  },
})

export const Textarea = defineComponent({
  name: 'ZsTextarea',
  props: {
    modelValue: { type: String as PropType<string | null>, default: '' },
    placeholder: { type: String, default: '' },
    rows: { type: Number, default: 4 },
    disabled: Boolean,
    invalid: Boolean,
    maxlength: { type: Number, default: undefined },
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    return () => (
      <textarea
        value={props.modelValue ?? ''}
        placeholder={props.placeholder}
        rows={props.rows}
        disabled={props.disabled}
        maxlength={props.maxlength}
        class={cn(fieldBase, 'px-4 py-3 text-sm leading-relaxed resize-y', props.invalid ? 'border-danger' : 'border-line')}
        onInput={(e: Event) => emit('update:modelValue', (e.target as HTMLTextAreaElement).value)}
      />
    )
  },
})
