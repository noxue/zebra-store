import { computed, defineComponent, ref, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, Search, X } from 'lucide-vue-next'
import { cn } from './cn'
import { Popover } from './Popover'
import type { SelectOption, SelectValue } from './Select'

/** Chip multi-select with search. Values keep their type. */
export const MultiSelect = defineComponent({
  name: 'ZsMultiSelect',
  props: {
    modelValue: { type: Array as PropType<SelectValue[]>, default: () => [] },
    options: { type: Array as PropType<SelectOption[]>, default: () => [] },
    placeholder: String,
    searchable: { type: Boolean, default: true },
    disabled: Boolean,
  },
  emits: { 'update:modelValue': (_v: SelectValue[]) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const open = ref(false)
    const keyword = ref('')
    const selectedSet = computed(() => new Set(props.modelValue.map(String)))
    const filtered = computed(() => {
      const k = keyword.value.trim().toLowerCase()
      return k ? props.options.filter((o) => o.label.toLowerCase().includes(k)) : props.options
    })
    const toggle = (opt: SelectOption) => {
      const key = String(opt.value)
      const next = selectedSet.value.has(key)
        ? props.modelValue.filter((v) => String(v) !== key)
        : [...props.modelValue, opt.value]
      emit('update:modelValue', next)
    }
    return () => (
      <Popover open={open.value} onUpdate:open={(v: boolean) => (open.value = v)} panelClass="w-full">
        {{
          trigger: () => (
            <button
              type="button"
              disabled={props.disabled}
              onClick={() => (open.value = !open.value)}
              class="flex min-h-9 w-full flex-wrap items-center gap-1 rounded-zs-sm border border-line-strong bg-surface-strong px-2 py-1 pr-8 text-left text-sm transition-colors focus:border-primary disabled:opacity-60"
            >
              {props.modelValue.length === 0 && <span class="px-1 text-muted/80">{props.placeholder ?? t('admin.common.select')}</span>}
              {props.modelValue.map((v) => {
                const opt = props.options.find((o) => String(o.value) === String(v))
                return (
                  <span key={String(v)} class="inline-flex items-center gap-1 rounded-full bg-primary-soft px-2 py-0.5 text-xs text-primary">
                    {opt?.label ?? String(v)}
                    <X
                      class="h-3 w-3 cursor-pointer"
                      onClick={(e: MouseEvent) => {
                        e.stopPropagation()
                        emit('update:modelValue', props.modelValue.filter((x) => String(x) !== String(v)))
                      }}
                    />
                  </span>
                )
              })}
              <ChevronDown class="pointer-events-none absolute right-3 top-2.5 h-4 w-4 text-primary" />
            </button>
          ),
          default: () => (
            <div>
              {props.searchable && (
                <div class="relative mb-1">
                  <Search class="absolute left-2 top-2 h-3.5 w-3.5 text-muted" />
                  <input
                    value={keyword.value}
                    onInput={(e: Event) => (keyword.value = (e.target as HTMLInputElement).value)}
                    placeholder={t('admin.common.search')}
                    class="h-8 w-full rounded-[10px] border border-line bg-surface-strong pl-7 pr-2 text-xs text-fg focus:border-primary focus:outline-none"
                  />
                </div>
              )}
              <div class="max-h-60 overflow-y-auto">
                {filtered.value.length === 0 && <p class="px-2 py-3 text-center text-xs text-muted">{t('admin.common.noData')}</p>}
                {filtered.value.map((opt) => {
                  const on = selectedSet.value.has(String(opt.value))
                  return (
                    <button
                      type="button"
                      key={String(opt.value)}
                      onClick={() => toggle(opt)}
                      class={cn(
                        'flex w-full items-center justify-between gap-2 rounded-[10px] px-2.5 py-1.5 text-left text-sm transition-colors hover:bg-primary-soft',
                        on && 'text-primary',
                      )}
                    >
                      <span class="truncate">{opt.label}</span>
                      {on && <Check class="h-3.5 w-3.5 shrink-0" />}
                    </button>
                  )
                })}
              </div>
            </div>
          ),
        }}
      </Popover>
    )
  },
})

export default MultiSelect
