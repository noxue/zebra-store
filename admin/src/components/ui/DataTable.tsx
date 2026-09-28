import { Fragment, type VNodeChild } from 'vue'
import { cn } from './cn'
import { Checkbox } from './Checkbox'
import { EmptyState } from './EmptyState'
import type { RowSelection } from '@/composables/useSelection'

export interface DataTableColumn<T> {
  key: string
  title: VNodeChild
  class?: string
  headerClass?: string
  align?: 'left' | 'center' | 'right'
  width?: string
  render?: (row: T, index: number) => VNodeChild
}

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[]
  rows: T[]
  rowKey: (row: T) => string | number
  loading?: boolean
  selection?: RowSelection<T>
  emptyText?: string
  empty?: VNodeChild
  /** Extra full-width row rendered under a row (expandable content). */
  expanded?: (row: T) => VNodeChild | null | undefined
  rowClass?: (row: T) => string
  onRowClick?: (row: T) => void
  /** Minimum table width before horizontal scroll kicks in (e.g. '960px'). */
  minWidth?: string
  skeletonRows?: number
  bare?: boolean
  /** Tighter horizontal cell padding for wide tables (many columns plus a sticky action column). */
  dense?: boolean
}

const alignClass = (a?: 'left' | 'center' | 'right') => (a === 'center' ? 'text-center' : a === 'right' ? 'text-right' : 'text-left')

function cellValue<T>(row: T, key: string): VNodeChild {
  const value = (row as Record<string, unknown>)[key]
  if (value === null || value === undefined || value === '') return '-'
  if (typeof value === 'object') return JSON.stringify(value)
  return String(value)
}

/** Skeleton rows shown while a list is loading. */
export function TableSkeleton(props: { rows?: number; cols?: number }) {
  const rows = props.rows ?? 5
  const cols = props.cols ?? 5
  return (
    <div class="space-y-3 p-5" aria-busy="true">
      {Array.from({ length: rows }).map((_, r) => (
        <div key={r} class="flex gap-4">
          {Array.from({ length: cols }).map((__, c) => (
            <div key={c} class="zs-skeleton h-4 flex-1 rounded-full" style={{ opacity: String(1 - r * 0.12) }} />
          ))}
        </div>
      ))}
    </div>
  )
}

/** Generic, typed data table (functional component). */
export function DataTable<T>(props: DataTableProps<T>) {
  const { columns, rows, selection } = props
  const colCount = columns.length + (selection ? 1 : 0)
  const padX = props.dense ? 'px-2.5' : 'px-4'
  const body = () => {
    if (props.loading && rows.length === 0) {
      return (
        <tr>
          <td colspan={colCount}>
            <TableSkeleton rows={props.skeletonRows ?? 5} cols={Math.min(colCount, 6)} />
          </td>
        </tr>
      )
    }
    if (rows.length === 0) {
      return (
        <tr>
          <td colspan={colCount}>{props.empty ?? <EmptyState title={props.emptyText} />}</td>
        </tr>
      )
    }
    return rows.map((row, index) => {
      const extra = props.expanded?.(row)
      return (
        <Fragment key={props.rowKey(row)}>
          <tr
            class={cn(
              'border-b border-line/70 transition-colors last:border-b-0 hover:bg-[var(--zs-row-hover)]',
              props.onRowClick && 'cursor-pointer',
              selection?.isSelected(row) && 'bg-primary-soft/60',
              props.rowClass?.(row),
            )}
            onClick={() => props.onRowClick?.(row)}
          >
            {selection && (
              <td class="w-10 px-4 py-3 align-middle" onClick={(e: MouseEvent) => e.stopPropagation()}>
                <Checkbox modelValue={selection.isSelected(row)} onUpdate:modelValue={() => selection.toggle(row)} />
              </td>
            )}
            {columns.map((col) => (
              <td key={col.key} class={cn(padX, 'py-3 align-middle text-sm text-fg', alignClass(col.align), col.class)}>
                {col.render ? col.render(row, index) : cellValue(row, col.key)}
              </td>
            ))}
          </tr>
          {extra ? (
            <tr class="border-b border-line/70 bg-surface-muted/50">
              <td colspan={colCount} class="px-4 py-3">
                {extra}
              </td>
            </tr>
          ) : null}
        </Fragment>
      )
    })
  }
  return (
    <div class={cn('relative overflow-x-auto', !props.bare && 'zs-glass rounded-zs-lg shadow-zs-sm')}>
      {props.loading && rows.length > 0 && <div class="zs-gradient-bg absolute inset-x-0 top-0 z-10 h-0.5 animate-pulse" />}
      <table class="w-full border-collapse" style={props.minWidth ? { minWidth: props.minWidth } : undefined}>
        <thead>
          <tr class="border-b border-line bg-primary-soft/40">
            {selection && (
              <th class="w-10 px-4 py-3 text-left">
                <Checkbox
                  modelValue={selection.allSelected}
                  indeterminate={selection.someSelected}
                  onUpdate:modelValue={() => selection.toggleAll()}
                />
              </th>
            )}
            {columns.map((col) => (
              <th
                key={col.key}
                style={col.width ? { width: col.width } : undefined}
                class={cn('whitespace-nowrap py-3 text-xs font-semibold text-muted', padX, alignClass(col.align), col.headerClass)}
              >
                {col.title}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{body()}</tbody>
      </table>
    </div>
  )
}

export default DataTable
