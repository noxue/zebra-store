import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ClipboardPen } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import { Card, CardHeader, Checkbox, Field, Input, Select, Textarea, cn } from '@/components/ui'
import type { ManualFieldValue, ManualFormProduct, ManualFormValues, NormalizedManualField } from '@/utils/manualForm'

/** Per-product manual delivery form (`manual_form_schema.fields`). */
export const CheckoutManualForm = defineComponent({
  name: 'CheckoutManualForm',
  props: {
    products: { type: Array as PropType<ManualFormProduct[]>, required: true },
    modelValue: { type: Object as PropType<ManualFormValues>, required: true },
    submitAttempted: Boolean,
    fieldError: { type: Function as PropType<(itemKey: string, fieldKey: string) => string>, required: true },
  },
  emits: { 'update:modelValue': (_v: ManualFormValues) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()

    const getValue = (itemKey: string, key: string): ManualFieldValue => props.modelValue[itemKey]?.[key] ?? ''
    const setValue = (itemKey: string, key: string, value: ManualFieldValue) => {
      emit('update:modelValue', { ...props.modelValue, [itemKey]: { ...(props.modelValue[itemKey] || {}), [key]: value } })
    }
    const toggle = (itemKey: string, key: string, option: string, checked: boolean) => {
      const current = getValue(itemKey, key)
      const list = Array.isArray(current) ? current.filter((v) => v !== option) : []
      setValue(itemKey, key, checked ? [...list, option] : list)
    }
    const productTitle = (p: ManualFormProduct) => {
      const title = getLocalizedText(p.title)
      return p.skuCount <= 1 ? title : `${title} (${t('checkout.manualFormAppliesToSkuCount', { count: p.skuCount })})`
    }
    const renderField = (p: ManualFormProduct, field: NormalizedManualField) => {
      const value = getValue(p.itemKey, field.key)
      const text = Array.isArray(value) ? '' : value
      const placeholder = getLocalizedText(field.placeholder)
      const err = props.submitAttempted ? props.fieldError(p.itemKey, field.key) : ''
      let control
      if (field.type === 'textarea') {
        control = <Textarea modelValue={text} rows={3} placeholder={placeholder} invalid={!!err} onUpdate:modelValue={(v: string) => setValue(p.itemKey, field.key, v)} />
      } else if (field.type === 'select') {
        control = (
          <Select
            modelValue={text}
            placeholder={t('checkout.manualFormSelectPlaceholder')}
            options={field.options.map((o) => ({ label: o, value: o }))}
            invalid={!!err}
            onUpdate:modelValue={(v: string | number) => setValue(p.itemKey, field.key, String(v))}
          />
        )
      } else if (field.type === 'radio' || field.type === 'checkbox') {
        control = (
          <div class="flex flex-wrap gap-2 rounded-zs border border-line bg-surface-strong p-3">
            {field.options.map((option) => {
              const checked = field.type === 'radio' ? text === option : Array.isArray(value) && value.includes(option)
              return field.type === 'radio' ? (
                <button
                  key={option}
                  type="button"
                  class={cn(
                    'rounded-full border px-3.5 py-1.5 text-sm font-bold transition',
                    checked ? 'zs-gradient-bg border-transparent text-on-primary shadow-zs' : 'border-line bg-surface text-muted hover:border-line-strong',
                  )}
                  onClick={() => setValue(p.itemKey, field.key, option)}
                >
                  {option}
                </button>
              ) : (
                <Checkbox key={option} modelValue={checked} onUpdate:modelValue={(v: boolean) => toggle(p.itemKey, field.key, option, v)}>
                  {option}
                </Checkbox>
              )
            })}
          </div>
        )
      } else {
        const type = field.type === 'number' ? 'number' : field.type === 'email' ? 'email' : field.type === 'phone' ? 'tel' : 'text'
        control = <Input type={type} modelValue={text} placeholder={placeholder} invalid={!!err} onUpdate:modelValue={(v: string) => setValue(p.itemKey, field.key, v)} />
      }
      return (
        <Field key={`${p.itemKey}-${field.key}`} label={getLocalizedText(field.label) || field.key} required={field.required} error={err}>
          {control}
        </Field>
      )
    }
    return () =>
      props.products.length === 0 ? null : (
        <Card>
          <CardHeader title={t('checkout.manualFormTitle')} description={t('checkout.manualFormTip')}>
            {{ icon: () => <ClipboardPen class="size-5 text-primary" /> }}
          </CardHeader>
          <div class="space-y-4">
            {props.products.map((p) => (
              <div key={p.itemKey} class="rounded-zs border border-line bg-surface-muted p-4">
                <h3 class="mb-3 text-sm font-bold text-fg">{productTitle(p)}</h3>
                <div class="grid grid-cols-1 gap-4 md:grid-cols-2">{p.fields.map((field) => renderField(p, field))}</div>
              </div>
            ))}
          </div>
        </Card>
      )
  },
})
