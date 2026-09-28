import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search, X } from 'lucide-vue-next'
import type { PostType } from '@/api/types'
import { usePostList } from '@/composables/usePostList'
import { Button, EmptyState, Input, Pagination } from '@/components/ui'
import { PageHero } from './PageHero'
import { NoticeRow, PostCard } from './PostCard'

/** Shared Blog / Notice list page. Blog shows a search box. */
export const PostListPage = defineComponent({
  name: 'PostListPage',
  props: { type: { type: String as PropType<PostType>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const s = usePostList(props.type)
    const isBlog = props.type === 'blog'
    return () => (
      <div class="zs-page pb-8">
        <PageHero title={isBlog ? t('nav.blog') : t('nav.notice')} subtitle={isBlog ? t('blog.subtitle') : t('notice.subtitle')} />
        {isBlog && (
          <div class="mx-auto mb-10 max-w-xl">
            <Input v-model={s.searchKeyword.value} size="lg" placeholder={t('blog.searchPlaceholder')} class="[&_input]:rounded-full">
              {{
                prefix: () => <Search class="size-4" />,
                suffix: () =>
                  s.hasKeyword.value ? (
                    <button
                      type="button"
                      aria-label={t('blog.searchClear')}
                      class="flex size-8 items-center justify-center rounded-full hover:bg-primary-soft"
                      onClick={s.clearSearch}
                    >
                      <X class="size-4" />
                    </button>
                  ) : null,
              }}
            </Input>
          </div>
        )}
        {s.loading.value ? (
          <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} class="zs-card space-y-4 p-6">
                <div class="zs-skeleton h-5 w-20" />
                <div class="zs-skeleton h-6 w-3/4" />
                <div class="zs-skeleton h-4 w-full" />
                <div class="zs-skeleton h-4 w-2/3" />
              </div>
            ))}
          </div>
        ) : s.error.value ? (
          <EmptyState variant="error">
            <Button variant="secondary" onClick={() => void s.reload()}>
              {t('emptyState.retry')}
            </Button>
          </EmptyState>
        ) : s.posts.value.length === 0 ? (
          s.hasKeyword.value ? (
            <EmptyState variant="search" title={t('blog.noResults')}>
              <Button variant="secondary" onClick={s.clearSearch}>
                {t('blog.searchClear')}
              </Button>
            </EmptyState>
          ) : (
            <EmptyState title={isBlog ? t('blog.empty') : t('notice.empty')} />
          )
        ) : (
          <>
            {isBlog ? (
              <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
                {s.posts.value.map((post) => (
                  <PostCard key={post.id} post={post} date={s.formatDate(post.published_at)} />
                ))}
              </div>
            ) : (
              <div class="mx-auto max-w-4xl space-y-4">
                {s.posts.value.map((post) => (
                  <NoticeRow key={post.id} post={post} date={s.formatDate(post.published_at)} />
                ))}
              </div>
            )}
            <div class="mt-10">
              <Pagination page={s.currentPage.value} totalPages={s.totalPages.value} onChange={s.changePage} />
            </div>
          </>
        )}
      </div>
    )
  },
})
