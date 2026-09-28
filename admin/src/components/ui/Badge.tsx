import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'

export type BadgeTone = 'primary' | 'secondary' | 'info' | 'success' | 'warning' | 'danger' | 'gold' | 'neutral'

export const badgeTones: Record<BadgeTone, string> = {
  primary: 'bg-primary-soft text-primary border-primary/30',
  secondary: 'bg-secondary-soft text-secondary border-secondary/30',
  info: 'bg-accent-soft text-info-text border-accent/40',
  success: 'bg-success-soft text-success-text border-success/40',
  warning: 'bg-warning-soft text-warning-text border-warning/40',
  danger: 'bg-danger-soft text-danger-text border-danger/35',
  gold: 'bg-gold-soft text-warning-text border-gold/50',
  neutral: 'bg-surface-muted text-muted border-line',
}

export const Badge = defineComponent({
  name: 'ZsBadge',
  props: {
    tone: { type: String as PropType<BadgeTone>, default: 'neutral' },
    dot: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'sm' },
  },
  setup(props, { slots }) {
    return () => (
      <span
        class={cn(
          'inline-flex max-w-full items-center gap-1 whitespace-nowrap rounded-full border font-medium',
          props.size === 'sm' ? 'px-2 py-0.5 text-[11px] leading-4' : 'px-2.5 py-1 text-xs',
          badgeTones[props.tone],
        )}
      >
        {props.dot && <span class="h-1.5 w-1.5 rounded-full bg-current" />}
        {slots.default?.()}
      </span>
    )
  },
})

export default Badge
