import { defineComponent, type PropType } from 'vue'
import type { LucideIcon } from 'lucide-vue-next'
import { Alert } from '@/components/ui'
import type { PanelAlert } from '@/composables/personal/usePanelAlert'

/** Panel heading: gradient icon tile, display title, description, actions slot. */
export const PanelHeading = defineComponent({
  name: 'PanelHeading',
  props: {
    title: { type: String, required: true },
    description: { type: String, default: '' },
    icon: { type: Function as PropType<LucideIcon>, default: undefined },
  },
  setup(props, { slots }) {
    return () => {
      const Icon = props.icon
      return (
        <div class="mb-5 flex flex-wrap items-start justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3.5">
            {Icon && (
              <span class="zs-gradient-bg flex size-11 shrink-0 items-center justify-center rounded-zs text-on-primary shadow-zs">
                <Icon class="size-5" />
              </span>
            )}
            <div class="min-w-0">
              <h2 class="zs-title flex items-center gap-2 text-xl text-fg">
                {props.title}
                <span class="zs-sparkle text-xs" aria-hidden="true">
                  ✦
                </span>
              </h2>
              {props.description && <p class="mt-0.5 text-sm text-muted">{props.description}</p>}
            </div>
          </div>
          {slots.actions && <div class="flex flex-wrap items-center gap-2">{slots.actions()}</div>}
        </div>
      )
    }
  },
})

/** Renders a PanelAlert (or nothing). */
export const PanelAlertBox = defineComponent({
  name: 'PanelAlertBox',
  props: { alert: { type: Object as PropType<PanelAlert | null>, default: null } },
  setup(props) {
    return () => (props.alert ? <div class="mb-5"><Alert tone={props.alert.tone}>{props.alert.message}</Alert></div> : null)
  },
})

/** Dashed placeholder box used for empty lists inside panels. */
export const DashedNote = defineComponent({
  name: 'DashedNote',
  setup(_props, { slots }) {
    return () => <div class="rounded-zs border border-dashed border-line-strong bg-surface-muted px-4 py-5 text-sm text-muted">{slots.default?.()}</div>
  },
})

/** Stack of shimmering rows for loading states. */
export const SkeletonRows = defineComponent({
  name: 'SkeletonRows',
  props: { count: { type: Number, default: 3 }, height: { type: String, default: 'h-16' } },
  setup(props) {
    return () => (
      <div class="space-y-3">
        {Array.from({ length: props.count }).map((_, i) => (
          <div key={i} class={['zs-skeleton rounded-zs', props.height]} />
        ))}
      </div>
    )
  },
})

/** Thin gradient progress bar. */
export const ProgressBar = defineComponent({
  name: 'ProgressBar',
  props: { percent: { type: Number, required: true } },
  setup(props) {
    return () => (
      <div class="relative h-2 w-full overflow-hidden rounded-full bg-surface-muted">
        <div class="zs-gradient-bg absolute inset-y-0 left-0 rounded-full transition-all duration-700 ease-out" style={{ width: `${Math.max(0, Math.min(100, props.percent))}%` }} />
      </div>
    )
  },
})
