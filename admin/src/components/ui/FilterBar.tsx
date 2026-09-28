import { defineComponent } from 'vue'

/** Glass card holding list filters in a responsive grid; `actions` slot for search/reset buttons. */
export const FilterBar = defineComponent({
  name: 'ZsFilterBar',
  props: { cols: { type: Number, default: 4 } },
  setup(props, { slots }) {
    const colClass: Record<number, string> = {
      2: 'sm:grid-cols-2',
      3: 'sm:grid-cols-2 lg:grid-cols-3',
      4: 'sm:grid-cols-2 lg:grid-cols-4',
      5: 'sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5',
      6: 'sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6',
    }
    return () => (
      <div class="zs-glass rounded-zs-lg p-4 shadow-zs-sm">
        <div class={`grid grid-cols-1 items-end gap-3 ${colClass[props.cols] ?? colClass[4]}`}>{slots.default?.()}</div>
        {slots.actions && <div class="mt-3 flex flex-wrap items-center justify-end gap-2">{slots.actions()}</div>}
      </div>
    )
  },
})

export default FilterBar
