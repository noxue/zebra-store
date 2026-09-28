import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'

/** RouterLink styled like an outline `<Button size="sm">` (avoids nesting a button inside a link). */
export const LinkButton = defineComponent({
  name: 'UsersLinkButton',
  props: { to: { type: String, required: true } },
  setup(props, { slots }) {
    return () => (
      <RouterLink
        to={props.to}
        class="inline-flex h-8 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-full border border-line-strong bg-surface-strong px-3 text-xs font-medium text-fg transition-all duration-200 hover:border-primary hover:bg-primary-soft hover:text-primary"
      >
        {slots.default?.()}
      </RouterLink>
    )
  },
})

export default LinkButton
