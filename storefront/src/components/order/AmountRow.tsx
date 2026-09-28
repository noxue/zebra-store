import { defineComponent, type PropType } from 'vue'
import { cn } from '@/components/ui'

/** label ··· value row for money summaries. */
export const AmountRow = defineComponent({
  name: 'AmountRow',
  props: {
    label: { type: String, required: true },
    value: { type: String, required: true },
    tone: { type: String as PropType<'default' | 'discount' | 'wholesale' | 'member' | 'strong' | 'warning'>, default: 'default' },
  },
  setup(props) {
    const tones = {
      default: 'text-fg',
      discount: 'text-danger-text',
      wholesale: 'text-success-text',
      member: 'text-warning-text',
      warning: 'text-warning-text',
      strong: 'text-lg font-bold text-primary-text',
    }
    return () => (
      <div class={cn('flex items-center justify-between gap-4', props.tone === 'strong' && 'border-t border-dashed border-line pt-3')}>
        <span class={props.tone === 'strong' ? 'font-bold text-fg' : 'text-muted'}>{props.label}</span>
        <span class={cn('zs-num font-bold', tones[props.tone])}>{props.value}</span>
      </div>
    )
  },
})

/** Small info tile (label over value) used by order/payment cards. */
export const InfoTile = defineComponent({
  name: 'InfoTile',
  props: {
    label: { type: String, required: true },
    value: { type: String, default: '' },
    tone: { type: String as PropType<'default' | 'discount' | 'wholesale' | 'member'>, default: 'default' },
    mono: { type: Boolean, default: true },
  },
  setup(props, { slots }) {
    const tones = {
      default: 'border-line bg-surface-strong text-fg',
      discount: 'border-danger/30 bg-danger-soft text-danger-text',
      wholesale: 'border-success/30 bg-success-soft text-success-text',
      member: 'border-warning/30 bg-warning-soft text-warning-text',
    }
    return () => (
      <div class={cn('rounded-zs border p-3.5', tones[props.tone])}>
        <div class={cn('text-xs', props.tone === 'default' ? 'text-muted' : '')}>{props.label}</div>
        <div class={cn('mt-1 break-all text-sm font-bold', props.mono && 'zs-num')}>{slots.default ? slots.default() : props.value}</div>
      </div>
    )
  },
})
