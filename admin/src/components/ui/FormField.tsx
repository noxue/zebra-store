import { defineComponent } from 'vue'
import { cn } from './cn'

/** Label + control + hint/error wrapper used by every form. */
export const FormField = defineComponent({
  name: 'ZsFormField',
  props: {
    label: String,
    required: Boolean,
    hint: String,
    error: String,
    inline: Boolean,
    for: String,
  },
  setup(props, { slots }) {
    return () => (
      <div class={cn('min-w-0', props.inline ? 'flex items-center justify-between gap-4' : 'space-y-1.5')}>
        {(props.label || slots.label) && (
          <label for={props.for} class="flex items-center gap-1 text-xs font-medium text-fg/85">
            {slots.label ? slots.label() : props.label}
            {props.required && <span class="text-primary">*</span>}
            {slots.labelExtra?.()}
          </label>
        )}
        <div class={cn(props.inline && 'shrink-0')}>{slots.default?.()}</div>
        {props.error ? (
          <p class="text-xs text-danger-text">{props.error}</p>
        ) : props.hint || slots.hint ? (
          <p class="text-xs leading-relaxed text-muted">{slots.hint ? slots.hint() : props.hint}</p>
        ) : null}
      </div>
    )
  },
})

export default FormField
