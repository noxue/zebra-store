import { computed, defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import { cn } from './cn'

/** Page buttons with ellipsis. Emits `change(page)`. */
export const Pagination = defineComponent({
  name: 'ZsPagination',
  props: {
    page: { type: Number, required: true },
    totalPages: { type: Number, required: true },
    total: { type: Number, default: undefined },
  },
  emits: { change: (_page: number) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const pages = computed<Array<number | '…'>>(() => {
      const total = Math.max(1, props.totalPages)
      const cur = props.page
      if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1)
      const out: Array<number | '…'> = [1]
      const start = Math.max(2, cur - 1)
      const end = Math.min(total - 1, cur + 1)
      if (start > 2) out.push('…')
      for (let i = start; i <= end; i++) out.push(i)
      if (end < total - 1) out.push('…')
      out.push(total)
      return out
    })
    const go = (p: number) => {
      if (p < 1 || p > props.totalPages || p === props.page) return
      emit('change', p)
    }
    const btn = 'flex size-9 items-center justify-center rounded-full text-sm font-bold transition zs-num'
    return () =>
      props.totalPages > 1 ? (
        <nav class="flex flex-wrap items-center justify-center gap-1.5 pt-2" aria-label="pagination">
          <button type="button" class={cn(btn, 'border border-line bg-surface hover:bg-primary-soft disabled:opacity-40')} disabled={props.page <= 1} aria-label={t('pagination.previous')} onClick={() => go(props.page - 1)}>
            <ChevronLeft class="size-4" />
          </button>
          {pages.value.map((p, i) =>
            p === '…' ? (
              <span key={`e${i}`} class="px-1 text-muted">
                …
              </span>
            ) : (
              <button
                key={p}
                type="button"
                class={cn(btn, p === props.page ? 'zs-gradient-bg text-on-primary shadow-zs' : 'border border-line bg-surface text-fg hover:bg-primary-soft')}
                onClick={() => go(p)}
              >
                {p}
              </button>
            ),
          )}
          <button type="button" class={cn(btn, 'border border-line bg-surface hover:bg-primary-soft disabled:opacity-40')} disabled={props.page >= props.totalPages} aria-label={t('pagination.next')} onClick={() => go(props.page + 1)}>
            <ChevronRight class="size-4" />
          </button>
        </nav>
      ) : null
  },
})
