import { defineComponent, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Check, FileText, Megaphone, Pencil, Plus, Search, Trash2 } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  FormField,
  IdCell,
  Input,
  ListPagination,
  LocalizedInput,
  PageHeader,
  Select,
  Switch,
  Tabs,
  type DataTableColumn,
} from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { RichEditor } from '@/components/RichEditor'
import type { AdminPost } from '@/api/types'
import { formatDate, getLocalizedText } from '@/utils/format'
import { getImageUrl } from '@/utils/image'
import { normalizePostType, postSubmitLabelKey, stripLangLabel, type PostType } from './contentUtils'
import { usePosts, type RelatedProductRef } from './usePosts'

export default defineComponent({
  name: 'PostsView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const router = useRouter()
    const currentTab = ref<PostType>(normalizePostType(route.params.type))
    const p = usePosts(currentTab)

    onMounted(() => {
      void p.fetchCategories()
      void p.list.fetchData(1)
      if (route.query.post_id) void p.openEditById(route.query.post_id)
    })
    watch(
      () => route.params.type,
      (type) => {
        if (type === 'blog' || type === 'notice') currentTab.value = type
      },
    )
    watch(currentTab, (tab) => {
      void p.list.fetchData(1)
      if (route.params.type !== tab) void router.replace(`/posts/${tab}`)
    })
    watch(
      () => route.query.post_id,
      (id) => id && void p.openEditById(id),
    )

    const columns = (): DataTableColumn<AdminPost>[] => [
      { key: 'id', title: t('admin.posts.table.id'), render: (r) => <IdCell value={r.id} /> },
      ...(currentTab.value === 'blog'
        ? [{ key: 'category', title: t('admin.posts.table.category'), class: 'w-32 text-sm text-muted', render: (r: AdminPost) => p.categoryName(r.category_id) }]
        : []),
      {
        key: 'title',
        title: t('admin.posts.table.title'),
        class: 'min-w-[280px]',
        render: (r) => (
          <div class="flex items-center gap-4">
            {r.thumbnail && (
              <div class="h-12 w-12 shrink-0 overflow-hidden rounded-zs-sm border border-line bg-surface-muted">
                <img src={getImageUrl(r.thumbnail)} alt="" class="h-full w-full object-cover" />
              </div>
            )}
            <div class="break-words font-medium text-fg">{getLocalizedText(r.title)}</div>
          </div>
        ),
      },
      { key: 'slug', title: t('admin.posts.table.slug'), class: 'min-w-[220px] break-all font-mono text-xs text-muted', render: (r) => r.slug },
      {
        key: 'status',
        title: t('admin.posts.table.status'),
        render: (r) => (
          <Badge tone={r.is_published ? 'success' : 'warning'} dot>
            {r.is_published ? t('admin.posts.status.published') : t('admin.posts.status.draft')}
          </Badge>
        ),
      },
      { key: 'createdAt', title: t('admin.posts.table.createdAt'), class: 'whitespace-nowrap text-xs text-muted', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.posts.table.action'),
        align: 'right',
        render: (r) => (
          <div class="flex justify-end gap-2">
            <Button size="icon-sm" title={t('admin.common.edit')} onClick={() => p.openEdit(r)}>
              <Pencil class="h-4 w-4" />
            </Button>
            <Button size="icon-sm" variant="danger" title={t('admin.common.delete')} onClick={() => p.remove(r)}>
              <Trash2 class="h-4 w-4" />
            </Button>
          </div>
        ),
      },
    ]

    const productRow = (item: RelatedProductRef, action: () => unknown) => (
      <div key={item.id} class="flex items-center gap-3 px-3 py-2">
        {item.image && (
          <div class="h-10 w-10 shrink-0 overflow-hidden rounded-zs-sm border border-line bg-surface-muted">
            <img src={getImageUrl(item.image)} alt="" class="h-full w-full object-cover" />
          </div>
        )}
        <div class="min-w-0 flex-1">
          <div class="truncate text-sm font-medium text-fg">{getLocalizedText(item.title)}</div>
          <div class="truncate font-mono text-xs text-muted">/{item.slug}</div>
        </div>
        {action()}
      </div>
    )

    const renderRelated = () => (
      <div class="space-y-3 border-t border-line pt-4 md:col-span-2">
        <FormField label={t('admin.posts.form.relatedProducts')} hint={t('admin.posts.form.relatedProductsHint')}>
          <div class="space-y-3">
            {p.relatedProducts.value.length ? (
              <div class="divide-y divide-line rounded-zs border border-line bg-surface-muted">
                {p.relatedProducts.value.map((item) =>
                  productRow(item, () => (
                    <Button size="sm" onClick={() => p.removeRelated(item.id)}>
                      {t('admin.posts.form.relatedProductsRemove')}
                    </Button>
                  )),
                )}
              </div>
            ) : (
              <p class="text-sm text-muted">{t('admin.posts.form.relatedProductsEmpty')}</p>
            )}
            <Input icon={Search} v-model={p.productSearch.value} placeholder={t('admin.posts.form.relatedProductsSearch')} />
            {p.productSearch.value.trim() && (
              <div class="max-h-60 divide-y divide-line overflow-y-auto rounded-zs border border-line bg-surface-strong">
                {p.productSearchLoading.value ? (
                  <div class="px-3 py-3 text-center text-xs text-muted">...</div>
                ) : !p.productSearchResults.value.length ? (
                  <div class="px-3 py-3 text-center text-sm text-muted">{t('admin.posts.form.relatedProductsNoResults')}</div>
                ) : (
                  p.productSearchResults.value.map((item) =>
                    productRow(item, () =>
                      p.form.product_ids.includes(item.id) ? (
                        <Check class="h-4 w-4 text-success-text" />
                      ) : (
                        <Button size="sm" onClick={() => p.addRelated(item)}>
                          {t('admin.posts.form.relatedProductsAdd')}
                        </Button>
                      ),
                    ),
                  )
                )}
              </div>
            )}
          </div>
        </FormField>
      </div>
    )

    const renderForm = () => {
      const f = p.form
      return (
        <form
          class="grid grid-cols-1 gap-4 md:grid-cols-2 md:gap-5"
          onSubmit={(e: Event) => {
            e.preventDefault()
            p.submit()
          }}
        >
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.posts.form.title', { lang: '' }))} required error={p.errors.title}>
              <LocalizedInput v-model={f.title} placeholder={t('admin.posts.form.titlePlaceholder')} />
            </FormField>
          </div>
          <FormField label={stripLangLabel(t('admin.posts.form.slug'))} required error={p.errors.slug}>
            <Input v-model={f.slug} placeholder={t('admin.posts.form.slugPlaceholder')} mono />
          </FormField>
          <FormField label={t('admin.posts.form.type')} required error={p.errors.type}>
            <Select
              v-model={f.type}
              options={[
                { label: t('admin.posts.form.typeBlog'), value: 'blog' },
                { label: t('admin.posts.form.typeNotice'), value: 'notice' },
              ]}
            />
          </FormField>
          {f.type === 'blog' && (
            <FormField label={t('admin.posts.form.category')}>
              <Select
                modelValue={f.category_id ?? 0}
                options={p.categoryOptions.value}
                placeholder={t('admin.posts.form.categoryPlaceholder')}
                onUpdate:modelValue={(v) => (f.category_id = Number(v) || null)}
              />
            </FormField>
          )}
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.posts.form.summary', { lang: '' }))}>
              <LocalizedInput v-model={f.summary} mode="textarea" rows={2} placeholder={t('admin.posts.form.summaryPlaceholder')} />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={t('admin.posts.form.thumbnail')}>
              <MediaPicker v-model={f.thumbnail} scene="post" />
            </FormField>
          </div>
          <div class="md:col-span-2">
            <FormField label={stripLangLabel(t('admin.posts.form.content', { lang: '' }))}>
              <LocalizedInput
                v-model={f.content}
                mode="rich"
                editor={(v, update) => (
                  <RichEditor modelValue={v} onUpdate:modelValue={update} scene="post" placeholder={t('admin.posts.form.contentPlaceholder')} minHeight="280px" />
                )}
              />
            </FormField>
          </div>
          {f.type === 'blog' && renderRelated()}
          <div class="border-t border-line pt-4 md:col-span-2">
            <Switch v-model={f.is_published} label={t('admin.posts.form.publishNow')} />
          </div>
          {p.modal.error.value && <p class="text-xs text-danger-text md:col-span-2">{p.modal.error.value}</p>}
        </form>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.posts.title')}>
          {{
            actions: () => (
              <Button variant="primary" onClick={p.openCreate}>
                <Plus class="h-4 w-4" />
                {t('admin.posts.create')}
              </Button>
            ),
          }}
        </PageHeader>

        <Tabs
          modelValue={currentTab.value}
          onUpdate:modelValue={(v: string) => (currentTab.value = normalizePostType(v))}
          items={[
            { key: 'blog', label: t('admin.posts.tabs.blog'), icon: FileText },
            { key: 'notice', label: t('admin.posts.tabs.notice'), icon: Megaphone },
          ]}
        />

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.posts.empty')}
            minWidth="980px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <Dialog
          v-model={p.modal.showModal.value}
          title={p.modal.isEditing.value ? t('admin.posts.modal.editTitle') : t('admin.posts.modal.createTitle')}
          description={p.modal.isEditing.value ? t('admin.posts.modal.editDescription') : t('admin.posts.modal.createDescription')}
          size="3xl"
          closeOnOverlay={false}
        >
          {{
            default: renderForm,
            footer: () => (
              <>
                <Button onClick={p.modal.closeModal}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={p.modal.submitting.value} onClick={p.submit}>
                  {t(postSubmitLabelKey(p.modal.isEditing.value, p.form.is_published))}
                </Button>
              </>
            ),
          }}
        </Dialog>
      </div>
    )
  },
})
