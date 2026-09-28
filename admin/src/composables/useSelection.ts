import { computed, ref, type Ref } from 'vue'

/** Row selection over the current page (ids). Matches DataTable's `selection` prop. */
export function useSelection<T>(rows: Ref<T[]>, getId: (row: T) => number) {
  const selected = ref<Set<number>>(new Set())
  const selectedIds = computed(() => Array.from(selected.value))
  const pageIds = computed(() => rows.value.map(getId))
  const allSelected = computed(() => pageIds.value.length > 0 && pageIds.value.every((id) => selected.value.has(id)))
  const someSelected = computed(() => !allSelected.value && pageIds.value.some((id) => selected.value.has(id)))

  const isSelected = (row: T) => selected.value.has(getId(row))
  const toggle = (row: T) => {
    const next = new Set(selected.value)
    const id = getId(row)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selected.value = next
  }
  const toggleAll = () => {
    const next = new Set(selected.value)
    if (allSelected.value) pageIds.value.forEach((id) => next.delete(id))
    else pageIds.value.forEach((id) => next.add(id))
    selected.value = next
  }
  const clear = () => {
    selected.value = new Set()
  }

  return { selected, selectedIds, allSelected, someSelected, isSelected, toggle, toggleAll, clear }
}

export interface RowSelection<T> {
  allSelected: boolean
  someSelected: boolean
  isSelected: (row: T) => boolean
  toggle: (row: T) => void
  toggleAll: () => void
}

/** Adapt a `useSelection()` result to DataTable's plain `selection` prop (call inside render). */
export function toRowSelection<T>(sel: ReturnType<typeof useSelection<T>>): RowSelection<T> {
  return {
    allSelected: sel.allSelected.value,
    someSelected: sel.someSelected.value,
    isSelected: sel.isSelected,
    toggle: sel.toggle,
    toggleAll: sel.toggleAll,
  }
}
