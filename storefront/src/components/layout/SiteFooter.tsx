import { computed, defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Mail, MessageCircle, Send } from 'lucide-vue-next'
import { useLocalized } from '@/composables/useLocalized'
import { useNavConfig } from '@/composables/useNavConfig'
import { useAppStore } from '@/stores/app'
import { WaveDivider } from '@/components/ui'
import { SiteLogo } from './SiteLogo'

export const SiteFooter = defineComponent({
  name: 'SiteFooter',
  setup() {
    const { t } = useI18n()
    const appStore = useAppStore()
    const { getLocalizedText } = useLocalized()
    const { primaryNavItems } = useNavConfig()
    const description = computed(() => getLocalizedText(appStore.config?.brand?.site_description) || t('footer.description'))
    const footerLinks = computed(() =>
      (appStore.config?.footer_links || [])
        .map((link) => ({ label: getLocalizedText(link.name || link.title), url: link.url }))
        .filter((link) => link.label && link.url),
    )
    const contacts = computed(() => {
      const c = appStore.config?.contact || {}
      const s = appStore.config?.support || {}
      const rows: Array<{ key: string; label: string; href: string; icon: typeof Send }> = []
      const tg = c.telegram || s.telegram
      const wa = c.whatsapp || s.whatsapp
      if (tg) rows.push({ key: 'tg', label: 'Telegram', href: tg, icon: Send })
      if (wa) rows.push({ key: 'wa', label: 'WhatsApp', href: wa, icon: MessageCircle })
      if (s.email) rows.push({ key: 'mail', label: s.email, href: `mailto:${s.email}`, icon: Mail })
      return rows
    })
    const year = new Date().getFullYear()
    return () => (
      <footer class="relative mt-16">
        <WaveDivider />
        <div class="zs-glass border-x-0 border-b-0">
          <div class="zs-page grid gap-10 py-12 md:grid-cols-[1.4fr_1fr_1fr]">
            <div class="space-y-4">
              <SiteLogo />
              <p class="max-w-md text-sm leading-relaxed text-muted">{description.value}</p>
            </div>
            <div>
              <h4 class="zs-title mb-4 text-base text-fg">{t('footer.quickLinks')}</h4>
              <ul class="grid grid-cols-2 gap-2.5 text-sm">
                {primaryNavItems.value.map((item) => {
                  const Icon = item.icon
                  return (
                    <li key={item.key}>
                      {item.type === 'link' ? (
                        <a href={item.path} target={item.target} class="inline-flex items-center gap-2 text-muted transition hover:text-primary-text">
                          <Icon class="size-4" />
                          {item.label}
                        </a>
                      ) : (
                        <RouterLink to={item.path} class="inline-flex items-center gap-2 text-muted transition hover:text-primary-text">
                          <Icon class="size-4" />
                          {item.label}
                        </RouterLink>
                      )}
                    </li>
                  )
                })}
                {footerLinks.value.map((link) => (
                  <li key={link.url}>
                    <a href={link.url} target="_blank" rel="noopener noreferrer" class="text-muted transition hover:text-primary-text">
                      {link.label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
            {contacts.value.length > 0 && (
              <div>
                <h4 class="zs-title mb-4 text-base text-fg">{t('footer.contact')}</h4>
                <div class="space-y-3">
                  {contacts.value.map((row) => {
                    const Icon = row.icon
                    return (
                      <a
                        key={row.key}
                        href={row.href}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="zs-card-hover flex items-center gap-3 rounded-zs border border-line bg-surface-strong px-4 py-3 text-sm font-bold text-fg"
                      >
                        <span class="zs-gradient-bg flex size-8 items-center justify-center rounded-full text-on-primary">
                          <Icon class="size-4" />
                        </span>
                        {row.label}
                      </a>
                    )
                  })}
                </div>
              </div>
            )}
          </div>
          <div class="zs-page flex flex-col gap-3 border-t border-line py-6 text-xs text-muted sm:flex-row sm:items-center sm:justify-between">
            <div>
              © {year} {appStore.siteName}. {t('footer.rights')}
            </div>
            <div class="flex items-center gap-4">
              <RouterLink to="/privacy" class="hover:text-primary-text">
                {t('footer.privacy')}
              </RouterLink>
              <RouterLink to="/terms" class="hover:text-primary-text">
                {t('footer.terms')}
              </RouterLink>
            </div>
          </div>
        </div>
      </footer>
    )
  },
})
