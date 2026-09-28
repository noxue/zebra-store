import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'
import { inputBase } from './Input'

export const Textarea = defineComponent({
  name: 'ZsTextarea',
  props: {
    modelValue: { type: String as PropType<string | null | undefined>, default: '' },
    rows: { type: Number, default: 4 },
    placeholder: String,
    disabled: Boolean,
    readonly: Boolean,
    mono: Boolean,
    maxlength: [String, Number],
  },
  emits: { 'update:modelValue': (_v: string) => true, paste: (_e: ClipboardEvent) => true },
  setup(props, { emit }) {
    return () => (
      <textarea
        rows={props.rows}
        value={props.modelValue ?? ''}
        placeholder={props.placeholder}
        disabled={props.disabled}
        readonly={props.readonly}
        maxlength={props.maxlength}
        class={cn(inputBase, 'min-h-[4rem] px-3 py-2 text-sm leading-relaxed', props.mono && 'font-mono text-xs')}
        onInput={(e: Event) => emit('update:modelValue', (e.target as HTMLTextAreaElement).value)}
        onPaste={(e: ClipboardEvent) => emit('paste', e)}
      />
    )
  },
})

export default Textarea
