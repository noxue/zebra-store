import { defineComponent, type PropType } from 'vue'
import type { BadgeTone } from '@/utils/status'
import { cn } from './cn'

const tones: Record<BadgeTone, string> = {
  primary: 'bg-primary-soft text-primary-text border-primary/30',
  success: 'bg-success-soft text-success-text border-success/35',
  warning: 'bg-warning-soft text-warning-text border-warning/35',
  info: 'bg-accent-soft text-accent-text border-accent/35',
  accent: 'bg-secondary-soft text-secondary-text border-secondary/35',
  danger: 'bg-danger-soft text-danger-text border-danger/35',
  neutral: 'bg-surface-muted text-muted border-line',
}

/** Pill badge with light background and matching border. */
export const Badge = defineComponent({
  name: 'ZsBadge',
  props: {
    tone: { type: String as PropType<BadgeTone>, default: 'neutral' },
    size: { type: String as PropType<'xs' | 'sm' | 'md'>, default: 'sm' },
  },
  setup(props, { slots }) {
    const sizes = { xs: 'h-5 px-2 text-[10px] gap-1', sm: 'h-6 px-2.5 text-xs gap-1', md: 'h-7 px-3 text-sm gap-1.5' }
    return () => (
      <span class={cn('inline-flex shrink-0 items-center whitespace-nowrap rounded-full border font-bold', tones[props.tone], sizes[props.size])}>
        {slots.default?.()}
      </span>
    )
  },
})
