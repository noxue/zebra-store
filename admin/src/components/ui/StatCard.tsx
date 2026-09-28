import { defineComponent, type PropType } from 'vue'
import { cn, type IconComponent } from './cn'

export type StatTone = 'primary' | 'secondary' | 'accent' | 'gold' | 'success' | 'danger'

const iconBg: Record<StatTone, string> = {
  primary: 'bg-gradient-to-br from-primary to-secondary',
  secondary: 'bg-gradient-to-br from-secondary to-accent',
  accent: 'bg-gradient-to-br from-accent to-secondary',
  gold: 'bg-gradient-to-br from-gold to-primary',
  success: 'bg-gradient-to-br from-success to-accent',
  danger: 'bg-gradient-to-br from-danger to-primary',
}

/** KPI card with a gradient icon badge (DESIGN.md 管理后台差异). */
export const StatCard = defineComponent({
  name: 'ZsStatCard',
  props: {
    label: { type: String, required: true },
    value: { type: [String, Number], default: '' },
    icon: { type: [Object, Function] as PropType<IconComponent>, default: undefined },
    tone: { type: String as PropType<StatTone>, default: 'primary' },
    active: Boolean,
    clickable: Boolean,
  },
  emits: { click: () => true },
  setup(props, { slots, emit }) {
    return () => {
      const Icon = props.icon
      return (
        <div
          class={cn(
            'zs-glass zs-card-hover rounded-zs-lg p-5 shadow-zs-sm',
            props.clickable && 'cursor-pointer',
            props.active && 'ring-2 ring-primary',
          )}
          onClick={() => props.clickable && emit('click')}
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <p class="text-xs font-medium text-muted">{props.label}</p>
              <p class="zs-num mt-2 truncate text-2xl font-bold text-fg">{slots.value ? slots.value() : props.value}</p>
            </div>
            {Icon && (
              <span class={cn('flex h-10 w-10 shrink-0 items-center justify-center rounded-[14px] text-white shadow-zs-sm', iconBg[props.tone])}>
                <Icon class="h-5 w-5" />
              </span>
            )}
          </div>
          {slots.default && <div class="mt-2 space-y-0.5 text-xs text-muted">{slots.default()}</div>}
        </div>
      )
    }
  },
})

export default StatCard
