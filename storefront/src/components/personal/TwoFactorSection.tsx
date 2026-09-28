import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { KeyRound, ShieldCheck } from 'lucide-vue-next'
import { CopyButton } from '@/components/common/CopyButton'
import { QrCode } from '@/components/common/QrCode'
import { Badge, Button, Card, Field, Input, Modal, Tabs } from '@/components/ui'
import { useTwoFactor } from '@/composables/personal/useTwoFactor'
import { formatDateTime } from '@/utils/format'
import { PanelAlertBox, PanelHeading } from './PanelParts'

/** TOTP two-factor setup / disable / recovery codes. */
export const TwoFactorSection = defineComponent({
  name: 'TwoFactorSection',
  setup() {
    const { t } = useI18n()
    const f = useTwoFactor()

    const codeInput = (model: { value: string }, placeholder: string) => (
      <Input
        modelValue={model.value}
        placeholder={placeholder}
        inputmode="numeric"
        maxlength={6}
        autocomplete="one-time-code"
        inputClass="zs-num tracking-[0.4em] placeholder:tracking-normal"
        onUpdate:modelValue={(v: string) => {
          model.value = v.replace(/\D/g, '').slice(0, 6)
        }}
      />
    )

    const setupView = () => {
      const s = f.setup.value
      if (!s) return null
      return (
        <div class="zs-soft-bg space-y-4 rounded-zs-lg border border-line p-5">
          <p class="text-sm font-bold text-fg">{t('personalCenter.security.twofa.setupStep1')}</p>
          <div class="flex flex-col items-center gap-5 sm:flex-row sm:items-start">
            <QrCode value={s.otpauth_url || ''} size={180} />
            <div class="min-w-0 flex-1 space-y-2">
              <div class="text-xs font-bold text-muted">{t('personalCenter.security.twofa.secret')}</div>
              <div class="flex flex-wrap items-center gap-2">
                <code class="zs-num break-all rounded-zs-sm bg-surface-strong px-3 py-2 text-sm font-bold tracking-wider text-fg">{s.secret}</code>
                <CopyButton value={s.secret} />
              </div>
            </div>
          </div>
          <p class="text-sm font-bold text-fg">{t('personalCenter.security.twofa.setupStep2')}</p>
          <div class="flex flex-col gap-3 sm:flex-row sm:items-end">
            <div class="sm:w-60">
              <Field label={t('personalCenter.security.twofa.codeLabel')}>{codeInput(f.enableCode, t('personalCenter.security.twofa.codePlaceholder'))}</Field>
            </div>
            <div class="flex gap-2">
              <Button loading={f.loading.value} onClick={() => void f.submitEnable()}>
                {f.loading.value ? t('personalCenter.security.twofa.enableSubmitting') : t('personalCenter.security.twofa.enableSubmit')}
              </Button>
              <Button variant="secondary" onClick={f.cancelSetup}>
                {t('personalCenter.security.twofa.cancel')}
              </Button>
            </div>
          </div>
        </div>
      )
    }

    const enabledView = () => {
      const s = f.status.value
      if (!s) return null
      return (
        <div class="space-y-4">
          <div class="flex flex-wrap gap-x-6 gap-y-1 text-sm text-muted">
            {s.enabled_at && <span>{t('personalCenter.security.twofa.enabledAt', { date: formatDateTime(s.enabled_at) })}</span>}
            <span>{t('personalCenter.security.twofa.recoveryRemaining', { remaining: s.recovery_codes_remaining ?? 0, total: s.recovery_codes_total ?? 0 })}</span>
          </div>
          {f.mode.value === 'idle' && (
            <div class="flex flex-wrap gap-2">
              <Button variant="secondary" onClick={() => f.openMode('regenerate')}>
                {t('personalCenter.security.twofa.regenerateAction')}
              </Button>
              <Button variant="danger" onClick={() => f.openMode('disable')}>
                {t('personalCenter.security.twofa.disableAction')}
              </Button>
            </div>
          )}
          {f.mode.value === 'disable' && (
            <div class="space-y-3 rounded-zs border border-danger/35 bg-danger-soft p-4">
              <p class="text-sm text-fg">{t('personalCenter.security.twofa.disableHint')}</p>
              <Tabs
                size="sm"
                modelValue={f.disableWith.value}
                items={[
                  { key: 'code', label: t('personalCenter.security.twofa.useCode') },
                  { key: 'recovery', label: t('personalCenter.security.twofa.useRecovery') },
                ]}
                onUpdate:modelValue={(v: string) => {
                  f.disableWith.value = v === 'recovery' ? 'recovery' : 'code'
                }}
              />
              <div class="sm:w-72">
                {f.disableWith.value === 'code' ? (
                  codeInput(f.disableCode, t('personalCenter.security.twofa.codePlaceholder'))
                ) : (
                  <Input v-model={f.disableRecovery.value} placeholder={t('personalCenter.security.twofa.recoveryPlaceholder')} inputClass="zs-num" />
                )}
              </div>
              <div class="flex gap-2">
                <Button variant="danger" loading={f.loading.value} onClick={() => void f.submitDisable()}>
                  {f.loading.value ? t('personalCenter.security.twofa.disableSubmitting') : t('personalCenter.security.twofa.disableSubmit')}
                </Button>
                <Button variant="secondary" onClick={() => f.openMode('idle')}>
                  {t('personalCenter.security.twofa.cancel')}
                </Button>
              </div>
            </div>
          )}
          {f.mode.value === 'regenerate' && (
            <div class="zs-soft-bg space-y-3 rounded-zs border border-line p-4">
              <p class="text-sm text-fg">{t('personalCenter.security.twofa.regenerateHint')}</p>
              <div class="sm:w-60">{codeInput(f.regenerateCode, t('personalCenter.security.twofa.codePlaceholder'))}</div>
              <div class="flex gap-2">
                <Button loading={f.loading.value} onClick={() => void f.submitRegenerate()}>
                  {f.loading.value ? t('personalCenter.security.twofa.regenerateSubmitting') : t('personalCenter.security.twofa.regenerateSubmit')}
                </Button>
                <Button variant="secondary" onClick={() => f.openMode('idle')}>
                  {t('personalCenter.security.twofa.cancel')}
                </Button>
              </div>
            </div>
          )}
        </div>
      )
    }

    return () => {
      const enabled = !!f.status.value?.enabled
      return (
        <Card>
          <PanelHeading title={t('personalCenter.security.twofa.title')} description={t('personalCenter.security.twofa.subtitle')} icon={ShieldCheck}>
            {{
              actions: () => (
                <Badge tone={enabled ? 'success' : 'neutral'}>
                  {enabled ? t('personalCenter.security.twofa.statusEnabled') : t('personalCenter.security.twofa.statusDisabled')}
                </Badge>
              ),
            }}
          </PanelHeading>
          <PanelAlertBox alert={f.alert.value} />
          {enabled ? (
            enabledView()
          ) : f.setup.value ? (
            setupView()
          ) : (
            <div class="space-y-4">
              <p class="text-sm text-muted">{t('personalCenter.security.twofa.notEnabledHint')}</p>
              <Button loading={f.loading.value} onClick={() => void f.startSetup()}>
                <KeyRound class="size-4" />
                {f.loading.value ? t('personalCenter.security.twofa.startingSetup') : t('personalCenter.security.twofa.startSetup')}
              </Button>
            </div>
          )}
          <Modal open={f.recoveryCodes.value.length > 0} closable={false} title={t('personalCenter.security.twofa.recoveryTitle')} size="md" onClose={f.acknowledgeRecovery}>
            {{
              default: () => (
                <div class="space-y-4">
                  <p class="text-sm text-warning-text">{t('personalCenter.security.twofa.recoveryWarning')}</p>
                  <div class="grid grid-cols-2 gap-2">
                    {f.recoveryCodes.value.map((code) => (
                      <code key={code} class="zs-num rounded-zs-sm border border-line bg-surface-muted px-3 py-2 text-center text-sm font-bold text-fg">
                        {code}
                      </code>
                    ))}
                  </div>
                </div>
              ),
              footer: () => (
                <>
                  <CopyButton value={f.recoveryCodes.value.join('\n')} label={t('personalCenter.security.twofa.copy')} size="md" variant="secondary" />
                  <Button onClick={f.acknowledgeRecovery}>{t('personalCenter.security.twofa.acknowledge')}</Button>
                </>
              ),
            }}
          </Modal>
        </Card>
      )
    }
  },
})
