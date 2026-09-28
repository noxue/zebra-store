import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowRight, Bell, Newspaper } from 'lucide-vue-next'
import { HomeHero } from '@/components/home/HomeHero'
import { AnnouncementModal } from '@/components/product/AnnouncementModal'
import { CategorySidebar } from '@/components/product/CategorySidebar'
import { ProductGrid } from '@/components/product/ProductGrid'
import { ProductListItem } from '@/components/product/ProductListItem'
import { ProductQuickBuy } from '@/components/product/ProductQuickBuy'
import { useHome } from '@/composables/useHome'
import type { Product } from '@/api/types'
import { Button, EmptyState, Pagination, SectionTitle, SmartImage } from '@/components/ui'

export default defineComponent({
  name: 'Home',
  setup() {
    const { t } = useI18n()
    const h = useHome()

    const cardMode = () => (
      <>
        <section class="space-y-6">
          <SectionTitle title={t('home.featured.title')} subtitle={t('home.featured.description')}>
            {{
              actions: () => (
                <RouterLink to="/products" class="inline-flex items-center gap-1 text-sm font-bold text-primary-text hover:underline">
                  {t('home.featured.viewAll')}
                  <ArrowRight class="size-4" />
                </RouterLink>
              ),
            }}
          </SectionTitle>
          {!h.featuredLoading.value && h.featured.value.length === 0 ? (
            <div class="zs-card">
              <EmptyState title={t('home.featured.empty')} />
            </div>
          ) : (
            <ProductGrid products={h.featured.value} loading={h.featuredLoading.value} onOpen={h.goToProduct} onQuickBuy={h.quickBuy.show} />
          )}
        </section>

        {h.latestVisible.value && h.posts.value.length > 0 && (
          <section class="space-y-6">
            <SectionTitle title={t('home.latest.title')} subtitle={t('home.latest.description')}>
              {{
                actions: () => (
                  <div class="flex gap-2">
                    {h.blogEnabled.value && (
                      <Button size="sm" variant="soft" to="/blog">
                        <Newspaper class="size-4" />
                        {t('nav.blog')}
                      </Button>
                    )}
                    {h.noticeEnabled.value && (
                      <Button size="sm" variant="soft" to="/notice">
                        <Bell class="size-4" />
                        {t('nav.notice')}
                      </Button>
                    )}
                  </div>
                ),
              }}
            </SectionTitle>
            <div class="grid gap-4 md:grid-cols-3">
              {h.posts.value.map((post) => (
                <RouterLink key={post.id} to={`/blog/${post.slug}`} class="zs-card zs-card-hover group flex flex-col overflow-hidden p-0">
                  {post.thumbnail && (
                    <div class="aspect-[16/9] overflow-hidden">
                      <SmartImage src={post.thumbnail} alt={h.getLocalizedText(post.title)} />
                    </div>
                  )}
                  <div class="flex flex-1 flex-col gap-2 p-5">
                    <div class="flex items-center gap-2 text-xs text-muted">
                      <span class="rounded-full bg-secondary-soft px-2 py-0.5 font-bold text-secondary-text">{post.type === 'notice' ? t('nav.notice') : t('nav.blog')}</span>
                      <span class="zs-num">{h.formatDate(post.published_at)}</span>
                    </div>
                    <h3 class="line-clamp-2 font-bold text-fg group-hover:text-primary-text">{h.getLocalizedText(post.title)}</h3>
                    <p class="line-clamp-2 text-sm text-muted">{h.getLocalizedText(post.summary)}</p>
                    <span class="mt-auto inline-flex items-center gap-1 pt-2 text-sm font-bold text-accent-text">
                      {t('blog.readMore')}
                      <ArrowRight class="size-4 transition group-hover:translate-x-1" />
                    </span>
                  </div>
                </RouterLink>
              ))}
            </div>
          </section>
        )}
      </>
    )

    const listMode = () => {
      const l = h.list
      return (
        <section class="flex flex-col gap-6 lg:flex-row">
          <CategorySidebar
            categories={l.categoryGroups.value}
            selectedCategory={l.selectedCategory.value}
            expandedParentIds={l.expandedParentIds.value}
            showDrawer={l.showFilterDrawer.value}
            showSearch
            searchQuery={l.searchQuery.value}
            onSelectCategory={(id: number | null, close?: boolean) => l.selectCategory(id, close)}
            onToggleParent={l.toggleParentCategory}
            onUpdate:showDrawer={(v: boolean) => {
              l.showFilterDrawer.value = v
            }}
            onUpdate:searchQuery={(v: string) => {
              l.searchQuery.value = v
            }}
            onClearSearch={l.clearSearch}
          />
          <div class="min-w-0 flex-1 space-y-6">
            {l.loading.value ? (
              <div class="space-y-3">
                {Array.from({ length: 6 }).map((_, i) => (
                  <div key={i} class="zs-skeleton h-20 rounded-zs" />
                ))}
              </div>
            ) : h.groups.value.length === 0 ? (
              <div class="zs-card">
                <EmptyState variant={l.hasFilters.value ? 'search' : 'empty'} title={l.hasFilters.value ? t('products.emptyFiltered') : t('products.empty')}>
                  {l.hasFilters.value && (
                    <Button variant="secondary" onClick={l.clearFilters}>
                      {t('products.clearFilters')}
                    </Button>
                  )}
                </EmptyState>
              </div>
            ) : (
              h.groups.value.map((group) => (
                <div key={String(group.categoryId)} class="zs-card space-y-3 p-4 sm:p-5">
                  <h3 class="zs-title flex items-center gap-2 text-lg">
                    <span class="zs-sparkle text-sm">✦</span>
                    {group.categoryName}
                    <span class="zs-num text-xs text-muted">({group.products.length})</span>
                  </h3>
                  <div class="space-y-2">
                    {group.products.map((p, i) => (
                      <ProductListItem key={p.id} product={p} index={i} onClick={h.goToProduct} onQuickBuy={(x: Product) => h.quickBuy.show(x)} />
                    ))}
                  </div>
                </div>
              ))
            )}
            <Pagination page={l.currentPage.value} totalPages={l.totalPages.value} onChange={l.changePage} />
          </div>
        </section>
      )
    }

    return () => (
      <div class="zs-page space-y-14 py-6">
        <HomeHero carousel={h.banner} />
        {h.isListMode.value ? listMode() : cardMode()}
        <ProductQuickBuy product={h.quickBuy.product.value} open={h.quickBuy.open.value} onClose={h.quickBuy.close} />
        {h.announcement.value && (
          <AnnouncementModal
            announcement={h.announcement.value}
            open={h.announcementOpen.value}
            onClose={() => {
              h.announcementOpen.value = false
            }}
          />
        )}
      </div>
    )
  },
})
