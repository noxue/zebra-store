import { defineComponent, nextTick, ref, watch, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { Chrome, LockKeyhole, Mail, Send, ShieldCheck } from 'lucide-vue-next'
import { Badge, Button, Card, Field, Input } from '@/components/ui'
import { useSecurityPanel } from '@/composables/personal/useSecurityPanel'
import { useAppStore } from '@/stores/app'
import { GoogleBindButton } from './GoogleBindButton'
import { LoginHistorySection } from './LoginHistorySection'
import { PanelAlertBox, PanelHeading } from './PanelParts'
import { TwoFactorSection } from './TwoFactorSection'

/** Security center: bindings, email, password, 2FA, login history. */
export const SecurityPanel = defineComponent({
  name: 'SecurityPanel',
  setup() {
    const { t } = useI18n()
    const appStore = useAppStore()
    const s = useSecurityPanel()
    const widgetEl = ref<HTMLElement | null>(null)

    watch(
      () => [s.showTelegramWidget.value, widgetEl.value] as const,
      () => void nextTick(() => s.mountTelegramWidget(widgetEl.value)),
      { immediate: true },
    )

    const bindingCard = (opts: {
      title: string
      subtitle: string
      bound: boolean
      boundLabel: string
      unboundLabel: string
      icon: typeof Send
      body: () => VNodeChild
    }) => {
      const Icon = opts.icon
      return (
        <div class="rounded-zs-lg border border-line bg-surface-strong p-5">
          <div class="flex items-start justify-between gap-3">
            <div class="flex items-start gap-3">
              <span class="flex size-10 shrink-0 items-center justify-center rounded-zs-sm bg-accent-soft text-accent-text">
                <Icon class="size-5" />
              </span>
              <div>
                <h3 class="font-bold text-fg">{opts.title}</h3>
                <p class="mt-0.5 text-xs text-muted">{opts.subtitle}</p>
              </div>
            </div>
            <Badge tone={opts.bound ? 'success' : 'warning'}>{opts.bound ? opts.boundLabel : opts.unboundLabel}</Badge>
          </div>
          <div class="mt-4 text-sm">{opts.body()}</div>
        </div>
      )
    }

    const telegramBody = () => {
      const b = s.store.telegramBinding
      if (s.store.loadingTelegramBinding) return <p class="text-muted">{t('personalCenter.security.telegramLoading')}</p>
      if (s.telegramBound.value && b) {
        return (
          <div class="space-y-3">
            <div class="flex items-center gap-3">
              {b.avatar_url && <img src={b.avatar_url} alt="" class="size-9 rounded-full" />}
              <div>
                <div class="font-bold text-fg">{s.telegramDisplayName.value}</div>
                <div class="text-xs text-muted">{t('personalCenter.security.telegramBindID', { id: b.provider_user_id || '-' })}</div>
              </div>
            </div>
            <div class="text-xs text-muted">{t('personalCenter.security.telegramBindTime', { time: s.bindingTime(b.auth_at) })}</div>
            <Button size="sm" variant="secondary" disabled={!s.canUnbindTelegram.value} loading={s.store.unbindingTelegram} onClick={() => void s.unbindTelegram()}>
              {s.store.unbindingTelegram ? t('personalCenter.security.telegramUnbinding') : t('personalCenter.security.telegramUnbind')}
            </Button>
            {!s.canUnbindTelegram.value && <p class="text-xs text-muted">{t('personalCenter.security.telegramUnbindDisabledTip')}</p>}
          </div>
        )
      }
      if (!s.telegramEnabled.value) return <p class="text-xs text-muted">{t('personalCenter.security.telegramDisabledTip')}</p>
      return (
        <div class="space-y-3">
          <p class="text-xs text-muted">{t('personalCenter.security.telegramUnboundTip')}</p>
          {s.showTelegramWidget.value && <div ref={widgetEl} />}
          {s.showTelegramOidc.value && (
            <div class="space-y-1">
              <Button size="sm" onClick={() => void s.startTelegramOidcBind()}>
                <Send class="size-4" />
                {t('personalCenter.security.telegramOidcBindButton')}
              </Button>
              <p class="text-xs text-muted">{t('personalCenter.security.telegramOidcBindHint')}</p>
            </div>
          )}
        </div>
      )
    }

    const googleBody = () => {
      const b = s.store.googleBinding
      if (s.store.loadingGoogleBinding) return <p class="text-muted">{t('personalCenter.security.googleLoading')}</p>
      if (s.googleBound.value && b) {
        return (
          <div class="space-y-3">
            <div class="flex items-center gap-3">
              {b.avatar_url && <img src={b.avatar_url} alt={t('personalCenter.security.googleAvatarAlt')} class="size-9 rounded-full" />}
              <div>
                <div class="font-bold text-fg">{s.googleDisplayName.value}</div>
                <div class="text-xs text-muted">{t('personalCenter.security.googleBindID', { id: b.provider_user_id || '-' })}</div>
              </div>
            </div>
            <div class="text-xs text-muted">{t('personalCenter.security.googleBindTime', { time: s.bindingTime(b.auth_at) })}</div>
            <Button size="sm" variant="secondary" disabled={!s.canUnbindGoogle.value} loading={s.store.unbindingGoogle} onClick={() => void s.unbindGoogle()}>
              {s.store.unbindingGoogle ? t('personalCenter.security.googleUnbinding') : t('personalCenter.security.googleUnbind')}
            </Button>
            {!s.canUnbindGoogle.value && <p class="text-xs text-muted">{t('personalCenter.security.googleUnbindDisabledTip')}</p>}
          </div>
        )
      }
      if (!s.googleEnabled.value) return <p class="text-xs text-muted">{t('personalCenter.security.googleDisabledTip')}</p>
      return (
        <div class="space-y-3">
          <p class="text-xs text-muted">{t('personalCenter.security.googleUnboundTip')}</p>
          {s.store.bindingGoogle ? (
            <p class="text-xs text-muted">{t('personalCenter.security.googleBinding')}</p>
          ) : (
            <GoogleBindButton
              clientId={s.googleClientId.value}
              locale={appStore.locale}
              onCredential={(c: string) => void s.handleGoogleCredential(c)}
              onScriptError={s.handleGoogleScriptError}
            />
          )}
        </div>
      )
    }

    const cooldownLabel = (seconds: number, label: string) => (seconds > 0 ? t('personalCenter.security.countdown', { seconds }) : label)

    return () => {
      const needOld = s.requiresOldEmailCode.value
      const needOldPwd = s.requiresOldPassword.value
      return (
        <div class="space-y-5">
          <Card>
            <PanelHeading
              title={t('personalCenter.security.title')}
              description={needOld ? t('personalCenter.security.subtitle') : t('personalCenter.security.subtitleBindOnly')}
              icon={ShieldCheck}
            >
              {{ actions: () => <Badge tone="accent">{t('personalCenter.tabs.security')}</Badge> }}
            </PanelHeading>
            <PanelAlertBox alert={s.alert.value} />
            <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
              {bindingCard({
                title: t('personalCenter.security.telegramTitle'),
                subtitle: s.telegramEnabled.value ? t('personalCenter.security.telegramSubtitle') : t('personalCenter.security.telegramDisabledTip'),
                bound: s.telegramBound.value,
                boundLabel: t('personalCenter.security.telegramBound'),
                unboundLabel: t('personalCenter.security.telegramUnbound'),
                icon: Send,
                body: telegramBody,
              })}
              {bindingCard({
                title: t('personalCenter.security.googleTitle'),
                subtitle: s.googleEnabled.value ? t('personalCenter.security.googleSubtitle') : t('personalCenter.security.googleDisabledTip'),
                bound: s.googleBound.value,
                boundLabel: t('personalCenter.security.googleBound'),
                unboundLabel: t('personalCenter.security.googleUnbound'),
                icon: Chrome,
                body: googleBody,
              })}
            </div>

            <div class="zs-divider my-6" />

            <h3 class="zs-title mb-4 flex items-center gap-2 text-lg text-fg">
              <Mail class="size-5 text-primary-text" />
              {needOld ? t('personalCenter.security.submit') : t('personalCenter.security.bindSubmit')}
            </h3>
            {!needOld && <p class="mb-4 text-sm text-warning-text">{t('personalCenter.security.bindOnlyTip')}</p>}
            <form
              class="space-y-4"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void s.submitEmail()
              }}
            >
              <Field label={t('personalCenter.security.currentEmailLabel')}>
                <Input modelValue={s.currentEmailDisplay.value} readonly disabled />
              </Field>
              <Field label={t('personalCenter.security.newEmailLabel')}>
                <Input v-model={s.emailForm.newEmail} type="email" placeholder={t('personalCenter.security.newEmailPlaceholder')} autocomplete="email" />
              </Field>
              <div class={['grid grid-cols-1 gap-4', needOld && 'md:grid-cols-2']}>
                {needOld && (
                  <Field label={t('personalCenter.security.oldCodeLabel')}>
                    <div class="flex gap-2">
                      <Input class="flex-1" v-model={s.emailForm.oldCode} placeholder={t('personalCenter.security.codePlaceholder')} inputmode="numeric" />
                      <Button variant="secondary" disabled={s.oldCooldown.value > 0 || s.store.sendingCode} onClick={() => void s.sendOldCode()}>
                        {cooldownLabel(s.oldCooldown.value, t('personalCenter.security.sendOldCode'))}
                      </Button>
                    </div>
                  </Field>
                )}
                <Field label={t('personalCenter.security.newCodeLabel')}>
                  <div class="flex gap-2">
                    <Input class="flex-1" v-model={s.emailForm.newCode} placeholder={t('personalCenter.security.codePlaceholder')} inputmode="numeric" />
                    <Button variant="secondary" disabled={s.newCooldown.value > 0 || s.store.sendingCode} onClick={() => void s.sendNewCode()}>
                      {cooldownLabel(s.newCooldown.value, t('personalCenter.security.sendNewCode'))}
                    </Button>
                  </div>
                </Field>
              </div>
              <Button type="submit" loading={s.store.changingEmail}>
                {s.store.changingEmail
                  ? t('personalCenter.security.submitting')
                  : needOld
                    ? t('personalCenter.security.submit')
                    : t('personalCenter.security.bindSubmit')}
              </Button>
            </form>
          </Card>

          <LoginHistorySection />

          {needOld && (
            <Card>
              <PanelHeading
                title={needOldPwd ? t('personalCenter.security.passwordTitle') : t('personalCenter.security.setPasswordTitle')}
                description={needOldPwd ? t('personalCenter.security.passwordSubtitle') : t('personalCenter.security.setPasswordSubtitle')}
                icon={LockKeyhole}
              />
              <form
                class="space-y-4"
                onSubmit={(e: Event) => {
                  e.preventDefault()
                  void s.submitPassword()
                }}
              >
                {needOldPwd && (
                  <Field label={t('personalCenter.security.currentPasswordLabel')}>
                    <Input v-model={s.passwordForm.oldPassword} type="password" autocomplete="current-password" placeholder={t('personalCenter.security.passwordPlaceholder')} />
                  </Field>
                )}
                <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                  <Field label={t('personalCenter.security.newPasswordLabel')}>
                    <Input v-model={s.passwordForm.newPassword} type="password" autocomplete="new-password" placeholder={t('personalCenter.security.passwordPlaceholder')} />
                  </Field>
                  <Field label={t('personalCenter.security.confirmPasswordLabel')}>
                    <Input v-model={s.passwordForm.confirmPassword} type="password" autocomplete="new-password" placeholder={t('personalCenter.security.passwordPlaceholder')} />
                  </Field>
                </div>
                <Button type="submit" variant="secondary" loading={s.store.changingPassword}>
                  {needOldPwd
                    ? s.store.changingPassword
                      ? t('personalCenter.security.changePasswordSubmitting')
                      : t('personalCenter.security.changePassword')
                    : s.store.changingPassword
                      ? t('personalCenter.security.setPasswordSubmitting')
                      : t('personalCenter.security.setPassword')}
                </Button>
              </form>
            </Card>
          )}

          <TwoFactorSection />
        </div>
      )
    }
  },
})
