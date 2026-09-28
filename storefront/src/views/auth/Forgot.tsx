import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { KeySquare, Mail } from 'lucide-vue-next'
import { AuthShell } from '@/components/auth/AuthShell'
import { PasswordInput } from '@/components/auth/PasswordInput'
import { SendCodeInput } from '@/components/auth/SendCodeInput'
import { CaptchaField } from '@/components/common/CaptchaField'
import { useForgot } from '@/composables/useForgot'
import { Alert, Button, Field, Input } from '@/components/ui'

export default defineComponent({
  name: 'ForgotView',
  setup() {
    const { t } = useI18n()
    const s = useForgot()
    return () => (
      <AuthShell
        title={t('auth.forgot.title')}
        subtitle={t('auth.forgot.subtitle')}
        panelTitle={t('zsContent.forgotPanelTitle')}
        panelSubtitle={t('zsContent.forgotPanelSubtitle')}
        mood={s.emailVerificationEnabled.value ? 'happy' : 'sad'}
      >
        {!s.emailVerificationEnabled.value ? (
          <div class="space-y-5 py-4 text-center">
            <Alert tone="warning">{t('auth.forgot.disabled')}</Alert>
            <Button variant="secondary" to="/auth/login">
              {t('auth.forgot.backLogin')}
            </Button>
          </div>
        ) : (
          <>
            {s.error.value && (
              <div class="mb-5">
                <Alert tone="error">{s.error.value}</Alert>
              </div>
            )}
            <form
              class="space-y-5"
              novalidate
              onSubmit={(e: Event) => {
                e.preventDefault()
                void s.handleReset()
              }}
            >
              <Field label={t('auth.forgot.emailLabel')}>
                <Input type="email" autocomplete="email" v-model={s.email.value} placeholder={t('auth.forgot.emailPlaceholder')}>
                  {{ prefix: () => <Mail class="size-4" /> }}
                </Input>
              </Field>
              {s.captcha.enabled.value && (
                <Field label={t('auth.common.captchaLabel')}>
                  <CaptchaField captcha={s.captcha} />
                </Field>
              )}
              <Field label={t('auth.forgot.codeLabel')}>
                <SendCodeInput
                  v-model={s.code.value}
                  placeholder={t('auth.forgot.codePlaceholder')}
                  countdown={s.countdown.value}
                  sending={s.sending.value}
                  onSend={() => void s.handleSendCode()}
                />
              </Field>
              <Field label={t('auth.forgot.newPasswordLabel')}>
                <PasswordInput v-model={s.newPassword.value} autocomplete="new-password" placeholder={t('auth.forgot.newPasswordPlaceholder')} />
              </Field>
              <Button type="submit" size="lg" block loading={s.auth.loading}>
                {!s.auth.loading && <KeySquare class="size-4" />}
                {s.auth.loading ? t('auth.forgot.submitting') : t('auth.forgot.submit')}
              </Button>
            </form>
            <div class="mt-8 text-center text-sm">
              <RouterLink to="/auth/login" class="font-bold text-primary-text hover:underline">
                {t('auth.forgot.backLogin')}
              </RouterLink>
            </div>
          </>
        )}
      </AuthShell>
    )
  },
})
