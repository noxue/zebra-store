import { defineComponent, type PropType } from 'vue'
import { barPercent, shortDate } from './dashboardUtils'

export interface TrendSeries {
  label: string
  /** CSS color (use a token var) */
  color: string
  values: number[]
}

/** Hand-rolled grouped bar chart (SVG-free: flex bars) with gridlines and hover values. */
export const TrendBars = defineComponent({
  name: 'TrendBars',
  props: {
    dates: { type: Array as PropType<string[]>, required: true },
    series: { type: Array as PropType<TrendSeries[]>, required: true },
    max: { type: Number, required: true },
    footer: { type: Array as PropType<string[]>, default: () => [] },
    emptyText: { type: String, default: '' },
  },
  setup(props) {
    return () => {
      if (props.dates.length === 0) return <p class="py-10 text-center text-sm text-muted">{props.emptyText}</p>
      return (
        <div class="space-y-3">
          <div class="flex flex-wrap gap-3 text-xs text-muted">
            {props.series.map((s) => (
              <span key={s.label} class="inline-flex items-center gap-1.5">
                <span class="h-2.5 w-2.5 rounded-full" style={{ background: s.color }} />
                {s.label}
              </span>
            ))}
          </div>
          <div class="relative h-48 rounded-zs bg-surface-muted/60 px-3 pt-3">
            {[0, 1, 2, 3].map((i) => (
              <div key={i} class="absolute inset-x-3 border-t border-dashed border-line" style={{ top: `${12 + i * 25}%` }} />
            ))}
            <div class="relative flex h-full items-end gap-2">
              {props.dates.map((d, di) => (
                <div key={d} class="group flex h-full min-w-0 flex-1 items-end justify-center gap-1">
                  {props.series.map((s) => {
                    const v = s.values[di] ?? 0
                    return (
                      <div key={s.label} class="relative flex h-full w-full max-w-[18px] items-end" title={`${shortDate(d)} · ${s.label}: ${v}`}>
                        <div
                          class="w-full rounded-t-full transition-all duration-500 group-hover:brightness-110"
                          style={{ height: `${barPercent(v, props.max)}%`, background: s.color, opacity: v === 0 ? 0.35 : 1 }}
                        />
                        <span class="zs-num pointer-events-none absolute -top-5 left-1/2 hidden -translate-x-1/2 rounded-full bg-surface-solid px-1.5 text-[10px] text-fg shadow-zs-sm group-hover:block">
                          {v}
                        </span>
                      </div>
                    )
                  })}
                </div>
              ))}
            </div>
          </div>
          <div class="flex gap-2 px-3">
            {props.dates.map((d, di) => (
              <div key={d} class="min-w-0 flex-1 text-center">
                <p class="zs-num text-[11px] text-muted">{shortDate(d)}</p>
                {props.footer[di] !== undefined && <p class="zs-num truncate text-[10px] text-success-text">{props.footer[di]}</p>}
              </div>
            ))}
          </div>
        </div>
      )
    }
  },
})

export default TrendBars
