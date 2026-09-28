import { defineComponent } from 'vue'
import { SectionTitle } from '@/components/ui'

/** Centered page header used by content pages. */
export const PageHero = defineComponent({
  name: 'PageHero',
  props: {
    title: { type: String, required: true },
    subtitle: { type: String, default: '' },
  },
  setup(props, { slots }) {
    return () => (
      <header class="mx-auto flex max-w-2xl flex-col items-center py-8 text-center sm:py-12 [&_h1]:justify-center">
        <SectionTitle as="h1" size="xl" title={props.title} />
        {props.subtitle && <p class="mt-3 text-base text-muted">{props.subtitle}</p>}
        <div class="zs-divider mt-8 w-full" />
        {slots.default?.()}
      </header>
    )
  },
})
