import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, CalendarDays } from 'lucide-vue-next'
import { RelatedProductCard } from '@/components/content/RelatedProductCard'
import { RichContent } from '@/components/common/RichContent'
import { useBlogDetail } from '@/composables/useBlogDetail'
import { Badge, Breadcrumb, Button, Card, EmptyState, SectionTitle, SmartImage } from '@/components/ui'

export default defineComponent({
  name: 'BlogDetailView',
  setup() {
    const { t } = useI18n()
    const s = useBlogDetail()
    return () => (
      <div class="zs-page max-w-4xl! py-8">
        {s.loading.value ? (
          <Card padding="lg">
            <div class="space-y-4">
              <div class="zs-skeleton h-4 w-40" />
              <div class="zs-skeleton h-10 w-3/4" />
              <div class="zs-skeleton h-4 w-full" />
              <div class="zs-skeleton h-4 w-5/6" />
              <div class="zs-skeleton h-4 w-2/3" />
            </div>
          </Card>
        ) : !s.post.value ? (
          <Card padding="lg">
            <EmptyState variant="error" title={t('blogDetail.notFound')}>
              <Button variant="secondary" to="/blog">
                <ArrowLeft class="size-4" />
                {t('blogDetail.backToBlog')}
              </Button>
            </EmptyState>
          </Card>
        ) : (
          <>
            <div class="mb-6">
              <Breadcrumb items={[{ label: t('nav.home'), to: '/' }, { label: s.sectionLabel.value, to: s.backLink.value }, { label: s.title.value }]} />
            </div>
            <article class="zs-card overflow-hidden">
              {s.post.value.thumbnail && (
                <div class="aspect-[21/9] overflow-hidden">
                  <SmartImage src={s.post.value.thumbnail} alt={s.title.value} eager />
                </div>
              )}
              <div class="p-6 sm:p-10">
                <div class="mb-4 flex flex-wrap items-center gap-3">
                  <Badge tone={s.isNotice.value ? 'warning' : 'info'}>{s.sectionLabel.value}</Badge>
                  {s.publishedAt.value && (
                    <span class="inline-flex items-center gap-1.5 text-sm text-muted zs-num">
                      <CalendarDays class="size-4" />
                      {s.publishedAt.value}
                    </span>
                  )}
                </div>
                <h1 class="zs-title text-3xl leading-tight text-fg sm:text-4xl">{s.title.value}</h1>
                {s.summary.value && <p class="mt-4 rounded-zs border-l-4 border-primary bg-primary-soft px-4 py-3 text-sm text-muted">{s.summary.value}</p>}
                <div class="zs-divider my-8" />
                <RichContent html={s.content.value} />
              </div>
            </article>
            {s.relatedProducts.value.length > 0 && (
              <section class="mt-10">
                <SectionTitle size="md" title={t('blog.relatedProducts')} subtitle={t('zsContent.relatedProductsHint')} />
                <div class="mt-5 grid gap-4 sm:grid-cols-2">
                  {s.relatedProducts.value.map((product) => (
                    <RelatedProductCard key={product.id} product={product} />
                  ))}
                </div>
              </section>
            )}
            <div class="mt-10 flex justify-center">
              <RouterLink to={s.backLink.value} class="inline-flex items-center gap-2 text-muted transition hover:text-primary-text">
                <ArrowLeft class="size-4" />
                {s.backText.value}
              </RouterLink>
            </div>
          </>
        )}
      </div>
    )
  },
})
