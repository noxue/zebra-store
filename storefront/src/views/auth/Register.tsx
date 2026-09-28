import { defineComponent } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Ban, Mail, UserPlus } from 'lucide-vue-next'
import { AuthShell } from '@/components/auth/AuthShell'
import { PasswordInput } from '@/components/auth/PasswordInput'
import { PasswordStrengthMeter } from '@/components/auth/PasswordStrengthMeter'
import { SendCodeInput } from '@/components/auth/SendCodeInput'
import { CaptchaField } from '@/components/common/CaptchaField'
import { useRegister } from '@/composables/useRegister'
import { Alert, Button, Checkbox, Field, Input, Select } from '@/components/ui'

export default defineComponent({
  name: 'RegisterView',
  setup() {
    const { t } = useI18n()
    const s = useRegister()

    const emailField = () =>
      s.emailDomainSelectionRequired.value ? (
        <Field label={t('auth.register.emailLabel')} error={s.validation.getError('email')} hint={t('auth.register.emailDomainSelectHint')}>
          <div class="flex items-center gap-2">
            <Input class="flex-1" v-model={s.emailLocalPart.value} placeholder={t('auth.register.emailLocalPlaceholder')} autocomplete="username" onBlur={s.touchEmail}>
              {{ prefix: () => <Mail class="size-4" /> }}
            </Input>
            <span class="font-bold text-muted">@</span>
            <div class="w-40">
              <Select
                v-model={s.selectedEmailDomain.value}
                options={s.allowedEmailDomains.value.map((d) => ({
                  label: d,
                  value: d,
                }))}
              />
            </div>
          </div>
        </Field>
      ) : (
        <Field
          label={t('auth.register.emailLabel')}
          error={s.validation.getError('email')}
          hint={
            s.emailDomainAllowlistEnabled.value
              ? s.allowedEmailDomains.value.length
                ? t('auth.register.allowedEmailDomainsHint', {
                    domains: s.allowedEmailDomainsText.value,
                  })
                : t('auth.register.noAllowedEmailDomainsHint')
              : ''
          }
        >
          <Input
            type="email"
            autocomplete="email"
            v-model={s.email.value}
            placeholder={t('auth.register.emailPlaceholder')}
            invalid={s.validation.hasError('email')}
            onBlur={s.touchEmail}
          >
            {{ prefix: () => <Mail class="size-4" /> }}
          </Input>
        </Field>
      )

    const form = () => (
      <form
        class="space-y-5"
        novalidate
        onSubmit={(e: Event) => {
          e.preventDefault()
          void s.handleRegister()
        }}
      >
        {emailField()}
        <Field label={t('auth.register.passwordLabel')} error={s.validation.getError('password')}>
          <PasswordInput
            v-model={s.password.value}
            autocomplete="new-password"
            placeholder={t('auth.register.passwordPlaceholder')}
            invalid={s.validation.hasError('password')}
            onBlur={s.touchPassword}
          />
          {s.password.value && <PasswordStrengthMeter strength={s.passwordStrength.value} />}
        </Field>
        {s.emailVerificationEnabled.value && s.captcha.enabled.value && (
          <Field label={t('auth.common.captchaLabel')}>
            <CaptchaField captcha={s.captcha} />
          </Field>
        )}
        {s.emailVerificationEnabled.value && (
          <Field label={t('auth.register.codeLabel')}>
            <SendCodeInput
              v-model={s.code.value}
              placeholder={t('auth.register.codePlaceholder')}
              countdown={s.countdown.value}
              sending={s.sending.value}
              onSend={() => void s.handleSendCode()}
            />
          </Field>
        )}
        <Checkbox v-model={s.agreed.value}>
          <span class="text-muted">
            {t('auth.register.agreementPrefix')}{' '}
            <RouterLink to="/privacy" target="_blank" class="font-bold text-accent-text hover:underline">
              {t('footer.privacy')}
            </RouterLink>{' '}
            {t('auth.register.agreementAnd')}{' '}
            <RouterLink to="/terms" target="_blank" class="font-bold text-accent-text hover:underline">
              {t('footer.terms')}
            </RouterLink>
          </span>
        </Checkbox>
        <Button type="submit" size="lg" block loading={s.auth.loading} disabled={!s.agreed.value}>
          {!s.auth.loading && <UserPlus class="size-4" />}
          {s.auth.loading ? t('auth.register.creating') : t('auth.register.create')}
        </Button>
      </form>
    )

    return () => (
      <AuthShell
        title={t('auth.register.title')}
        subtitle={t('auth.register.subtitle')}
        panelTitle={t('zsContent.registerPanelTitle')}
        panelSubtitle={t('zsContent.registerPanelSubtitle')}
        mood="wink"
      >
        {{
          // Same corner badge as the original register card.
          topRight: () => (
            <span class="inline-flex items-center gap-1 rounded-full border border-line px-3 py-1 text-xs text-muted">
              <UserPlus class="size-3.5" />
              {t('auth.register.title')}
            </span>
          ),
          default: () =>
            !s.registrationEnabled.value ? (
              <div class="space-y-4 py-8 text-center">
                <Ban class="mx-auto size-10 text-muted" />
                <p class="text-muted">{t('auth.register.registrationDisabled')}</p>
                <Button variant="secondary" to="/auth/login">
                  {t('auth.register.hasAccount')}
                </Button>
              </div>
            ) : (
              <>
                {s.error.value && (
                  <div class="mb-5">
                    <Alert tone="error">{s.error.value}</Alert>
                  </div>
                )}
                {form()}
                <div class="mt-8 text-center text-sm">
                  <RouterLink to="/auth/login" class="font-bold text-primary-text hover:underline">
                    {t('auth.register.hasAccount')}
                  </RouterLink>
                </div>
              </>
            ),
        }}
      </AuthShell>
    )
  },
})
