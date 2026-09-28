import { defineComponent, type PropType } from 'vue'
import { AlertTriangle, CheckCircle2, Info, XCircle } from 'lucide-vue-next'
import { cn } from './cn'

/** Shimmer placeholder block. */
export const Skeleton = defineComponent({
  name: 'ZsSkeleton',
  setup() {
    return () => <div class="zs-skeleton" />
  },
})

/** Bouncing sakura-petal loader. */
export const PetalLoader = defineComponent({
  name: 'ZsPetalLoader',
  props: { label: { type: String, default: '' }, size: { type: String as PropType<'sm' | 'md'>, default: 'md' } },
  setup(props) {
    const colors = ['var(--zs-primary)', 'var(--zs-secondary)', 'var(--zs-accent)']
    return () => (
      <div class="flex flex-col items-center justify-center gap-3 py-2" role="status">
        <div class="flex items-end gap-1.5">
          {colors.map((c, i) => (
            <svg
              key={i}
              viewBox="0 0 20 20"
              class={props.size === 'sm' ? 'size-3' : 'size-4'}
              style={{ animation: `zs-bounce-dot 1.2s ${i * 0.15}s infinite ease-in-out`, color: c }}
            >
              <path fill="currentColor" d="M10 2c2.5 2.6 5 5 5 8.2A5 5 0 0 1 10 15a5 5 0 0 1-5-4.8C5 7 7.5 4.6 10 2z" />
            </svg>
          ))}
        </div>
        {props.label && <span class="text-sm text-muted">{props.label}</span>}
      </div>
    )
  },
})

export type AlertTone = 'info' | 'success' | 'warning' | 'error'

/** Inline notice box. */
export const Alert = defineComponent({
  name: 'ZsAlert',
  props: {
    tone: { type: String as PropType<AlertTone>, default: 'info' },
    title: { type: String, default: '' },
  },
  setup(props, { slots }) {
    const map = {
      info: { cls: 'bg-accent-soft border-accent/35 text-accent-text', icon: Info },
      success: { cls: 'bg-success-soft border-success/35 text-success-text', icon: CheckCircle2 },
      warning: { cls: 'bg-warning-soft border-warning/35 text-warning-text', icon: AlertTriangle },
      error: { cls: 'bg-danger-soft border-danger/35 text-danger-text', icon: XCircle },
    }
    return () => {
      const { cls, icon: Icon } = map[props.tone]
      return (
        <div role="alert" class={cn('flex gap-3 rounded-zs border px-4 py-3 text-sm', cls)}>
          <Icon class="mt-0.5 size-4 shrink-0" />
          <div class="min-w-0 flex-1 leading-relaxed">
            {props.title && <div class="mb-0.5 font-bold">{props.title}</div>}
            {slots.default?.()}
          </div>
        </div>
      )
    }
  },
})

/** Form field wrapper: label, required mark, hint and error. */
export const Field = defineComponent({
  name: 'ZsField',
  props: {
    label: { type: String, default: '' },
    required: Boolean,
    error: { type: String, default: '' },
    hint: { type: String, default: '' },
    for: { type: String, default: undefined },
  },
  setup(props, { slots }) {
    return () => (
      <div class="space-y-1.5">
        {(props.label || slots.label) && (
          <label for={props.for} class="flex items-center justify-between gap-2 text-sm font-bold text-fg">
            <span>
              {props.label}
              {props.required && <span class="ml-0.5 text-danger">*</span>}
            </span>
            {slots.label?.()}
          </label>
        )}
        {slots.default?.()}
        {props.error ? <p class="text-xs text-danger-text">{props.error}</p> : props.hint ? <p class="text-xs text-muted">{props.hint}</p> : null}
      </div>
    )
  },
})
