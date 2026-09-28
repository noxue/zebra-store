import { defineComponent } from 'vue'

/** Page title row: gradient display title + sparkle, subtitle, and right-aligned actions. */
export const PageHeader = defineComponent({
  name: 'ZsPageHeader',
  props: { title: { type: String, required: true }, subtitle: String },
  setup(props, { slots }) {
    return () => (
      <div class="flex flex-wrap items-end justify-between gap-4">
        <div class="min-w-0">
          <h1 class="zs-display flex items-center gap-2 text-2xl text-fg sm:text-[26px]">
            <span class="zs-gradient-text">{props.title}</span>
            <span class="zs-sparkle text-base">✦</span>
          </h1>
          {props.subtitle && <p class="mt-1 text-sm text-muted">{props.subtitle}</p>}
          {slots.meta?.()}
        </div>
        {slots.actions && <div class="flex flex-wrap items-center gap-2">{slots.actions()}</div>}
      </div>
    )
  },
})

export default PageHeader
