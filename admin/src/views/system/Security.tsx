import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { KeyRound, RefreshCcw, ShieldCheck, ShieldOff, ShieldPlus } from 'lucide-vue-next'
import { Badge, Button, Card, Checkbox, FormField, Input, Loader, PageHeader } from '@/components/ui'
import { formatDate } from '@/utils/format'
import { useSecurity } from './security/useSecurity'
import { Setup2FAModal } from './security/Setup2FAModal'
import { RecoveryCodesModal } from './security/RecoveryCodesModal'

export default defineComponent({
  name: 'SecurityView',
  setup() {
    const { t } = useI18n()
    const s = useSecurity()
    onMounted(() => void s.refreshTOTPStatus())

    const passwordCard = () => (
      <Card>
        {{
          title: () => (
            <>
              <KeyRound class="h-4 w-4 text-primary" />
              {t('admin.settings.actions.changePassword')}
            </>
          ),
          default: () => (
            <form
              class="space-y-5"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void s.changePassword()
              }}
            >
              <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                <FormField label={t('admin.settings.security.currentPassword')} required>
                  <Input type="password" v-model={s.passwordForm.old} autocomplete="current-password" placeholder={t('admin.settings.security.currentPasswordPlaceholder')} />
                </FormField>
                <div class="hidden md:block" />
                <FormField label={t('admin.settings.security.newPassword')} required>
                  <Input type="password" v-model={s.passwordForm.new} autocomplete="new-password" placeholder={t('admin.settings.security.newPasswordPlaceholder')} />
                </FormField>
                <FormField label={t('admin.settings.security.confirmPassword')} required>
                  <Input type="password" v-model={s.passwordForm.confirm} autocomplete="new-password" placeholder={t('admin.settings.security.confirmPasswordPlaceholder')} />
                </FormField>
              </div>
              <div class="flex justify-end">
                <Button type="submit" variant="primary" loading={s.passwordSaving.value}>
                  {t('admin.settings.actions.changePassword')}
                </Button>
              </div>
            </form>
          ),
        }}
      </Card>
    )

    const enabledBody = () => {
      const st = s.totpStatus.value
      if (!st) return null
      return (
        <div class="space-y-5">
          <div class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
            <div class="rounded-zs border border-line bg-surface-muted/40 px-4 py-3">
              <p class="text-xs text-muted">{t('admin.twofa.enabledSince')}</p>
              <p class="mt-1 text-fg">{formatDate(st.enabled_at) || '-'}</p>
            </div>
            <div class="rounded-zs border border-line bg-surface-muted/40 px-4 py-3">
              <p class="text-xs text-muted">{t('admin.twofa.recoveryRemaining')}</p>
              <p class="zs-num mt-1 text-fg">
                {st.recovery_codes_remaining} / {st.recovery_codes_total}
              </p>
            </div>
          </div>

          <div class="space-y-3 rounded-zs border border-line p-4">
            <h4 class="flex items-center gap-2 text-sm font-semibold text-fg">
              <RefreshCcw class="h-4 w-4 text-accent" />
              {t('admin.twofa.regenerate.title')}
            </h4>
            <p class="text-xs text-muted">{t('admin.twofa.regenerate.hint')}</p>
            <div class="flex flex-col gap-2 sm:flex-row sm:items-end">
              <FormField label={t('admin.twofa.regenerate.codePlaceholder')} class="flex-1">
                <Input v-model={s.regenForm.code} maxlength={6} mono placeholder="123456" autocomplete="one-time-code" onEnter={s.submitRegen} />
              </FormField>
              <Button loading={s.regenerating.value} onClick={s.submitRegen}>
                {t('admin.twofa.regenerate.button')}
              </Button>
            </div>
          </div>

          <div class="space-y-3 rounded-zs border border-line bg-danger-soft/40 p-4">
            <h4 class="flex items-center gap-2 text-sm font-semibold text-danger-text">
              <ShieldOff class="h-4 w-4" />
              {t('admin.twofa.disable.title')}
            </h4>
            <p class="text-xs text-muted">{t('admin.twofa.disable.hint')}</p>
            <Checkbox v-model={s.disableForm.useRecovery} label={t('admin.twofa.disable.useRecovery')} />
            <div class="flex flex-col gap-2 sm:flex-row sm:items-end">
              <div class="flex-1">
                {s.disableForm.useRecovery ? (
                  <Input v-model={s.disableForm.recoveryCode} mono placeholder={t('admin.twofa.disable.recoveryPlaceholder')} onEnter={s.submitDisable} />
                ) : (
                  <Input
                    v-model={s.disableForm.code}
                    maxlength={6}
                    mono
                    autocomplete="one-time-code"
                    placeholder={t('admin.twofa.disable.codePlaceholder')}
                    onEnter={s.submitDisable}
                  />
                )}
              </div>
              <Button variant="danger" loading={s.disabling.value} onClick={s.submitDisable}>
                {t('admin.twofa.disable.button')}
              </Button>
            </div>
          </div>
        </div>
      )
    }

    const twofaCard = () => {
      const st = s.totpStatus.value
      return (
        <Card description={t('admin.twofa.description')}>
          {{
            title: () => (
              <>
                <ShieldCheck class="h-4 w-4 text-primary" />
                {t('admin.twofa.title')}
              </>
            ),
            extra: () =>
              st ? (
                <Badge tone={st.enabled ? 'success' : 'neutral'} dot>
                  {st.enabled ? t('admin.authz.adminTotpEnabled') : t('admin.authz.adminTotpDisabled')}
                </Badge>
              ) : null,
            default: () =>
              s.totpLoading.value && !st ? (
                <div class="flex items-center gap-2 text-sm text-muted">
                  <Loader />
                  {t('admin.common.loading')}
                </div>
              ) : st && !st.enabled ? (
                <Button variant="primary" onClick={() => (s.setupOpen.value = true)}>
                  <ShieldPlus class="h-4 w-4" />
                  {t('admin.twofa.enableButton')}
                </Button>
              ) : (
                enabledBody()
              ),
          }}
        </Card>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.settings.security.title')} subtitle={t('admin.settings.security.subtitle')} />
        {passwordCard()}
        {twofaCard()}
        <Setup2FAModal v-model={s.setupOpen.value} onEnabled={s.onEnabled} />
        <RecoveryCodesModal v-model={s.recoveryOpen.value} codes={s.recoveryCodes.value} />
      </div>
    )
  },
})
