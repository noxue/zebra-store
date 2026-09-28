import { defineComponent, type PropType } from 'vue'
import type { LucideIcon } from 'lucide-vue-next'
import { cn } from './cn'

export interface TabItem {
  key: string
  label: string
  icon?: LucideIcon
  count?: number
}

/** Pill tabs; horizontally scrollable on small screens. */
export const Tabs = defineComponent({
  name: 'ZsTabs',
  props: {
    modelValue: { type: String, default: '' },
    items: { type: Array as PropType<TabItem[]>, default: () => [] },
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    return () => (
      <div class="zs-scroll-x">
        <div role="tablist" class="inline-flex min-w-max gap-1 rounded-full border border-line bg-surface p-1">
          {props.items.map((item) => {
            const active = item.key === props.modelValue
            const Icon = item.icon
            return (
              <button
                key={item.key}
                type="button"
                role="tab"
                aria-selected={active}
                class={cn(
                  'inline-flex items-center gap-1.5 rounded-full font-bold transition-all',
                  props.size === 'sm' ? 'h-8 px-3 text-xs' : 'h-9 px-4 text-sm',
                  active ? 'zs-gradient-bg text-on-primary shadow-zs' : 'text-muted hover:bg-primary-soft hover:text-primary-text',
                )}
                onClick={() => emit('update:modelValue', item.key)}
              >
                {Icon && <Icon class="size-4" />}
                {item.label}
                {item.count !== undefined && (
                  <span class={cn('rounded-full px-1.5 text-[10px] zs-num', active ? 'bg-white/25' : 'bg-surface-muted')}>{item.count}</span>
                )}
              </button>
            )
          })}
        </div>
      </div>
    )
  },
})
