import { defineComponent, type PropType } from 'vue'
import { cn, type IconComponent } from './cn'

export interface TabItem {
  key: string
  label: string
  icon?: IconComponent
  badge?: string | number
}

export const Tabs = defineComponent({
  name: 'ZsTabs',
  props: {
    modelValue: { type: String, default: '' },
    items: { type: Array as PropType<TabItem[]>, default: () => [] },
    variant: { type: String as PropType<'pills' | 'underline'>, default: 'pills' },
  },
  emits: { 'update:modelValue': (_v: string) => true, change: (_v: string) => true },
  setup(props, { emit }) {
    const pick = (key: string) => {
      emit('update:modelValue', key)
      emit('change', key)
    }
    return () => (
      <div
        role="tablist"
        class={cn(
          'flex max-w-full gap-1 overflow-x-auto',
          props.variant === 'pills' ? 'zs-glass w-fit flex-wrap rounded-zs p-1' : 'border-b border-line',
        )}
      >
        {props.items.map((item) => {
          const active = item.key === props.modelValue
          const Icon = item.icon
          return (
            <button
              key={item.key}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => pick(item.key)}
              class={cn(
                'inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap text-sm font-medium transition-all',
                props.variant === 'pills'
                  ? cn('h-8 rounded-[12px] px-3.5', active ? 'zs-gradient-bg text-on-primary zs-text-shadow shadow-zs-sm' : 'text-muted hover:bg-primary-soft hover:text-primary')
                  : cn('-mb-px h-10 border-b-2 px-3', active ? 'border-primary text-primary' : 'border-transparent text-muted hover:text-fg'),
              )}
            >
              {Icon && <Icon class="h-4 w-4" />}
              {item.label}
              {item.badge !== undefined && item.badge !== '' && (
                <span class={cn('rounded-full px-1.5 text-[10px]', active ? 'bg-white/25' : 'bg-primary-soft text-primary')}>{item.badge}</span>
              )}
            </button>
          )
        })}
      </div>
    )
  },
})

export default Tabs
