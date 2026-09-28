import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, MessageCircle, Send, Sparkles } from 'lucide-vue-next'
import { useAbout } from '@/composables/useAbout'
import { Card, Mascot } from '@/components/ui'

export default defineComponent({
  name: 'AboutView',
  setup() {
    const { t } = useI18n()
    const s = useAbout()
    const heading = (text: string) => (
      <h2 class="zs-title mb-5 flex items-center gap-3 text-2xl text-fg">
        <span class="zs-gradient-bg h-7 w-1.5 rounded-full" />
        {text}
      </h2>
    )
    return () => (
      <div class="zs-page max-w-5xl! pb-8">
        <section class="relative grid items-center gap-6 py-10 sm:py-14 md:grid-cols-[1.4fr_1fr]">
          <div class="text-center md:text-left">
            <span class="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface px-3 py-1 text-xs font-bold text-primary-text">
              <Sparkles class="size-3.5" />
              {t('nav.about')}
            </span>
            <h1 class="zs-title mt-4 text-4xl leading-tight sm:text-5xl">
              <span class="zs-gradient-text">{s.heroTitle.value}</span>
            </h1>
            {s.heroSubtitle.value && <p class="mt-4 text-lg leading-relaxed text-muted">{s.heroSubtitle.value}</p>}
          </div>
          <div class="mx-auto h-64 w-52 sm:h-72 sm:w-60">
            <Mascot mood="wink" />
          </div>
        </section>
        {(s.hasIntroduction.value || s.hasServices.value || s.hasContact.value) && (
          <Card padding="lg">
            <div class="space-y-12">
              {s.hasIntroduction.value && <p class="whitespace-pre-line text-lg leading-relaxed text-muted">{s.introductionText.value}</p>}
              {s.hasServices.value && (
                <div>
                  {heading(s.servicesTitle.value || t('about.ourServices'))}
                  <div class="grid gap-4 md:grid-cols-2">
                    {s.serviceItems.value.map((item, i) => (
                      <div key={i} class="flex items-start gap-3 rounded-zs border border-line bg-surface-muted p-4">
                        <span class="zs-gradient-bg mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-full text-on-primary">
                          <Check class="size-3.5" stroke-width={3} />
                        </span>
                        <span class="text-fg">{item}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {s.hasContact.value && (
                <div>
                  {(s.contactTitle.value || s.hasContactLinks.value) && heading(s.contactTitle.value || t('about.contactUs'))}
                  {s.contactText.value && <p class="mb-6 whitespace-pre-line text-muted">{s.contactText.value}</p>}
                  {s.hasContactLinks.value && (
                    <div class="grid gap-4 sm:grid-cols-2">
                      {s.contactConfig.value.telegram && (
                        <a href={s.contactConfig.value.telegram} target="_blank" rel="noopener noreferrer" class="zs-card-hover flex items-center justify-center gap-3 rounded-zs border border-line bg-surface-strong px-6 py-4 font-bold text-fg">
                          <span class="flex size-9 items-center justify-center rounded-full bg-accent-soft text-accent-text">
                            <Send class="size-4" />
                          </span>
                          Telegram
                        </a>
                      )}
                      {s.contactConfig.value.whatsapp && (
                        <a href={s.contactConfig.value.whatsapp} target="_blank" rel="noopener noreferrer" class="zs-card-hover flex items-center justify-center gap-3 rounded-zs border border-line bg-surface-strong px-6 py-4 font-bold text-fg">
                          <span class="flex size-9 items-center justify-center rounded-full bg-success-soft text-success-text">
                            <MessageCircle class="size-4" />
                          </span>
                          WhatsApp
                        </a>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>
          </Card>
        )}
      </div>
    )
  },
})
