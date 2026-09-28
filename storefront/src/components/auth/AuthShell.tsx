import { defineComponent, type PropType } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft } from 'lucide-vue-next'
import { useAppStore } from '@/stores/app'
import { Mascot, type MascotMood } from '@/components/ui'

/**
 * Split auth layout: form card on the left, mascot panel (theme.login_background
 * or gradient) on the right; the panel is hidden below lg.
 */
export const AuthShell = defineComponent({
  name: 'AuthShell',
  props: {
    title: { type: String, required: true },
    subtitle: { type: String, default: '' },
    panelTitle: { type: String, default: '' },
    panelSubtitle: { type: String, default: '' },
    mood: { type: String as PropType<MascotMood>, default: 'happy' },
  },
  setup(props, { slots }) {
    const { t } = useI18n()
    const appStore = useAppStore()
    return () => (
      <div class="zs-page flex min-h-[calc(100vh-5rem)] items-center py-8 sm:py-12">
        <div class="zs-card grid w-full overflow-hidden p-0 lg:grid-cols-[1fr_1.05fr]">
          <div class="relative p-6 sm:p-10">
            <div class="mb-6 flex items-center justify-between">
              <RouterLink to="/" class="inline-flex items-center gap-1.5 text-sm text-muted transition hover:text-primary-text">
                <ArrowLeft class="size-4" />
                {t('auth.login.backHome')}
              </RouterLink>
              {slots.topRight?.()}
            </div>
            <div class="mx-auto max-w-md">
              <div class="mb-8 text-center">
                <div class="mb-2 text-xs font-bold uppercase tracking-[0.3em] text-primary-text">{appStore.siteName}</div>
                <h1 class="zs-title text-3xl sm:text-4xl">
                  <span class="zs-gradient-text">{props.title}</span>
                  <span class="zs-sparkle ml-2 text-lg" aria-hidden="true">
                    ✦
                  </span>
                </h1>
                {props.subtitle && <p class="mt-2 text-sm text-muted">{props.subtitle}</p>}
              </div>
              {slots.default?.()}
            </div>
          </div>
          <aside
            class="relative hidden min-h-[560px] flex-col items-center justify-center overflow-hidden p-10 text-center lg:flex zs-soft-bg"
            style={appStore.loginBackground ? { backgroundImage: `url(${appStore.loginBackground})`, backgroundSize: 'cover', backgroundPosition: 'center' } : undefined}
          >
            <div class="pointer-events-none absolute -left-16 -top-16 size-64 rounded-full bg-primary/20 blur-3xl" />
            <div class="pointer-events-none absolute -bottom-20 -right-10 size-72 rounded-full bg-accent/20 blur-3xl" />
            <div class="relative h-72 w-60">
              <Mascot mood={props.mood} />
            </div>
            <div class="relative mt-6 rounded-zs-lg border border-line bg-surface-strong px-6 py-4 shadow-zs backdrop-blur">
              <div class="zs-title text-xl text-fg">{props.panelTitle || t('zsContent.authPanelTitle')}</div>
              <p class="mt-1 text-sm text-muted">{props.panelSubtitle || t('zsContent.authPanelSubtitle')}</p>
            </div>
          </aside>
        </div>
      </div>
    )
  },
})
