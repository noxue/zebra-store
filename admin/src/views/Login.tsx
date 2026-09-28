import { defineComponent, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { KeyRound, Lock, Moon, Sun, User } from 'lucide-vue-next'
import { Button, FormField, Input, Mascot, Select } from '@/components/ui'
import { SakuraCanvas } from '@/components/SakuraCanvas'
import { ImageCaptcha } from '@/components/captcha/ImageCaptcha'
import { TurnstileCaptcha } from '@/components/captcha/TurnstileCaptcha'
import { useLogin } from '@/composables/useLogin'
import { setLocale, type AppLocale, localeSelectOptions } from '@/i18n'

export default defineComponent({
  name: 'LoginView',
  setup() {
    const { t, locale } = useI18n()
    const s = useLogin()
    const imageRef = ref<{ refresh: () => void } | null>(null)
    const turnstileRef = ref<{ reset: () => void } | null>(null)
    onMounted(() => void s.app.loadConfig())

    const resetCaptcha = () => {
      imageRef.value?.refresh()
      turnstileRef.value?.reset()
    }

    const renderPasswordStep = () => (
      <form
        class="space-y-4"
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.submitPassword(resetCaptcha)
        }}
      >
        <FormField label={t('admin.login.username')}>
          <Input icon={User} v-model={s.username.value} placeholder={t('admin.login.username')} autocomplete="username" />
        </FormField>
        <FormField label={t('admin.login.password')}>
          <Input icon={Lock} type="password" v-model={s.password.value} placeholder={t('admin.login.password')} autocomplete="current-password" />
        </FormField>
        {s.captchaProvider.value === 'image' && (
          <FormField label={t('admin.login.captchaLabel')}>
            <ImageCaptcha ref={imageRef} v-model={s.imageCaptcha.value} />
          </FormField>
        )}
        {s.captchaProvider.value === 'turnstile' && s.turnstileSiteKey.value && (
          <TurnstileCaptcha ref={turnstileRef} siteKey={s.turnstileSiteKey.value} v-model={s.turnstileToken.value} />
        )}
        {s.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-xs text-danger-text">{s.error.value}</p>}
        <Button type="submit" variant="primary" size="lg" block loading={s.auth.loading}>
          {s.auth.loading ? t('admin.login.submitting') : t('admin.login.submit')}
        </Button>
      </form>
    )

    const renderTotpStep = () => (
      <form
        class="space-y-4"
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.submitTotp()
        }}
      >
        <div class="rounded-zs bg-secondary-soft px-4 py-3 text-xs text-secondary">
          {t('admin.login.totp.remaining', { seconds: s.remaining.value })}
        </div>
        {s.useRecovery.value ? (
          <FormField label={t('admin.login.totp.recoveryLabel')}>
            <Input icon={KeyRound} v-model={s.recoveryCode.value} mono placeholder="xxxx-xxxx" />
          </FormField>
        ) : (
          <FormField label={t('admin.login.totp.codeLabel')}>
            <Input
              v-model={s.totpCode.value}
              maxlength={6}
              autocomplete="one-time-code"
              placeholder="000000"
              inputClass="zs-num text-center text-xl tracking-[0.6em]"
            />
          </FormField>
        )}
        {s.error.value && <p class="rounded-zs-sm bg-danger-soft px-3 py-2 text-xs text-danger-text">{s.error.value}</p>}
        <Button type="submit" variant="primary" size="lg" block loading={s.auth.loading}>
          {t('admin.login.totp.submit')}
        </Button>
        <div class="flex items-center justify-between text-xs">
          <button type="button" class="text-accent hover:underline" onClick={() => (s.useRecovery.value = !s.useRecovery.value)}>
            {s.useRecovery.value ? t('admin.login.totp.useCode') : t('admin.login.totp.useRecovery')}
          </button>
          <button type="button" class="text-muted hover:text-primary" onClick={s.backToPassword}>
            {t('admin.login.totp.back')}
          </button>
        </div>
      </form>
    )

    return () => {
      const bg = s.app.loginBackground
      const totp = s.auth.requiresTotp
      return (
        <div class="relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-10">
          {bg && <img src={bg} alt="" class="absolute inset-0 h-full w-full object-cover opacity-40" />}
          <div class="pointer-events-none absolute -left-24 top-10 h-72 w-72 rounded-full bg-primary/25 blur-3xl" />
          <div class="pointer-events-none absolute -right-20 bottom-10 h-80 w-80 rounded-full bg-secondary/25 blur-3xl" />
          <div class="pointer-events-none absolute left-1/2 top-1/3 h-56 w-56 rounded-full bg-accent/20 blur-3xl" />
          {s.app.sakuraEnabled && <SakuraCanvas />}

          <div class="absolute right-4 top-4 flex items-center gap-2">
            <div class="w-28">
              <Select
                size="sm"
                modelValue={locale.value}
                options={localeSelectOptions()}
                onUpdate:modelValue={(v) => setLocale(String(v) as AppLocale)}
              />
            </div>
            <Button size="icon-sm" onClick={() => s.app.toggleTheme()}>
              {s.app.theme === 'dark' ? <Sun class="h-3.5 w-3.5" /> : <Moon class="h-3.5 w-3.5" />}
            </Button>
          </div>

          <div class="zs-pop-in relative grid w-full max-w-4xl overflow-hidden rounded-zs-lg border border-line bg-surface shadow-zs backdrop-blur-xl md:grid-cols-[1fr_1.05fr]">
            <div class="zs-gradient-soft-bg relative hidden flex-col items-center justify-center gap-4 p-10 text-center md:flex">
              {s.app.mascotImage ? (
                <img src={s.app.mascotImage} alt="mascot" class="zs-float max-h-72 object-contain drop-shadow-xl" />
              ) : (
                <Mascot size={220} mood={totp ? 'wink' : 'happy'} float />
              )}
              <div>
                <p class="zs-display zs-gradient-text text-2xl">{t('admin.zebra.welcomeBack')}</p>
                <p class="mt-2 text-sm text-muted">{t('admin.zebra.loginSlogan')}</p>
              </div>
            </div>
            <div class="bg-surface-solid/80 p-8 sm:p-10">
              <div class="mb-6 flex items-center gap-3">
                <img src={s.app.siteLogo || '/favicon.svg'} alt="" class="h-11 w-11 rounded-[14px] object-cover shadow-zs-sm" />
                <div class="min-w-0">
                  <p class="zs-display truncate text-sm text-muted">{s.app.siteName || t('admin.brand')}</p>
                  <h1 class="zs-display flex items-center gap-2 text-2xl text-fg">
                    {totp ? t('admin.login.totp.title') : t('admin.login.title')}
                    <span class="zs-sparkle text-base">✦</span>
                  </h1>
                </div>
              </div>
              <p class="mb-6 text-sm text-muted">{totp ? t('admin.login.totp.subtitle') : t('admin.login.subtitle')}</p>
              {totp ? renderTotpStep() : renderPasswordStep()}
              <p class="mt-8 text-center text-[11px] text-muted">
                © {new Date().getFullYear()} {s.app.siteName || 'Zebra Store'}
                {s.app.appVersion && ` · ${s.app.appVersion}`}
              </p>
            </div>
          </div>
        </div>
      )
    }
  },
})
