import { computed, defineComponent, type PropType } from 'vue'

export interface DonutSegmentView {
  value: number
  color: string
  label: string
}

/** SVG donut with legend. Colors are CSS variables (theme aware). */
export const ResellerDonut = defineComponent({
  name: 'ResellerDonut',
  props: {
    segments: { type: Array as PropType<DonutSegmentView[]>, required: true },
    centerValue: { type: [String, Number], default: '' },
    centerLabel: { type: String, default: '' },
  },
  setup(props) {
    const RADIUS = 42
    const CIRC = 2 * Math.PI * RADIUS
    const arcs = computed(() => {
      const total = props.segments.reduce((s, x) => s + x.value, 0)
      let offset = 0
      return props.segments.map((seg) => {
        const len = total > 0 ? (seg.value / total) * CIRC : 0
        const arc = { ...seg, dash: `${len} ${CIRC - len}`, offset: -offset, pct: total > 0 ? Math.round((seg.value / total) * 100) : 0 }
        offset += len
        return arc
      })
    })
    return () => (
      <div class="flex flex-col items-center gap-5 sm:flex-row">
        <div class="relative size-40 shrink-0">
          <svg viewBox="0 0 100 100" class="size-full -rotate-90">
            <circle cx="50" cy="50" r={RADIUS} fill="none" stroke="var(--zs-surface-muted)" stroke-width="12" />
            {arcs.value.map((a, i) => (
              <circle key={i} cx="50" cy="50" r={RADIUS} fill="none" stroke={a.color} stroke-width="12" stroke-dasharray={a.dash} stroke-dashoffset={a.offset} stroke-linecap="butt" />
            ))}
          </svg>
          <div class="absolute inset-0 flex flex-col items-center justify-center">
            <span class="zs-num text-2xl font-bold text-fg">{props.centerValue}</span>
            <span class="text-[11px] text-muted">{props.centerLabel}</span>
          </div>
        </div>
        <ul class="w-full space-y-2 text-sm">
          {arcs.value.map((a, i) => (
            <li key={i} class="flex items-center justify-between gap-3">
              <span class="flex min-w-0 items-center gap-2">
                <span class="size-3 shrink-0 rounded-full" style={{ background: a.color }} />
                <span class="truncate text-fg">{a.label}</span>
              </span>
              <span class="zs-num text-muted">
                {a.value} · {a.pct}%
              </span>
            </li>
          ))}
        </ul>
      </div>
    )
  },
})
