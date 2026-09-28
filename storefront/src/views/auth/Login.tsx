import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { KeyRound, LogIn, Mail, Send, ShieldCheck, User } from 'lucide-vue-next'
import { AuthShell } from '@/components/auth/AuthShell'
import { GoogleIdentityButton } from '@/components/auth/GoogleIdentityButton'
import { PasswordInput } from '@/components/auth/PasswordInput'
import { TelegramLoginWidget } from '@/components/auth/TelegramLoginWidget'
import { CaptchaField } from '@/components/common/CaptchaField'
import { TELEGRAM_WIDGET_CALLBACK, useLogin } from '@/composables/useLogin'
import { useAppStore } from '@/stores/app'
import { Alert, Button, Checkbox, Field, Input } from '@/components/ui'

export default defineComponent({
  name: 'LoginView',
  setup() {
    const { t } = useI18n()
    const appStore = useAppStore()
    const s = useLogin()

    const passwordStep = () => (
      <form
        class="space-y-5"
        novalidate
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.handleLogin()
        }}
      >
        <Field label={t('auth.login.emailLabel')} error={s.validation.getError('email')} for="login-email">
          <Input
            id="login-email"
            type="email"
            autocomplete="email"
            v-model={s.email.value}
            placeholder={t('auth.login.emailPlaceholder')}
            invalid={s.validation.hasError('email')}
            onBlur={() => s.validation.touchField('email', s.email.value)}
          >
            {{ prefix: () => <Mail class="size-4" /> }}
          </Input>
        </Field>
        <Field label={t('auth.login.passwordLabel')} error={s.validation.getError('password')} for="login-password">
          {{
            label: () =>
              s.emailVerificationEnabled.value && (
                <RouterLink to="/auth/forgot" class="text-xs font-bold text-accent-text hover:underline">
                  {t('auth.login.forgot')}
                </RouterLink>
              ),
            default: () => (
              <PasswordInput
                id="login-password"
                v-model={s.password.value}
                placeholder={t('auth.login.passwordPlaceholder')}
                invalid={s.validation.hasError('password')}
                onBlur={() => s.validation.touchField('password', s.password.value)}
              />
            ),
          }}
        </Field>
        {s.captcha.enabled.value && (
          <Field label={t('auth.common.captchaLabel')}>
            <CaptchaField captcha={s.captcha} />
          </Field>
        )}
        <Checkbox v-model={s.rememberMe.value}>{t('auth.login.rememberMe')}</Checkbox>
        <Button type="submit" size="lg" block loading={s.auth.loading}>
          {!s.auth.loading && <LogIn class="size-4" />}
          {s.auth.loading ? t('auth.login.submitting') : t('auth.login.submit')}
        </Button>
      </form>
    )

    const totpStep = () => (
      <form
        class="space-y-5"
        novalidate
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.handleVerify2FA()
        }}
      >
        <div class="flex items-center gap-3 rounded-zs border border-line bg-surface-muted p-4">
          <span class="zs-gradient-bg flex size-10 shrink-0 items-center justify-center rounded-full text-on-primary">
            <ShieldCheck class="size-5" />
          </span>
          <div class="min-w-0">
            <div class="font-bold text-fg">{t('auth.login.totp.title')}</div>
            <p class="text-xs text-muted">{t('auth.login.totp.subtitle')}</p>
          </div>
        </div>
        {s.challengeRemainingSeconds.value > 0 && (
          <p class="text-center text-xs font-bold text-warning-text zs-num">
            {t('auth.login.totp.countdown', {
              seconds: s.challengeRemainingSeconds.value,
            })}
          </p>
        )}
        {s.totpMode.value === 'code' ? (
          <Field label={t('auth.login.totp.codeLabel')}>
            <Input
              v-model={s.totpCode.value}
              inputmode="numeric"
              autocomplete="one-time-code"
              maxlength={6}
              size="lg"
              inputClass="text-center tracking-[0.6em] zs-num text-lg"
              placeholder={t('auth.login.totp.codePlaceholder')}
            />
          </Field>
        ) : (
          <Field label={t('auth.login.totp.recoveryLabel')}>
            <Input v-model={s.recoveryCode.value} autocomplete="off" size="lg" inputClass="text-center zs-num" placeholder={t('auth.login.totp.recoveryPlaceholder')}>
              {{ prefix: () => <KeyRound class="size-4" /> }}
            </Input>
          </Field>
        )}
        <Button type="submit" size="lg" block loading={s.auth.loading}>
          {s.auth.loading ? t('auth.login.totp.verifying') : t('auth.login.totp.submit')}
        </Button>
        <div class="flex items-center justify-between text-sm">
          <button type="button" class="font-bold text-accent-text hover:underline" onClick={s.toggleTotpMode}>
            {s.totpMode.value === 'code' ? t('auth.login.totp.useRecovery') : t('auth.login.totp.useCode')}
          </button>
          <button type="button" class="text-muted hover:text-fg" onClick={s.cancel2FA}>
            {t('auth.login.totp.cancel')}
          </button>
        </div>
      </form>
    )

    const thirdParty = () =>
      s.showThirdPartyLogin.value && (
        <div class="mt-8 space-y-4">
          <div class="flex items-center gap-3 text-xs text-muted">
            <span class="zs-divider flex-1" />
            {t('auth.login.socialOr')}
            <span class="zs-divider flex-1" />
          </div>
          {s.showMiniAppLoginHint.value && (
            <Alert tone="info">{s.attemptingMiniAppLogin.value ? t('auth.login.telegramMiniAppLoggingIn') : t('auth.login.telegramMiniAppHint')}</Alert>
          )}
          {s.showTelegramWidget.value && (
            <div class="space-y-2 text-center">
              <TelegramLoginWidget botUsername={s.telegramBotUsername.value} callbackName={TELEGRAM_WIDGET_CALLBACK} onError={s.handleTelegramWidgetError} />
              <p class="text-xs text-muted">{t('auth.login.telegramHint')}</p>
            </div>
          )}
          {s.showTelegramOidc.value && (
            <div class="space-y-2 text-center">
              <Button variant="secondary" block onClick={() => void s.startTelegramOidc()}>
                <Send class="size-4 text-accent-text" />
                {t('auth.login.telegramOidcButton')}
              </Button>
              <p class="text-xs text-muted">{t('auth.login.telegramOidcHint')}</p>
            </div>
          )}
          {s.showGoogleLogin.value && (
            <div class="space-y-2 text-center">
              <GoogleIdentityButton
                clientId={s.googleClientId.value}
                locale={appStore.locale}
                disabled={s.auth.loading}
                loadingLabel={t('auth.login.googleLoading')}
                uxMode={s.googleUxMode}
                loginUri={s.googleRedirectLoginUri}
                prepareRedirect={s.prepareGoogleRedirectLogin}
                onCredential={(c: string) => void s.handleGoogleCredential(c)}
                onError={s.handleGoogleError}
              />
              <p class="text-xs text-muted">{t('auth.login.googleHint')}</p>
            </div>
          )}
          {s.showTelegramMiniAppEntry.value && (
            <div class="space-y-2 text-center">
              <p class="text-xs text-muted">{t('auth.login.telegramMiniAppEntryHint')}</p>
              <Button variant="ghost" size="sm" onClick={s.openTelegramMiniAppEntry}>
                {t('auth.login.telegramMiniAppEntryAction')}
              </Button>
            </div>
          )}
        </div>
      )

    return () => (
      <AuthShell title={t('auth.login.title')} subtitle={t('auth.login.subtitle')} mood={s.step.value === 'totp' ? 'surprised' : 'happy'}>
        {{
          topRight: () => (
            <RouterLink to="/me" class="inline-flex items-center gap-1 rounded-full border border-line px-3 py-1 text-xs text-muted hover:text-primary-text">
              <User class="size-3.5" />
              {t('navbar.personalCenter')}
            </RouterLink>
          ),
          default: () => (
            <>
              {s.info.value && (
                <div class="mb-5">
                  <Alert tone="success">{s.info.value}</Alert>
                </div>
              )}
              {s.error.value && (
                <div class="mb-5">
                  <Alert tone="error">{s.error.value}</Alert>
                </div>
              )}
              {s.step.value === 'totp' ? totpStep() : passwordStep()}
              {s.step.value === 'password' && thirdParty()}
              {s.step.value === 'password' && s.registrationEnabled.value && (
                <div class="mt-8 text-center text-sm">
                  <RouterLink to="/auth/register" class="font-bold text-primary-text hover:underline">
                    {t('auth.login.noAccount')}
                  </RouterLink>
                </div>
              )}
            </>
          ),
        }}
      </AuthShell>
    )
  },
})
