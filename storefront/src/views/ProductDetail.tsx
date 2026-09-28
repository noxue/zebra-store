import { defineComponent, onBeforeUnmount, ref, watch } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, LogIn, ShieldCheck, ShoppingCart, Sparkles } from 'lucide-vue-next'
import { RichContent } from '@/components/common/RichContent'
import { ProductBadges } from '@/components/product/ProductBadges'
import { ProductImageGallery } from '@/components/product/ProductImageGallery'
import { ProductMobileBar } from '@/components/product/ProductMobileBar'
import { PriceBlock, RulePanels, SkuSelector } from '@/components/product/PurchaseParts'
import { useLocalized } from '@/composables/useLocalized'
import { useProductDetail } from '@/composables/useProductDetail'
import { Alert, Breadcrumb, Button, Card, EmptyState, QuantityStepper, SmartImage } from '@/components/ui'
import { hasHtmlContent } from '@/utils/content'

export default defineComponent({
  name: 'ProductDetail',
  setup() {
    const { t } = useI18n()
    const { getLocalizedText } = useLocalized()
    const pd = useProductDetail()

    // Mobile purchase bar appears once the action buttons scroll out of view.
    const actionsEl = ref<HTMLElement | null>(null)
    const showMobileBar = ref(false)
    let observer: IntersectionObserver | null = null
    watch(actionsEl, (el) => {
      observer?.disconnect()
      if (!el || typeof IntersectionObserver === 'undefined') return
      observer = new IntersectionObserver(([entry]) => {
        showMobileBar.value = !entry.isIntersecting && entry.boundingClientRect.top < 0
      })
      observer.observe(el)
    })
    onBeforeUnmount(() => observer?.disconnect())

    const skeleton = () => (
      <div class="grid gap-8 lg:grid-cols-2">
        <div class="zs-skeleton aspect-[4/3] rounded-zs-lg" />
        <div class="space-y-4">
          <div class="zs-skeleton h-4 w-32" />
          <div class="zs-skeleton h-10 w-3/4" />
          <div class="zs-skeleton h-24 w-full" />
          <div class="zs-skeleton h-32 w-full" />
        </div>
      </div>
    )

    return () => {
      const p = pd.product.value
      if (pd.loading.value) return <div class="zs-page py-8">{skeleton()}</div>
      if (!p) {
        return (
          <div class="zs-page py-12">
            <Card>
              <EmptyState variant="error" title={t('productDetail.notFound')}>
                <Button variant="secondary" onClick={() => void pd.loadProduct()}>
                  {t('emptyState.retry')}
                </Button>
                <Button to="/products">{t('productDetail.backToProducts')}</Button>
              </EmptyState>
            </Card>
          </div>
        )
      }
      return (
        <div class="zs-page space-y-8 py-6">
          <Breadcrumb items={pd.breadcrumbs.value} />
          <Card padding="none" class="overflow-hidden">
            <div class="grid lg:grid-cols-2">
              <div class="zs-soft-bg p-5 sm:p-8">
                <ProductImageGallery
                  images={pd.images.value}
                  current={pd.currentImage.value}
                  alt={pd.title.value}
                  onUpdate:current={(v: string) => {
                    pd.currentImage.value = v
                  }}
                />
                <div class="mt-5 hidden items-center gap-2 rounded-zs border border-line bg-surface-strong px-4 py-3 text-xs text-muted lg:flex">
                  <ShieldCheck class="size-4 shrink-0 text-success-text" />
                  {t('productDetail.deliveryReassurance')}
                </div>
              </div>
              <div class="space-y-6 p-5 sm:p-8">
                <div class="space-y-3">
                  {pd.categoryName.value && (
                    <div class="text-xs font-bold tracking-wider text-secondary-text">
                      {t('productDetail.categoryLabel')} · {pd.categoryName.value}
                    </div>
                  )}
                  {p.tags && p.tags.length > 0 && (
                    <div class="flex flex-wrap gap-1.5">
                      {p.tags.map((tag) => (
                        <span key={tag} class="rounded-full border border-line bg-surface-muted px-2.5 py-0.5 text-xs font-bold text-primary-text">
                          ✦ {tag}
                        </span>
                      ))}
                    </div>
                  )}
                  <h1 class="zs-title text-3xl leading-tight text-fg sm:text-4xl">{pd.title.value}</h1>
                  <ProductBadges product={p} size="sm" />
                </div>
                <PriceBlock engine={pd} />
                <RulePanels engine={pd} />
                <SkuSelector engine={pd} />
                {pd.description.value && (
                  <div>
                    <div class="mb-1.5 text-sm font-bold text-fg">{t('productDetail.description')}</div>
                    <p class="whitespace-pre-line text-sm leading-relaxed text-muted">{pd.description.value}</p>
                  </div>
                )}
                <div>
                  <div class="mb-2 text-sm font-bold text-fg">{t('productDetail.quantity')}</div>
                  <QuantityStepper
                    modelValue={pd.quantity.value}
                    min={pd.quantityMin.value}
                    max={pd.quantityLimit.value}
                    disabled={!pd.canPurchase.value}
                    onUpdate:modelValue={(v: number) => {
                      pd.quantity.value = v
                    }}
                    onLimit={pd.onQuantityLimit}
                  />
                </div>
                {pd.purchaseWarning.value && <Alert tone="warning">{pd.purchaseWarning.value}</Alert>}
                {pd.cannotPurchaseReason.value && !pd.purchaseWarning.value && <Alert tone="error">{pd.cannotPurchaseReason.value}</Alert>}
                <div ref={actionsEl} class="grid grid-cols-2 gap-3">
                  {pd.requiresLogin.value ? (
                    <Button size="lg" block class="col-span-2" onClick={pd.goLogin}>
                      <LogIn class="size-4" />
                      {t('productDetail.loginToBuy')}
                    </Button>
                  ) : (
                    <>
                      <Button size="lg" variant="secondary" block disabled={!pd.canPurchase.value} onClick={() => pd.addToCart()}>
                        <ShoppingCart class="size-4" />
                        {t('productDetail.addToCart')}
                      </Button>
                      <Button size="lg" block disabled={!pd.canPurchase.value} onClick={() => pd.buyNow()}>
                        <Sparkles class="size-4" />
                        {t('productDetail.buyNow')}
                      </Button>
                    </>
                  )}
                </div>
              </div>
            </div>
          </Card>

          {hasHtmlContent(pd.content.value) && (
            <Card padding="lg">
              <h2 class="zs-title mb-5 flex items-center gap-3 border-b border-line pb-4 text-2xl">
                <span class="zs-gradient-bg h-7 w-1.5 rounded-full" />
                {t('productDetail.details')}
              </h2>
              <RichContent html={pd.content.value} />
            </Card>
          )}

          {pd.relatedPosts.value.length > 0 && (
            <Card padding="lg">
              <h2 class="zs-title mb-5 text-2xl">{t('productDetail.relatedPosts')}</h2>
              <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                {pd.relatedPosts.value.map((post) => (
                  <RouterLink key={post.id} to={`/blog/${post.slug}`} class="zs-card-hover flex gap-3 rounded-zs border border-line bg-surface-strong p-3">
                    {post.thumbnail && (
                      <div class="size-16 shrink-0 overflow-hidden rounded-zs-sm">
                        <SmartImage src={post.thumbnail} />
                      </div>
                    )}
                    <div class="min-w-0">
                      <div class="text-xs text-muted">{pd.formatPostDate(post.published_at)}</div>
                      <div class="line-clamp-2 font-bold text-fg">{getLocalizedText(post.title)}</div>
                    </div>
                  </RouterLink>
                ))}
              </div>
            </Card>
          )}

          <div class="flex justify-center">
            <Button variant="ghost" to="/products">
              <ArrowLeft class="size-4" />
              {t('productDetail.backToProducts')}
            </Button>
          </div>
          <ProductMobileBar engine={pd} visible={showMobileBar.value} />
        </div>
      )
    }
  },
})
