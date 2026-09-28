import { computed, defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ImagePlus, Search, Trash2, Upload, Check } from 'lucide-vue-next'
import { adminAPI } from '@/api/admin'
import type { AdminMedia } from '@/api/types'
import { getImageUrl } from '@/utils/image'
import { useMediaUpload } from '@/composables/useMediaUpload'
import { Button, Dialog, Input, Tabs, Loader, EmptyState, cn } from '@/components/ui'

const PAGE_SIZE = 18

/**
 * Pick image(s) from the media library or upload new ones.
 * v-model: string (single) or string[] (multiple). `dialogOnly` + `v-model:open` for editor usage.
 */
export const MediaPicker = defineComponent({
  name: 'MediaPicker',
  props: {
    modelValue: { type: [String, Array] as PropType<string | string[] | null | undefined>, default: '' },
    multiple: Boolean,
    scene: { type: String, default: 'common' },
    dialogOnly: Boolean,
    open: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: string | string[]) => true, 'update:open': (_v: boolean) => true, pick: (_urls: string[]) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const internalOpen = ref(false)
    const dialogOpen = computed({
      get: () => (props.dialogOnly ? props.open : internalOpen.value),
      set: (v: boolean) => {
        if (props.dialogOnly) emit('update:open', v)
        else internalOpen.value = v
      },
    })
    const tab = ref('library')
    const items = ref<AdminMedia[]>([])
    const loading = ref(false)
    const search = ref('')
    const page = ref(1)
    const total = ref(0)
    const selected = ref<string[]>([])
    const fileInput = ref<HTMLInputElement | null>(null)
    const dialogFileInput = ref<HTMLInputElement | null>(null)
    const dragOver = ref(false)
    const { uploading, progress, uploadFiles } = useMediaUpload()

    const current = computed(() => {
      const v = props.modelValue
      if (!v) return [] as string[]
      return Array.isArray(v) ? v.filter(Boolean) : [v]
    })
    const totalPage = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)))

    const fetchMedia = async (p = 1) => {
      loading.value = true
      try {
        const res = await adminAPI.getMedia({ page: p, page_size: PAGE_SIZE, search: search.value || undefined })
        items.value = res.data?.items ?? []
        total.value = res.data?.total ?? 0
        page.value = p
      } catch {
        items.value = []
      } finally {
        loading.value = false
      }
    }

    let timer: ReturnType<typeof setTimeout> | undefined
    watch(search, () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => fetchMedia(1), 300)
    })
    watch(dialogOpen, (open) => {
      if (open) {
        selected.value = props.dialogOnly ? [] : [...current.value]
        tab.value = 'library'
        void fetchMedia(1)
      }
    })

    const toggle = (path: string) => {
      if (props.multiple) {
        selected.value = selected.value.includes(path) ? selected.value.filter((p) => p !== path) : [...selected.value, path]
      } else {
        selected.value = [path]
      }
    }

    const commit = (urls: string[]) => {
      emit('pick', urls)
      if (props.multiple) emit('update:modelValue', urls)
      else emit('update:modelValue', urls[0] ?? '')
    }

    const confirm = () => {
      commit(selected.value)
      dialogOpen.value = false
    }

    const directUpload = async (files: File[]) => {
      const urls = await uploadFiles(files.filter((f) => f.type.startsWith('image/') || f.type === ''), props.scene)
      if (!urls.length) return
      if (props.multiple) emit('update:modelValue', [...current.value, ...urls])
      else emit('update:modelValue', urls[urls.length - 1] ?? '')
    }

    const dialogUpload = async (files: File[]) => {
      const urls = await uploadFiles(files, props.scene)
      if (urls.length) selected.value = props.multiple ? [...selected.value, ...urls] : [urls[urls.length - 1] ?? '']
      tab.value = 'library'
      await fetchMedia(1)
    }

    const remove = (path: string) => {
      if (props.multiple) emit('update:modelValue', current.value.filter((p) => p !== path))
      else emit('update:modelValue', '')
    }

    const thumbSize = computed(() => (props.size === 'sm' ? 'h-16 w-16' : 'h-24 w-24'))

    const renderDialog = () => (
      <Dialog v-model={dialogOpen.value} title={t('admin.mediaPicker.title')} size="xl">
        {{
          default: () => (
            <div class="space-y-4">
              <Tabs
                v-model={tab.value}
                items={[
                  { key: 'library', label: t('admin.mediaPicker.tabLibrary') },
                  { key: 'upload', label: t('admin.mediaPicker.tabUpload') },
                ]}
              />
              {tab.value === 'library' ? (
                <div class="space-y-3">
                  <Input icon={Search} v-model={search.value} placeholder={t('admin.mediaPicker.searchPlaceholder')} />
                  {loading.value ? (
                    <Loader />
                  ) : items.value.length === 0 ? (
                    <EmptyState title={t('admin.mediaPicker.empty')} />
                  ) : (
                    <div class="grid grid-cols-3 gap-3 sm:grid-cols-4 md:grid-cols-6">
                      {items.value.map((m) => {
                        const on = selected.value.includes(m.path)
                        return (
                          <button
                            type="button"
                            key={m.id}
                            onClick={() => toggle(m.path)}
                            class={cn(
                              'group relative aspect-square overflow-hidden rounded-zs border-2 bg-surface-muted transition-all',
                              on ? 'border-primary shadow-glow' : 'border-transparent hover:border-line-strong',
                            )}
                          >
                            <img src={getImageUrl(m.path)} alt={m.name} loading="lazy" class="h-full w-full object-cover" />
                            <span class="absolute inset-x-0 bottom-0 truncate bg-black/45 px-1.5 py-0.5 text-[10px] text-white">{m.name || m.filename}</span>
                            {on && (
                              <span class="zs-gradient-bg absolute right-1.5 top-1.5 flex h-5 w-5 items-center justify-center rounded-full text-white">
                                <Check class="h-3 w-3" />
                              </span>
                            )}
                          </button>
                        )
                      })}
                    </div>
                  )}
                  {totalPage.value > 1 && (
                    <div class="flex items-center justify-center gap-2">
                      <Button size="sm" disabled={page.value <= 1} onClick={() => fetchMedia(page.value - 1)}>
                        {t('admin.common.prevPage')}
                      </Button>
                      <span class="zs-num text-sm">
                        {page.value}/{totalPage.value}
                      </span>
                      <Button size="sm" disabled={page.value >= totalPage.value} onClick={() => fetchMedia(page.value + 1)}>
                        {t('admin.common.nextPage')}
                      </Button>
                    </div>
                  )}
                </div>
              ) : (
                <div
                  class={cn(
                    'flex cursor-pointer flex-col items-center justify-center gap-3 rounded-zs-lg border-2 border-dashed p-10 text-center transition-colors',
                    dragOver.value ? 'border-primary bg-primary-soft' : 'border-line-strong hover:border-primary',
                  )}
                  onClick={() => dialogFileInput.value?.click()}
                  onDragover={(e: DragEvent) => {
                    e.preventDefault()
                    dragOver.value = true
                  }}
                  onDragleave={() => (dragOver.value = false)}
                  onDrop={(e: DragEvent) => {
                    e.preventDefault()
                    dragOver.value = false
                    void dialogUpload(Array.from(e.dataTransfer?.files ?? []))
                  }}
                >
                  <span class="zs-gradient-bg flex h-14 w-14 items-center justify-center rounded-full text-white shadow-zs">
                    <Upload class="h-6 w-6" />
                  </span>
                  <p class="text-sm text-fg">{t('admin.mediaPicker.uploadHint')}</p>
                  {uploading.value && <p class="text-xs text-muted">{t('admin.media.uploadProgress', progress.value)}</p>}
                  <input
                    ref={dialogFileInput}
                    type="file"
                    accept="image/*"
                    multiple
                    class="hidden"
                    onChange={(e: Event) => {
                      const el = e.target as HTMLInputElement
                      void dialogUpload(Array.from(el.files ?? []))
                      el.value = ''
                    }}
                  />
                </div>
              )}
            </div>
          ),
          footer: () => (
            <>
              <span class="mr-auto text-xs text-muted">{t('admin.mediaPicker.selected', { count: selected.value.length })}</span>
              <Button onClick={() => (dialogOpen.value = false)}>{t('admin.mediaPicker.cancel')}</Button>
              <Button variant="primary" disabled={selected.value.length === 0} onClick={confirm}>
                {t('admin.mediaPicker.confirm')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )

    return () => {
      if (props.dialogOnly) return renderDialog()
      return (
        <div class="space-y-2">
          <div class="flex flex-wrap gap-2">
            {current.value.map((path) => (
              <div key={path} class={cn('group relative overflow-hidden rounded-zs border border-line bg-surface-muted', thumbSize.value)}>
                <img src={getImageUrl(path)} alt="" class="h-full w-full object-cover" />
                <button
                  type="button"
                  onClick={() => remove(path)}
                  class="absolute right-1 top-1 hidden rounded-full bg-danger p-1 text-white group-hover:block"
                  aria-label="remove"
                >
                  <Trash2 class="h-3 w-3" />
                </button>
              </div>
            ))}
            {(props.multiple || current.value.length === 0) && (
              <div
                class={cn(
                  'flex cursor-pointer flex-col items-center justify-center gap-1 rounded-zs border-2 border-dashed text-muted transition-colors hover:border-primary hover:text-primary',
                  dragOver.value ? 'border-primary bg-primary-soft' : 'border-line-strong',
                  thumbSize.value,
                )}
                onClick={() => fileInput.value?.click()}
                onDragover={(e: DragEvent) => {
                  e.preventDefault()
                  dragOver.value = true
                }}
                onDragleave={() => (dragOver.value = false)}
                onDrop={(e: DragEvent) => {
                  e.preventDefault()
                  dragOver.value = false
                  void directUpload(Array.from(e.dataTransfer?.files ?? []))
                }}
              >
                <ImagePlus class="h-5 w-5" />
                <span class="text-[10px]">{uploading.value ? `${progress.value.current}/${progress.value.total}` : t('admin.mediaPicker.tabUpload')}</span>
              </div>
            )}
          </div>
          <div class="flex flex-wrap gap-2">
            <Button size="xs" variant="soft" onClick={() => (dialogOpen.value = true)}>
              {current.value.length && !props.multiple ? t('admin.mediaPicker.changeImage') : t('admin.mediaPicker.selectFromLibrary')}
            </Button>
          </div>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            multiple={props.multiple}
            class="hidden"
            onChange={(e: Event) => {
              const el = e.target as HTMLInputElement
              void directUpload(Array.from(el.files ?? []))
              el.value = ''
            }}
          />
          {renderDialog()}
        </div>
      )
    }
  },
})

export default MediaPicker
