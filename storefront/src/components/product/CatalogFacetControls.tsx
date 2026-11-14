import { defineComponent, useId, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, X } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import type { useCatalogFacets } from '@/composables/useCatalogFacets'
import { facetParam } from '@/utils/catalogFacets'
import { Button, Input, cn } from '@/components/ui'

type Facets = ReturnType<typeof useCatalogFacets>
export const CatalogFacetControls = defineComponent({
  name: 'CatalogFacetControls',
  props: {
    model: { type: Object as PropType<Facets>, required: true },
    retry: { type: Function as PropType<() => Promise<void>>, required: true },
    mobile: Boolean,
  },
  emits: { apply: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()
    const id = useId()
    return () => {
      const f = props.model
      if (!f.capabilityLoading.value && !f.capabilityError.value && !f.hasSelection.value && !f.routeError.value && f.visibleKeys.value.length === 0) return null
      return (
        <section aria-label={t('catalogBrowse.title')} class="min-w-0 space-y-4 border-t border-line pt-4">
          <div class="flex items-center justify-between gap-2">
            <h3 class="text-sm font-bold text-fg">{t('catalogBrowse.title')}</h3>
            {(f.hasSelection.value || f.routeError.value) && <Button size="sm" class="min-h-11" variant="ghost" disabled={!f.initialized.value} onClick={f.clear}>{t('catalogBrowse.clear')}</Button>}
          </div>
          <p class="text-xs leading-relaxed text-muted">{t('catalogBrowse.hint')}</p>
          {f.capabilityLoading.value && <p role="status" class="text-sm text-muted">{t('catalogBrowse.loading')}</p>}
          {f.capabilityError.value && <div role="alert" class="space-y-2 text-sm text-danger-text"><p>{t(`catalogBrowse.errors.${f.capabilityError.value}`)}</p><Button size="sm" class="min-h-11" variant="secondary" onClick={() => void props.retry()}>{t('catalogBrowse.retry')}</Button></div>}
          {(f.routeError.value || (f.initialized.value && f.hasSelection.value && !f.supported.value && !f.capabilityLoading.value && !f.capabilityError.value) || (f.supported.value && f.blocked.value)) && <p role="alert" class="text-sm text-warning-text">{t(f.routeError.value ? 'catalogBrowse.invalidLink' : 'catalogBrowse.unsupported')}</p>}
          {f.visibleKeys.value.map((key) => {
            const field = f.states[key]
            const selected = f.filters.value[facetParam(key)]
            const name = t(`catalogBrowse.dimensions.${key}`)
            const inputId = `${id}-${key}`
            const selectedLabel = field.selected ? getLocalizedText(field.selected.label) : ''
            return (
              <fieldset key={key} class="min-w-0 space-y-2" aria-busy={field.loading}>
                <legend class="text-sm font-bold text-fg">{name}</legend>
                {selected && <div class="flex min-w-0 items-center gap-1 rounded-zs border border-primary/30 bg-primary-soft p-2 text-sm text-primary-text">
                  <span class="min-w-0 flex-1 break-words">{selectedLabel || t(field.loading ? 'catalogBrowse.resolving' : field.error ? 'catalogBrowse.selectionUnknown' : 'catalogBrowse.selectionMissing')}</span>
                  <button type="button" class="flex size-11 shrink-0 items-center justify-center rounded-full focus-visible:outline-2 focus-visible:outline-primary" aria-label={t('catalogBrowse.remove', { name })} onClick={() => f.choose(key, null)}><X class="size-4" /></button>
                </div>}
                <label for={inputId} class="sr-only">{t('catalogBrowse.search', { name })}</label>
                <Input id={inputId} modelValue={field.search} maxlength={160} placeholder={t('catalogBrowse.search', { name })} onUpdate:modelValue={(value: string) => f.optionSearch(key, value)} />
                {field.error ? <div role="alert" class="space-y-2 text-sm text-danger-text"><p>{t(`catalogBrowse.errors.${field.error}`)}</p><Button size="sm" class="min-h-11" variant="secondary" onClick={() => void f.fetchOptions(key)}>{t('catalogBrowse.retry')}</Button></div>
                  : field.loading ? <p role="status" class="text-sm text-muted">{t('catalogBrowse.loading')}</p>
                    : <>
                      {field.missingSelection && <p role="status" class="text-xs text-warning-text">{t('catalogBrowse.selectionMissing')}</p>}
                      <div class="max-h-56 min-w-0 space-y-1 overflow-y-auto" role="group" aria-label={name}>
                        {field.items.map((option) => <button key={option.value_id} type="button" aria-pressed={selected === option.value_id}
                          class={cn('flex min-h-11 w-full min-w-0 items-center gap-2 rounded-zs border px-3 py-2 text-left text-sm transition focus-visible:outline-2 focus-visible:outline-primary', selected === option.value_id ? 'border-primary bg-primary-soft text-primary-text' : 'border-line bg-surface-strong text-fg')}
                          onClick={() => f.choose(key, option.value_id)}>
                          <span class="min-w-0 flex-1 break-words">{getLocalizedText(option.label)}</span>
                          <span class="shrink-0 text-xs text-muted">{t('catalogBrowse.count', { count: option.product_count })}</span>
                          {selected === option.value_id && <Check aria-hidden="true" class="size-4 shrink-0" />}
                        </button>)}
                      </div>
                      {field.items.length === 0 && <p role="status" class="text-sm text-muted">{t('catalogBrowse.noOptions')}</p>}
                      {field.page < field.totalPages && <Button size="sm" class="min-h-11" variant="secondary" block onClick={() => void f.fetchOptions(key, true)}>{t('catalogBrowse.more')}</Button>}
                    </>}
              </fieldset>
            )
          })}
          {f.supported.value && <p class="text-xs leading-relaxed text-muted">{t('catalogBrowse.countHint')}</p>}
          {props.mobile && <Button block onClick={() => emit('apply')}>{t('catalogBrowse.apply')}</Button>}
        </section>
      )
    }
  },
})
