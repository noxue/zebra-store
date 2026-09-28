import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, Bell, Home, LayoutGrid, Newspaper } from 'lucide-vue-next'
import { useNotFound } from '@/composables/useNotFound'
import { Button, Card, Mascot } from '@/components/ui'

export default defineComponent({
  name: 'NotFoundView',
  setup() {
    const { t } = useI18n()
    const s = useNotFound()
    const links = [
      { to: '/products', label: () => t('nav.products'), icon: LayoutGrid },
      { to: '/blog', label: () => t('nav.blog'), icon: Newspaper },
      { to: '/notice', label: () => t('nav.notice'), icon: Bell },
    ]
    return () => (
      <div class="zs-page flex min-h-[70vh] items-center justify-center py-10">
        <Card padding="lg">
          <div class="grid items-center gap-8 md:grid-cols-[auto_1fr]">
            <div class="relative mx-auto h-60 w-48">
              <Mascot mood="sad" />
            </div>
            <div class="text-center md:text-left">
              <div class="zs-num zs-gradient-text text-7xl font-bold leading-none sm:text-8xl">{t('zsContent.notFoundCode')}</div>
              <h1 class="zs-title mt-3 text-3xl text-fg">{t('notFoundPage.title')}</h1>
              <p class="mt-3 max-w-md text-muted">{t('notFoundPage.description', { site: s.siteName() })}</p>
              <div class="mt-6 flex flex-wrap justify-center gap-3 md:justify-start">
                <Button to="/">
                  <Home class="size-4" />
                  {t('notFoundPage.backHome')}
                </Button>
                <Button variant="secondary" onClick={s.goBack}>
                  <ArrowLeft class="size-4" />
                  {t('notFoundPage.backPrevious')}
                </Button>
              </div>
              <div class="mt-8">
                <div class="mb-3 text-xs font-bold text-muted">{t('notFoundPage.quickLinksTitle')}</div>
                <div class="flex flex-wrap justify-center gap-2 md:justify-start">
                  {links.map((link) => {
                    const Icon = link.icon
                    return (
                      <RouterLink key={link.to} to={link.to} class="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface-strong px-3.5 py-1.5 text-sm text-fg transition hover:border-line-strong hover:text-primary-text">
                        <Icon class="size-4" />
                        {link.label()}
                      </RouterLink>
                    )
                  })}
                  <RouterLink to="/about" class="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface-strong px-3.5 py-1.5 text-sm text-fg transition hover:border-line-strong hover:text-primary-text">
                    {t('nav.about')}
                  </RouterLink>
                </div>
              </div>
            </div>
          </div>
        </Card>
      </div>
    )
  },
})
