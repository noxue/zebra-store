import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'

/** Glass card. `padding`: none | sm | md | lg. */
export const Card = defineComponent({
  name: 'ZsCard',
  props: {
    padding: { type: String as PropType<'none' | 'sm' | 'md' | 'lg'>, default: 'md' },
    hover: Boolean,
    as: { type: String, default: 'div' },
  },
  setup(props, { slots }) {
    const pad = { none: '', sm: 'p-4', md: 'p-5 sm:p-6', lg: 'p-6 sm:p-8' }
    return () => {
      const Tag = props.as as 'div'
      return <Tag class={cn('zs-card relative', pad[props.padding], props.hover && 'zs-card-hover')}>{slots.default?.()}</Tag>
    }
  },
})

/** Card heading row: title (+ sparkle), optional description and actions slot. */
export const CardHeader = defineComponent({
  name: 'ZsCardHeader',
  props: {
    title: { type: String, default: '' },
    description: { type: String, default: '' },
    sparkle: Boolean,
  },
  setup(props, { slots }) {
    return () => (
      <div class="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div class="min-w-0">
          <h3 class="zs-title flex items-center gap-2 text-lg text-fg">
            {slots.icon?.()}
            {props.title}
            {props.sparkle && <span class="zs-sparkle text-sm">✦</span>}
          </h3>
          {props.description && <p class="mt-1 text-sm text-muted">{props.description}</p>}
        </div>
        {slots.actions && <div class="flex shrink-0 items-center gap-2">{slots.actions()}</div>}
      </div>
    )
  },
})
