import { defineComponent, onBeforeUnmount, ref, watch, type PropType } from 'vue'
import { Input } from '@/components/ui'

export interface SearchOption {
  id: number
  label: string
}

/** Search a paged server list and keep the selected ID separate from the typed query. */
export const SearchPicker = defineComponent({
  name: 'ResellerSearchPicker',
  props: {
    modelValue: { type: Object as PropType<SearchOption | null>, default: null },
    search: { type: Function as PropType<(query: string) => Promise<SearchOption[]>>, required: true },
    placeholder: { type: String, required: true },
    emptyText: { type: String, required: true },
    loadingText: { type: String, required: true },
  },
  emits: { 'update:modelValue': (_value: SearchOption | null) => true },
  setup(props, { emit }) {
    const query = ref(props.modelValue?.label ?? '')
    const options = ref<SearchOption[]>([])
    const open = ref(false)
    const loading = ref(false)
    const active = ref(0)
    let timer: ReturnType<typeof setTimeout> | undefined
    let sequence = 0

    watch(() => props.modelValue, (value) => { if (value) query.value = value.label })

    const load = async () => {
      const current = ++sequence
      loading.value = true
      try {
        const found = await props.search(props.modelValue ? '' : query.value.trim())
        if (current === sequence) {
          options.value = found
          active.value = 0
        }
      } catch {
        if (current === sequence) options.value = []
      } finally {
        if (current === sequence) loading.value = false
      }
    }
    const schedule = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => void load(), 250)
    }
    const select = (item: SearchOption) => {
      if (timer) clearTimeout(timer)
      sequence++
      query.value = item.label
      open.value = false
      emit('update:modelValue', item)
    }
    const onInput = (value: string | number) => {
      query.value = String(value)
      if (props.modelValue) emit('update:modelValue', null)
      open.value = true
      schedule()
    }
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') open.value = false
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        open.value = true
        active.value = Math.max(0, Math.min(options.value.length - 1, active.value + (event.key === 'ArrowDown' ? 1 : -1)))
      }
      if (event.key === 'Enter' && open.value && options.value[active.value]) {
        event.preventDefault()
        select(options.value[active.value])
      }
    }
    onBeforeUnmount(() => {
      if (timer) clearTimeout(timer)
      sequence++
    })

    return () => (
      <div class="relative" onFocusout={(event: FocusEvent) => {
        if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) open.value = false
      }}>
        <Input
          modelValue={query.value}
          placeholder={props.placeholder}
          autocomplete="off"
          onUpdate:modelValue={onInput}
          onFocus={() => { open.value = true; void load() }}
          onKeydown={onKeydown}
        />
        {open.value && (
          <div role="listbox" class="absolute z-30 mt-1 min-h-28 max-h-72 w-full overflow-y-auto rounded-zs border border-line bg-surface-solid p-2 shadow-zs">
            {loading.value ? <div class="px-3 py-3 text-sm text-muted">{props.loadingText}</div> : options.value.length ? options.value.map((item, index) => (
              <button
                key={item.id}
                type="button"
                role="option"
                aria-selected={index === active.value}
                class={['block min-h-12 w-full rounded-zs-sm px-3 py-3 text-left text-sm hover:bg-primary-soft', index === active.value ? 'bg-primary-soft' : '']}
                onPointerdown={(event: PointerEvent) => {
                  // Safari may report a null relatedTarget on blur and remove the list before click.
                  event.preventDefault()
                  select(item)
                }}
                onClick={() => select(item)}
              >{item.label}</button>
            )) : <div class="px-3 py-3 text-sm text-muted">{props.emptyText}</div>}
          </div>
        )}
      </div>
    )
  },
})
