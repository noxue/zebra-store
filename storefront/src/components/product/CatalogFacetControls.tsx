import { defineComponent, reactive, useId, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, X } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import type { useCatalogFacets } from '@/composables/useCatalogFacets'
import { facetParam } from '@/utils/catalogFacets'
import { Button, Input, cn } from '@/components/ui'

/** Keep long project/region lists compact until the customer explicitly expands them. */
const COLLAPSED_OPTIONS = 6

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
    const expanded = reactive<Record<string, boolean>>({})
    return () => {
      const f = props.model
      if (!f.capabilityLoading.value && !f.capabilityError.value && !f.hasSelection.value && !f.routeError.value && f.visibleKeys.value.length === 0) return null
      return (
        <section aria-label={t('catalogBrowse.title')} class="zs-card min-w-0 space-y-3 p-4 sm:p-5">
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
            const visibleOptions = expanded[key] || field.search ? field.items : field.items.slice(0, COLLAPSED_OPTIONS)
            const selectedVisible = visibleOptions.some((option) => option.value_id === selected)
            const hasHiddenOptions = visibleOptions.length < field.items.length
            const searchable = Boolean(field.total > COLLAPSED_OPTIONS || field.totalPages > 1 || field.search || field.error || field.loading || field.missingSelection)
            const selectedLabel = field.selected ? getLocalizedText(field.selected.label) : ''
            return (
              <fieldset key={key} class="min-w-0 border-t border-line pt-3" aria-busy={field.loading}>
                <legend class="sr-only">{name}</legend>
                <div class="flex min-w-0 gap-2 sm:gap-4">
                <p aria-hidden="true" class="w-18 shrink-0 pt-3 text-sm font-bold text-fg sm:w-24">{name}</p>
                <div class="min-w-0 flex-1 space-y-2">
                {((selected && !selectedVisible) || searchable) && <div class="flex min-w-0 flex-wrap items-center gap-2">
                {selected && !selectedVisible && <div class="inline-flex max-w-full min-w-0 items-center gap-1 rounded-zs border border-primary/30 bg-primary-soft pl-3 text-sm text-primary-text">
                  <span class="min-w-0 flex-1 break-words">{selectedLabel || t(field.loading ? 'catalogBrowse.resolving' : field.error ? 'catalogBrowse.selectionUnknown' : 'catalogBrowse.selectionMissing')}</span>
                  <button type="button" class="flex size-11 shrink-0 items-center justify-center rounded-full focus-visible:outline-2 focus-visible:outline-primary" aria-label={t('catalogBrowse.remove', { name })} onClick={() => f.choose(key, null)}><X class="size-4" /></button>
                </div>}
                {searchable && <div class="min-w-0 flex-1 basis-40 sm:max-w-xs"><label for={inputId} class="sr-only">{t('catalogBrowse.search', { name })}</label>
                <Input id={inputId} modelValue={field.search} maxlength={160} placeholder={t('catalogBrowse.search', { name })} onUpdate:modelValue={(value: string) => f.optionSearch(key, value)} /></div>}
                </div>}
                {field.error ? <div role="alert" class="space-y-2 text-sm text-danger-text"><p>{t(`catalogBrowse.errors.${field.error}`)}</p><Button size="sm" class="min-h-11" variant="secondary" onClick={() => void f.fetchOptions(key)}>{t('catalogBrowse.retry')}</Button></div>
                  : field.loading ? <p role="status" class="text-sm text-muted">{t('catalogBrowse.loading')}</p>
                    : <>
                      {field.missingSelection && <p role="status" class="text-xs text-warning-text">{t('catalogBrowse.selectionMissing')}</p>}
                      <div id={`${inputId}-options`} class="flex min-w-0 flex-wrap gap-2" role="group" aria-label={name}>
                        {visibleOptions.map((option) => <button key={option.value_id} type="button" aria-pressed={selected === option.value_id}
                          class={cn('inline-flex min-h-11 max-w-full min-w-0 items-center gap-2 rounded-zs border px-2 py-2 text-left text-sm transition sm:px-3 focus-visible:outline-2 focus-visible:outline-primary', selected === option.value_id ? 'border-primary bg-primary-soft text-primary-text' : 'border-line bg-surface-strong text-fg')}
                          onClick={() => f.choose(key, option.value_id)}>
                          <span class="min-w-0 flex-1 break-words">{getLocalizedText(option.label)}</span>
                          <span class="hidden shrink-0 text-xs text-muted sm:inline">{t('catalogBrowse.count', { count: option.product_count })}</span>
                          {selected === option.value_id && <Check aria-hidden="true" class="size-4 shrink-0" />}
                        </button>)}
                        {selected && selectedVisible && <button type="button" class="flex size-11 shrink-0 items-center justify-center rounded-full text-muted hover:bg-primary-soft focus-visible:outline-2 focus-visible:outline-primary" aria-label={t('catalogBrowse.remove', { name })} onClick={() => f.choose(key, null)}><X class="size-4" /></button>}
                      </div>
                      {field.items.length === 0 && <p role="status" class="text-sm text-muted">{t('catalogBrowse.noOptions')}</p>}
                      <div class="flex flex-wrap gap-2">
                        {hasHiddenOptions && <Button size="sm" class="min-h-11" variant="ghost" aria-expanded="false" aria-controls={`${inputId}-options`} onClick={() => { expanded[key] = true }}>{t('catalogBrowse.expand')}</Button>}
                        {!hasHiddenOptions && field.page < field.totalPages && <Button size="sm" class="min-h-11" variant="secondary" onClick={() => { expanded[key] = true; void f.fetchOptions(key, true) }}>{t('catalogBrowse.more')}</Button>}
                        {expanded[key] && !field.search && field.items.length > COLLAPSED_OPTIONS && <Button size="sm" class="min-h-11" variant="ghost" aria-expanded="true" aria-controls={`${inputId}-options`} onClick={() => { expanded[key] = false }}>{t('catalogBrowse.collapse')}</Button>}
                      </div>
                    </>}
                </div>
                </div>
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
