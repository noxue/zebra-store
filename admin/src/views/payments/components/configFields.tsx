import { defineComponent, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { cn, FormField, Input, Select, Textarea } from '@/components/ui'

const MODAL = 'admin.paymentChannels.modal'

/** Bordered sub-card wrapping one provider's config form (title + grid + hint). */
export const ProviderSection = defineComponent({
  name: 'PaymentProviderSection',
  props: {
    /** i18n suffix under admin.paymentChannels.modal for the title */
    title: { type: String, required: true },
    /** i18n suffix under admin.paymentChannels.modal for the footer hint */
    hint: String,
  },
  setup(props, { slots }) {
    const { t } = useI18n()
    return () => (
      <section class="min-w-0 overflow-hidden rounded-zs-lg border border-line bg-surface-muted p-4">
        <h4 class="zs-display mb-3 text-sm text-fg">{t(`${MODAL}.${props.title}`)}</h4>
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2 [&>*]:min-w-0">{slots.default?.()}</div>
        {slots.after?.()}
        {props.hint && <p class="mt-3 text-xs leading-relaxed text-muted">{t(`${MODAL}.${props.hint}`)}</p>}
      </section>
    )
  },
})

export interface FieldOptions {
  /** span both grid columns */
  wide?: boolean
  /** render a textarea with this many rows */
  rows?: number
  type?: string
  step?: string
  min?: string
  required?: boolean
  /** i18n suffix for a hint under the control */
  hint?: string
  /** i18n suffix for the placeholder (default `${name}Placeholder`) */
  placeholder?: string
}

export interface SelectFieldOption {
  value: string
  /** i18n suffix under admin.paymentChannels.modal, or a literal when `literal` */
  label: string
  literal?: boolean
}

/** Field renderers bound to the current i18n instance; call inside setup(). */
export function useConfigFields() {
  const { t } = useI18n()
  const m = (suffix: string) => t(`${MODAL}.${suffix}`)

  /** Text / textarea bound to `obj[key]`; `name` is the i18n suffix of the label. */
  const text = <K extends string>(obj: Record<K, string>, key: K, name: string, opts: FieldOptions = {}): VNodeChild => (
    <div class={cn('min-w-0', opts.wide && 'md:col-span-2')}>
      <FormField label={m(name)} required={opts.required} hint={opts.hint ? m(opts.hint) : undefined}>
        {opts.rows ? (
          <Textarea
            modelValue={obj[key]}
            onUpdate:modelValue={(v: string) => (obj[key] = v)}
            rows={opts.rows}
            mono
            placeholder={m(opts.placeholder ?? `${name}Placeholder`)}
          />
        ) : (
          <Input
            modelValue={obj[key]}
            onUpdate:modelValue={(v: string | number) => (obj[key] = String(v))}
            type={opts.type ?? 'text'}
            step={opts.step}
            min={opts.min}
            placeholder={m(opts.placeholder ?? `${name}Placeholder`)}
          />
        )}
      </FormField>
    </div>
  )

  const select = <K extends string>(obj: Record<K, string>, key: K, name: string, options: SelectFieldOption[], opts: FieldOptions = {}): VNodeChild => (
    <div class={cn('min-w-0', opts.wide && 'md:col-span-2')}>
      <FormField label={m(name)} hint={opts.hint ? m(opts.hint) : undefined}>
        <Select
          modelValue={obj[key]}
          onUpdate:modelValue={(v: string | number) => (obj[key] = String(v))}
          options={options.map((o) => ({ value: o.value, label: o.literal ? o.label : m(o.label) }))}
        />
      </FormField>
    </div>
  )

  return { t, m, text, select }
}
