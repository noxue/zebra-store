import { defineComponent } from 'vue'
import { cn } from './cn'

export const Card = defineComponent({
  name: 'ZsCard',
  props: {
    title: String,
    description: String,
    padded: { type: Boolean, default: true },
    hover: Boolean,
    bodyClass: String,
  },
  setup(props, { slots }) {
    return () => {
      const hasHeader = props.title || slots.title || slots.extra || props.description
      return (
        <section class={cn('zs-glass rounded-zs-lg shadow-zs', props.hover && 'zs-card-hover')}>
          {hasHeader && (
            <header class="flex flex-wrap items-start justify-between gap-3 px-5 pt-5">
              <div class="min-w-0">
                {(props.title || slots.title) && (
                  <h3 class="zs-display flex items-center gap-2 text-base text-fg">
                    {slots.title ? slots.title() : props.title}
                  </h3>
                )}
                {props.description && <p class="mt-1 text-xs text-muted">{props.description}</p>}
              </div>
              {slots.extra && <div class="flex flex-wrap items-center gap-2">{slots.extra()}</div>}
            </header>
          )}
          <div class={cn(props.padded ? 'p-5' : '', hasHeader && props.padded && 'pt-4', props.bodyClass)}>
            {slots.default?.()}
          </div>
          {slots.footer && <footer class="border-t border-line px-5 py-3">{slots.footer()}</footer>}
        </section>
      )
    }
  },
})

export default Card
