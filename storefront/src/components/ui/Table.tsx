import { defineComponent, type PropType, type VNodeChild } from 'vue'
import { cn } from './cn'

export interface TableColumn<T> {
  key: string
  title: string
  align?: 'left' | 'center' | 'right'
  width?: string
  class?: string
  render?: (row: T, index: number) => VNodeChild
}

/**
 * Data table with glass header. Generic over row type via `columns`.
 * Columns without `render` print `row[key]`.
 */
export const DataTable = defineComponent({
  name: 'ZsDataTable',
  props: {
    columns: { type: Array as PropType<TableColumn<never>[]>, required: true },
    rows: { type: Array as PropType<unknown[]>, default: () => [] },
    rowKey: { type: [String, Function] as PropType<string | ((row: never, index: number) => string | number)>, default: 'id' },
    loading: Boolean,
    emptyText: { type: String, default: '-' },
  },
  setup(props, { slots }) {
    const alignClass = (a?: string) => (a === 'right' ? 'text-right' : a === 'center' ? 'text-center' : 'text-left')
    const keyOf = (row: unknown, index: number): string | number => {
      if (typeof props.rowKey === 'function') return props.rowKey(row as never, index)
      const v = (row as Record<string, unknown>)[props.rowKey]
      return typeof v === 'string' || typeof v === 'number' ? v : index
    }
    return () => (
      <div class="overflow-x-auto rounded-zs border border-line">
        <table class="w-full min-w-[560px] border-collapse text-sm">
          <thead>
            <tr class="zs-soft-bg">
              {props.columns.map((col) => (
                <th key={col.key} style={col.width ? { width: col.width } : undefined} class={cn('whitespace-nowrap px-4 py-3 text-xs font-bold text-muted', alignClass(col.align))}>
                  {col.title}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {props.loading ? (
              Array.from({ length: 3 }).map((_, i) => (
                <tr key={`sk-${i}`} class="border-t border-line">
                  {props.columns.map((col) => (
                    <td key={col.key} class="px-4 py-3">
                      <div class="zs-skeleton h-4 w-3/4" />
                    </td>
                  ))}
                </tr>
              ))
            ) : props.rows.length === 0 ? (
              <tr>
                <td colspan={props.columns.length} class="px-4 py-10 text-center text-muted">
                  {slots.empty ? slots.empty() : props.emptyText}
                </td>
              </tr>
            ) : (
              props.rows.map((row, index) => (
                <tr key={keyOf(row, index)} class="border-t border-line transition-colors hover:bg-primary-soft/50">
                  {props.columns.map((col) => (
                    <td key={col.key} class={cn('px-4 py-3 align-middle text-fg', alignClass(col.align), col.class)}>
                      {col.render ? col.render(row as never, index) : String((row as Record<string, unknown>)[col.key] ?? '-')}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    )
  },
})

/** Typed column helper: `columns<Row>([...])`. */
export const columns = <T,>(cols: TableColumn<T>[]): TableColumn<never>[] => cols as unknown as TableColumn<never>[]
