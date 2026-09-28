import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { BadgeCheck, Check, CheckCircle2, X } from 'lucide-vue-next'
import { REASON_MAX, useResellerApply, type StepState } from '@/composables/reseller/useResellerApply'
import { ResellerAlert, ResellerPageHeader, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { Alert, Button, Card, Mascot, Textarea, cn } from '@/components/ui'

const nodeClass = (s: StepState) => {
  if (s === 'done') return 'bg-success text-on-primary'
  if (s === 'failed') return 'bg-danger text-on-primary'
  if (s === 'current') return 'zs-gradient-bg text-on-primary shadow-zs'
  return 'bg-surface-muted text-muted'
}

export default defineComponent({
  name: 'ResellerApply',
  setup() {
    const { t } = useI18n()
    const a = useResellerApply()
    onMounted(() => {
      if (!a.profile.snapshot.value) void a.profile.load()
    })
    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.apply.title')} description={t('resellerConsole.apply.description')} />
        <ResellerAlert alert={a.alert.value} />
        {a.profile.loading.value && !a.profile.snapshot.value ? (
          <ResellerPageState loading title={t('resellerConsole.common.loading')} description={t('resellerConsole.common.loadingDescription')} />
        ) : (
          <>
            <Card>
              <p class="mb-5 text-xs font-bold tracking-[0.14em] text-muted">
                {t('resellerConsole.apply.currentStatus')}：<span class="text-primary-text">{a.statusText.value}</span>
              </p>
              <div class="flex items-start">
                {a.steps.value.flatMap((step, i) => [
                    <div key={`s${i}`} class="flex w-20 shrink-0 flex-col items-center gap-1.5 text-center">
                      <span class={cn('flex size-9 items-center justify-center rounded-full text-sm font-bold', nodeClass(step.state))}>
                        {step.state === 'done' ? <Check class="size-4" /> : step.state === 'failed' ? <X class="size-4" /> : i + 1}
                      </span>
                      <span class={cn('text-xs', step.state === 'pending' ? 'text-muted' : 'font-bold text-fg')}>{step.label}</span>
                    </div>,
                    i < a.steps.value.length - 1 ? <span key={`l${i}`} class={cn('mt-4 h-1 flex-1 rounded-full', step.state === 'done' ? 'bg-success/60' : 'bg-surface-muted')} /> : null,
                ])}
              </div>
            </Card>

            {a.rejectReason.value && <Alert tone="warning">{t('personalCenter.reseller.rejectReason', { reason: a.rejectReason.value })}</Alert>}

            <div class="grid grid-cols-1 gap-5 lg:grid-cols-3">
              <Card>
                <h2 class="zs-title text-lg text-fg">{t('resellerConsole.apply.benefitsTitle')}</h2>
                <ul class="mt-4 space-y-3">
                  {a.benefits.value.map((b, i) => (
                    <li key={i} class="flex items-start gap-2.5 text-sm text-fg">
                      <CheckCircle2 class="mt-0.5 size-5 shrink-0 text-success" />
                      <span>{b}</span>
                    </li>
                  ))}
                </ul>
                <div class="mx-auto mt-6 h-40 w-32">
                  <Mascot builtin mood="wink" />
                </div>
              </Card>
              <Card class="lg:col-span-2">
                {a.canSubmit.value ? (
                  <form
                    class="space-y-4"
                    onSubmit={(e: Event) => {
                      e.preventDefault()
                      void a.submit()
                    }}
                  >
                    <h2 class="zs-title text-lg text-fg">
                      {a.profile.state.value.profileStatus === 'rejected' ? t('personalCenter.reseller.reapplyTitle') : t('personalCenter.reseller.applyTitle')}
                    </h2>
                    <p class="text-sm text-muted">
                      {a.profile.state.value.profileStatus === 'rejected' ? t('personalCenter.reseller.reapplyNotice') : t('personalCenter.reseller.applyNotice')}
                    </p>
                    <Textarea v-model={a.reason.value} rows={6} maxlength={REASON_MAX} disabled={a.submitting.value} placeholder={t('personalCenter.reseller.applyReasonPlaceholder')} />
                    <div class="flex items-center justify-between gap-3">
                      <span class="whitespace-nowrap zs-num text-xs text-muted">
                        {a.reason.value.length}/{REASON_MAX}
                      </span>
                      <Button type="submit" loading={a.submitting.value}>
                        {a.submitting.value ? t('personalCenter.reseller.applying') : t('personalCenter.reseller.applySubmit')}
                      </Button>
                    </div>
                  </form>
                ) : (
                  <div class="flex h-full flex-col items-center justify-center py-8 text-center">
                    <span class="flex size-14 items-center justify-center rounded-zs bg-primary-soft text-primary-text">
                      <BadgeCheck class="size-7" />
                    </span>
                    <h3 class="zs-title mt-4 text-lg text-fg">
                      {a.profile.state.value.opened ? t('resellerConsole.apply.unavailableTitle') : t('zsReseller.notOpenedTitle')}
                    </h3>
                    <p class="mx-auto mt-2 max-w-md text-sm text-muted">
                      {a.profile.state.value.opened ? t('resellerConsole.apply.unavailableDescription') : t('personalCenter.reseller.applyUnavailable')}
                    </p>
                    <Button to="/reseller" class="mt-5">
                      {t('resellerConsole.nav.dashboard')}
                    </Button>
                  </div>
                )}
              </Card>
            </div>
          </>
        )}
      </div>
    )
  },
})
