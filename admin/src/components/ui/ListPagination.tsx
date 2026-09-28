import { defineComponent, ref, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import type { Pagination } from '@/api/types'
import { PAGE_SIZE_OPTIONS, resolveJumpTarget } from '@/composables/useListPage'
import { Button } from './Button'
import { Input } from './Input'
import { Select } from './Select'

export const ListPagination = defineComponent({
  name: 'ZsListPagination',
  props: {
    pagination: { type: Object as PropType<Pagination>, required: true },
    pageSizeOptions: { type: Array as PropType<number[]>, default: () => [...PAGE_SIZE_OPTIONS] },
    hideWhenEmpty: Boolean,
  },
  emits: { changePage: (_p: number) => true, changePageSize: (_s: number) => true },
  setup(props, { emit, slots }) {
    const { t } = useI18n()
    const jump = ref<string | number>('')
    const doJump = () => {
      const p = props.pagination
      const target = resolveJumpTarget(jump.value, p.page, p.total_page)
      if (target !== null) emit('changePage', target)
    }
    return () => {
      const p = props.pagination
      if (props.hideWhenEmpty && p.total === 0) return null
      const totalPage = Math.max(p.total_page, 1)
      return (
        <div class="flex flex-wrap items-center justify-between gap-3 px-1 py-3">
          <div class="flex items-center gap-3 text-xs text-muted">
            <span>{t('admin.common.pageInfo', { total: p.total, page: p.page, totalPage })}</span>
            {slots.actions?.()}
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-xs text-muted">{t('admin.common.pageSize')}</span>
            <div class="w-20">
              <Select
                size="sm"
                modelValue={p.page_size}
                options={props.pageSizeOptions.map((s) => ({ label: String(s), value: s }))}
                onUpdate:modelValue={(v) => emit('changePageSize', Number(v))}
              />
            </div>
            <div class="w-20">
              <Input size="sm" type="number" min={1} max={totalPage} v-model={jump.value} placeholder={t('admin.common.jumpPlaceholder')} onEnter={doJump} />
            </div>
            <Button size="sm" onClick={doJump}>
              {t('admin.common.jumpTo')}
            </Button>
            <Button size="sm" disabled={p.page <= 1} onClick={() => emit('changePage', p.page - 1)}>
              <ChevronLeft class="h-3.5 w-3.5" />
              {t('admin.common.prevPage')}
            </Button>
            <span class="zs-num min-w-[3rem] text-center text-sm text-fg">
              {p.page}/{totalPage}
            </span>
            <Button size="sm" disabled={p.page >= totalPage} onClick={() => emit('changePage', p.page + 1)}>
              {t('admin.common.nextPage')}
              <ChevronRight class="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      )
    }
  },
})

export default ListPagination
