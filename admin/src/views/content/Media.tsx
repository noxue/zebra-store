import { defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckSquare, Search, Trash2, Upload, X } from 'lucide-vue-next'
import { Button, Checkbox, EmptyState, FilterBar, Input, ListPagination, Loader, PageHeader, Select, cn, inputBase } from '@/components/ui'
import type { AdminMedia } from '@/api/types'
import { getImageUrl } from '@/utils/image'
import { formatFileSize } from './contentUtils'
import { MEDIA_PAGE_SIZE_OPTIONS, MEDIA_SCENES, useMedia } from './useMedia'

/** Inline rename box: autofocus + select, Enter/blur saves, Escape cancels. */
const RenameInput = defineComponent({
  name: 'MediaRenameInput',
  props: { modelValue: { type: String, default: '' } },
  emits: { 'update:modelValue': (_v: string) => true, save: () => true, cancel: () => true },
  setup(props, { emit }) {
    const el = ref<HTMLInputElement | null>(null)
    onMounted(() => {
      el.value?.focus()
      el.value?.select()
    })
    return () => (
      <input
        ref={el}
        class={cn(inputBase, 'h-7 px-2 text-xs')}
        value={props.modelValue}
        onInput={(e: Event) => emit('update:modelValue', (e.target as HTMLInputElement).value)}
        onBlur={() => emit('save')}
        onKeydown={(e: KeyboardEvent) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            emit('save')
          } else if (e.key === 'Escape') emit('cancel')
        }}
      />
    )
  },
})

export default defineComponent({
  name: 'MediaView',
  setup() {
    const { t } = useI18n()
    const m = useMedia()
    const fileInput = ref<HTMLInputElement | null>(null)
    onMounted(() => void m.list.fetchData(1))

    const sceneOptions = () => [
      { label: t('admin.media.filters.sceneAll'), value: '__all__' },
      ...MEDIA_SCENES.map((s) => ({ label: t(`admin.media.scenes.${s}`), value: s })),
    ]

    const onFiles = (e: Event) => {
      const el = e.target as HTMLInputElement
      const files = Array.from(el.files ?? [])
      el.value = ''
      void m.upload(files)
    }

    const renderCard = (item: AdminMedia) => {
      const selected = m.isSelected(item.id)
      return (
        <div
          key={item.id}
          class={cn(
            'group relative overflow-hidden rounded-zs border bg-surface-strong shadow-zs-sm transition-all',
            selected ? 'border-primary ring-2 ring-primary/30' : 'border-line zs-card-hover',
          )}
        >
          <div class="relative aspect-square overflow-hidden bg-surface-muted">
            <img src={getImageUrl(item.path)} alt={item.name} class="h-full w-full object-contain" loading="lazy" />
            {m.batchMode.value ? (
              <>
                <button
                  type="button"
                  class="absolute inset-0 z-10 cursor-pointer"
                  aria-label={t('admin.media.batch.toggleItem', { name: item.name })}
                  aria-pressed={selected}
                  onClick={() => m.toggleSelect(item.id)}
                />
                <div class="absolute left-2 top-2 z-20 rounded-zs-sm bg-surface-solid/90 p-1 shadow-zs-sm">
                  <Checkbox modelValue={selected} onUpdate:modelValue={() => m.toggleSelect(item.id)} />
                </div>
              </>
            ) : (
              <div class="absolute inset-0 flex items-center justify-center bg-fg/40 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                <Button size="sm" variant="danger" onClick={() => m.remove(item)}>
                  <Trash2 class="h-3.5 w-3.5" />
                  {t('admin.common.delete')}
                </Button>
              </div>
            )}
          </div>
          <div class="space-y-1 p-2">
            {m.editingId.value === item.id ? (
              <RenameInput
                modelValue={m.editingName.value}
                onUpdate:modelValue={(v: string) => (m.editingName.value = v)}
                onSave={() => void m.saveRename(item)}
                onCancel={m.cancelRename}
              />
            ) : (
              <p
                class="cursor-pointer truncate text-xs font-medium text-fg hover:text-primary"
                title={t('admin.media.card.editName')}
                onClick={() => m.startRename(item)}
              >
                {item.name}
              </p>
            )}
            <p class="truncate text-[10px] text-muted">
              {formatFileSize(item.size)}
              {item.width > 0 && item.height > 0 && <span> · {t('admin.media.card.dimensions', { width: item.width, height: item.height })}</span>}
            </p>
          </div>
        </div>
      )
    }

    const renderGrid = () => {
      if (m.list.loading.value) return <Loader />
      if (!m.list.items.value.length) return <EmptyState title={t('admin.media.empty')} />
      return <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">{m.list.items.value.map(renderCard)}</div>
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.media.title')}>
          {{
            actions: () => (
              <>
                <Button
                  size="sm"
                  variant={m.batchMode.value ? 'soft' : 'outline'}
                  disabled={m.list.loading.value || m.uploading.value}
                  onClick={m.toggleBatchMode}
                >
                  {m.batchMode.value ? <X class="h-4 w-4" /> : <CheckSquare class="h-4 w-4" />}
                  {m.batchMode.value ? t('admin.media.batch.exit') : t('admin.media.batch.mode')}
                </Button>
                <Button size="sm" variant="primary" loading={m.uploading.value} disabled={m.uploading.value} onClick={() => fileInput.value?.click()}>
                  {!m.uploading.value && <Upload class="h-4 w-4" />}
                  {m.uploading.value ? t('admin.media.uploadProgress', { current: m.uploadProgress.current, total: m.uploadProgress.total }) : t('admin.media.uploadNew')}
                </Button>
                <input ref={fileInput} type="file" accept="image/*" multiple class="hidden" data-testid="media-upload" onChange={onFiles} />
              </>
            ),
          }}
        </PageHeader>

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={m.filters.search} placeholder={t('admin.media.searchPlaceholder')} onEnter={m.list.handleSearch} />
                <Select v-model={m.filters.scene} options={sceneOptions()} placeholder={t('admin.media.filters.scenePlaceholder')} />
              </>
            ),
          }}
        </FilterBar>

        {m.batchMode.value && (
          <div class="flex flex-wrap items-center gap-3 rounded-zs border border-primary/30 bg-primary-soft px-4 py-3">
            <span class="text-sm font-medium text-fg">{t('admin.media.batch.selected', { count: m.selected.value.size })}</span>
            <Button size="sm" disabled={!m.list.items.value.length || m.batchOperating.value} onClick={m.toggleSelectAll}>
              {m.allSelected.value ? t('admin.media.batch.deselectPage') : t('admin.media.batch.selectPage')}
            </Button>
            <Button size="sm" variant="danger" disabled={!m.selected.value.size || m.batchOperating.value} loading={m.batchOperating.value} onClick={m.batchDelete}>
              {t('admin.media.batch.delete')}
            </Button>
            <Button size="sm" variant="link" class="ml-auto" disabled={m.batchOperating.value} onClick={m.clearSelection}>
              {t('admin.media.batch.clearSelection')}
            </Button>
          </div>
        )}

        {renderGrid()}

        <ListPagination
          pagination={m.list.pagination.value}
          pageSizeOptions={MEDIA_PAGE_SIZE_OPTIONS}
          onChangePage={m.list.changePage}
          onChangePageSize={m.list.changePageSize}
        />
      </div>
    )
  },
})
