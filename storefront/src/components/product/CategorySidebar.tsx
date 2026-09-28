import { defineComponent, Teleport, Transition, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Filter, LayoutGrid, Search, X } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import type { CategoryGroup, PublicCategory } from '@/utils/category'
import { getImageUrl } from '@/utils/image'
import { Button, Input, cn } from '@/components/ui'

/**
 * Category tree (collapsible parents) + optional search. On < lg it becomes a
 * "filter" button opening a drawer.
 */
export const CategorySidebar = defineComponent({
  name: 'CategorySidebar',
  props: {
    categories: { type: Array as PropType<CategoryGroup[]>, required: true },
    selectedCategory: { type: Number as PropType<number | null>, default: null },
    expandedParentIds: { type: Array as PropType<number[]>, default: () => [] },
    showDrawer: Boolean,
    showSearch: Boolean,
    searchQuery: { type: String, default: '' },
  },
  emits: {
    selectCategory: (_id: number | null, _close?: boolean) => true,
    toggleParent: (_id: number) => true,
    'update:showDrawer': (_v: boolean) => true,
    'update:searchQuery': (_v: string) => true,
    clearSearch: () => true,
  },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()

    const icon = (c: PublicCategory) =>
      c.icon ? <img src={getImageUrl(c.icon)} alt="" class="size-5 shrink-0 rounded-md object-cover" /> : <span class="zs-sparkle text-[10px]">✦</span>

    const item = (c: PublicCategory, close: boolean, child = false) => (
      <button
        key={c.id}
        type="button"
        class={cn(
          'flex w-full min-w-0 items-center gap-2 rounded-full px-3 py-2 text-left text-sm font-bold transition',
          child && 'pl-8 text-[13px]',
          props.selectedCategory === c.id ? 'zs-gradient-bg text-on-primary shadow-zs' : 'text-fg hover:bg-primary-soft hover:text-primary-text',
        )}
        onClick={() => emit('selectCategory', c.id, close)}
      >
        {!child && icon(c)}
        <span class="truncate">{getLocalizedText(c.name as Record<string, string>)}</span>
      </button>
    )

    const tree = (close: boolean) => (
      <div class="space-y-1">
        <button
          type="button"
          class={cn(
            'flex w-full items-center gap-2 rounded-full px-3 py-2 text-sm font-bold transition',
            props.selectedCategory === null ? 'zs-gradient-bg text-on-primary shadow-zs' : 'text-fg hover:bg-primary-soft hover:text-primary-text',
          )}
          onClick={() => emit('selectCategory', null, close)}
        >
          <LayoutGrid class="size-4" />
          {t('products.allCategories')}
        </button>
        {props.categories.map((group) => {
          const expanded = props.expandedParentIds.includes(group.id)
          return (
            <div key={group.id}>
              <div class="flex items-center gap-1">
                <div class="min-w-0 flex-1">{item(group, close)}</div>
                {group.children.length > 0 && (
                  <button
                    type="button"
                    aria-expanded={expanded}
                    class={cn('flex size-8 shrink-0 items-center justify-center rounded-full transition', expanded ? 'bg-primary-soft text-primary-text' : 'text-muted hover:bg-surface-muted')}
                    onClick={() => emit('toggleParent', group.id)}
                  >
                    <ChevronRight class={cn('size-4 transition-transform', expanded && 'rotate-90')} />
                  </button>
                )}
              </div>
              {expanded && group.children.length > 0 && <div class="mt-1 space-y-1">{group.children.map((c) => item(c, close, true))}</div>}
            </div>
          )
        })}
      </div>
    )

    const search = () =>
      props.showSearch && (
        <Input
          modelValue={props.searchQuery}
          placeholder={t('products.searchBoxPlaceholder')}
          onUpdate:modelValue={(v: string) => emit('update:searchQuery', v)}
        >
          {{
            prefix: () => <Search class="size-4" />,
            suffix: props.searchQuery
              ? () => (
                  <button type="button" class="flex size-7 items-center justify-center rounded-full hover:bg-surface-muted" onClick={() => emit('clearSearch')}>
                    <X class="size-4" />
                  </button>
                )
              : undefined,
          }}
        </Input>
      )

    return () => (
      <>
        {/* mobile bar */}
        <div class="flex items-center gap-2 lg:hidden">
          <div class="flex-1">{search()}</div>
          <Button variant="secondary" onClick={() => emit('update:showDrawer', true)}>
            <Filter class="size-4" />
            {t('products.filter')}
          </Button>
        </div>
        {/* desktop */}
        <aside class="hidden w-64 shrink-0 lg:block">
          <div class="zs-card sticky top-24 space-y-4 p-4">
            {search()}
            <div class="zs-title px-2 text-sm text-muted">{t('products.categories')}</div>
            {tree(false)}
          </div>
        </aside>
        <Teleport to="body">
          <Transition name="zs-fade">
            {props.showDrawer && (
              <div class="fixed inset-0 z-[65] bg-[var(--zs-overlay)] backdrop-blur-sm lg:hidden" onClick={() => emit('update:showDrawer', false)}>
                <div
                  class="zs-pop absolute inset-x-0 bottom-0 max-h-[80vh] overflow-y-auto rounded-t-[var(--zs-radius-lg)] border border-line bg-surface-solid p-5 pb-8"
                  onClick={(e: MouseEvent) => e.stopPropagation()}
                >
                  <div class="mb-4 flex items-center justify-between">
                    <h3 class="zs-title text-lg">{t('products.categories')}</h3>
                    <button type="button" aria-label={t('products.closeFilter')} class="flex size-9 items-center justify-center rounded-full hover:bg-primary-soft" onClick={() => emit('update:showDrawer', false)}>
                      <X class="size-5" />
                    </button>
                  </div>
                  {tree(true)}
                </div>
              </div>
            )}
          </Transition>
        </Teleport>
      </>
    )
  },
})
