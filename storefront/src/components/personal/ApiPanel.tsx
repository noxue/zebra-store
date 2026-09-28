import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Clock, KeyRound, Key, RefreshCw, ShieldCheck, XCircle } from 'lucide-vue-next'
import { CopyButton } from '@/components/common/CopyButton'
import { Badge, Button, Card, Modal, Switch } from '@/components/ui'
import { useApiPanel } from '@/composables/personal/useApiPanel'
import { useApiConnect } from '@/composables/personal/useApiConnect'
import { ApiConnectSection } from './ApiConnectSection'
import { DashedNote, PanelAlertBox, PanelHeading, SkeletonRows } from './PanelParts'

/** API credential application and management. */
export const ApiPanel = defineComponent({
  name: 'ApiPanel',
  setup() {
    const { t } = useI18n()
    const a = useApiPanel()
    const connect = useApiConnect({ credential: a.credential, reload: a.reload })

    const approved = () => {
      const c = a.credential.value
      if (!c) return null
      return (
        <div class="space-y-4">
          {!a.hasViewedSecret.value && !a.newSecret.value && (
            <div class="rounded-zs border border-accent/35 bg-accent-soft p-5">
              <h3 class="flex items-center gap-2 font-bold text-accent-text">
                <ShieldCheck class="size-4" />
                {t('personalCenter.apiPanel.approvedNoticeTitle')}
              </h3>
              <p class="mt-1 text-sm text-muted">{t('personalCenter.apiPanel.approvedNoticeDesc')}</p>
              <Button class="mt-4" loading={a.submitting.value} onClick={() => void a.firstGenerate()}>
                {a.submitting.value ? t('personalCenter.apiPanel.regenerating') : t('personalCenter.apiPanel.generateSecret')}
              </Button>
            </div>
          )}
          <div class="rounded-zs border border-line bg-surface-strong p-5">
            <div class="text-xs font-bold text-muted">API Key</div>
            <div class="mt-2 flex flex-wrap items-center gap-2">
              <code class="zs-num min-w-0 flex-1 break-all rounded-zs-sm bg-surface-muted px-3 py-2 text-sm text-fg">{c.api_key || '-'}</code>
              {c.api_key && <CopyButton value={c.api_key} label={t('personalCenter.apiPanel.copy')} />}
            </div>
            <div class="mt-4 text-xs font-bold text-muted">API Secret</div>
            <div class="mt-2 flex flex-wrap items-center gap-2">
              <code class="zs-num min-w-0 flex-1 break-all rounded-zs-sm bg-surface-muted px-3 py-2 text-sm text-muted">{a.maskedSecret.value}</code>
              <Button size="sm" variant="secondary" onClick={a.askRegenerate}>
                <RefreshCw class="size-3.5" />
                {t('personalCenter.apiPanel.regenerate')}
              </Button>
            </div>
            <p class="mt-2 text-xs text-muted">{t('personalCenter.apiPanel.secretHint')}</p>
          </div>
          {a.newSecret.value && (
            <div class="zs-pop rounded-zs border border-success/40 bg-success-soft p-5">
              <h3 class="flex items-center gap-2 font-bold text-success-text">
                <KeyRound class="size-4" />
                {t('personalCenter.apiPanel.newSecretTitle')}
              </h3>
              <p class="mt-1 text-sm text-muted">{t('personalCenter.apiPanel.newSecretWarning')}</p>
              <div class="mt-3 flex flex-wrap items-center gap-2">
                <code class="zs-num min-w-0 flex-1 break-all rounded-zs-sm bg-surface-strong px-3 py-2 text-sm font-bold text-fg">{a.newSecret.value}</code>
                <CopyButton value={a.newSecret.value} label={t('personalCenter.apiPanel.copySecret')} />
              </div>
            </div>
          )}
          {connect.available.value && <ApiConnectSection connect={connect} protocols={c.protocols ?? []} />}
          <div class="flex items-center justify-between gap-4 rounded-zs border border-line bg-surface-strong p-5">
            <div>
              <div class="text-sm font-bold text-fg">{t('personalCenter.apiPanel.statusLabel')}</div>
              <div class="mt-0.5 text-xs text-muted">{c.is_active ? t('personalCenter.apiPanel.statusEnabled') : t('personalCenter.apiPanel.statusDisabled')}</div>
            </div>
            <Switch modelValue={!!c.is_active} disabled={a.submitting.value} label={t('personalCenter.apiPanel.statusLabel')} onUpdate:modelValue={(v: boolean) => void a.toggleActive(v)} />
          </div>
        </div>
      )
    }

    const body = () => {
      const c = a.credential.value
      if (a.loading.value) return <SkeletonRows />
      if (!c) {
        return (
          <DashedNote>
            <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
              <span class="leading-relaxed">{t('personalCenter.apiPanel.noCredential')}</span>
              <Button loading={a.submitting.value} onClick={() => void a.apply()}>
                {a.submitting.value ? t('personalCenter.apiPanel.applying') : t('personalCenter.apiPanel.apply')}
              </Button>
            </div>
          </DashedNote>
        )
      }
      if (c.status === 'pending_review' || c.status === 'pending') {
        return (
          <div class="flex items-start gap-3 rounded-zs border border-warning/40 bg-warning-soft p-5">
            <Clock class="mt-0.5 size-5 text-warning-text" />
            <div>
              <h3 class="font-bold text-fg">{t('personalCenter.apiPanel.pendingTitle')}</h3>
              <p class="mt-1 text-sm text-muted">{t('personalCenter.apiPanel.pendingDesc')}</p>
            </div>
          </div>
        )
      }
      if (c.status === 'rejected') {
        return (
          <div class="flex flex-col gap-4 rounded-zs border border-danger/40 bg-danger-soft p-5 sm:flex-row sm:items-start sm:justify-between">
            <div class="flex items-start gap-3">
              <XCircle class="mt-0.5 size-5 text-danger-text" />
              <div>
                <h3 class="font-bold text-fg">{t('personalCenter.apiPanel.rejectedTitle')}</h3>
                {c.reject_reason && <p class="mt-1 text-sm text-muted">{t('personalCenter.apiPanel.rejectReason', { reason: c.reject_reason })}</p>}
              </div>
            </div>
            <Button loading={a.submitting.value} onClick={() => void a.apply()}>
              {a.submitting.value ? t('personalCenter.apiPanel.applying') : t('personalCenter.apiPanel.reapply')}
            </Button>
          </div>
        )
      }
      return approved()
    }

    return () => (
      <Card>
        <PanelHeading title={t('personalCenter.apiPanel.title')} description={t('personalCenter.apiPanel.subtitle')} icon={Key}>
          {{ actions: () => <Badge tone="accent">{t('personalCenter.tabs.api')}</Badge> }}
        </PanelHeading>
        <PanelAlertBox alert={a.alert.value} />
        {body()}
        <Modal open={a.confirmOpen.value} title={t('personalCenter.apiPanel.regenerateTitle')} size="sm" onClose={() => (a.confirmOpen.value = false)}>
          {{
            default: () => (
              <div class="flex gap-3">
                <AlertTriangle class="mt-0.5 size-5 shrink-0 text-warning-text" />
                <p class="text-sm leading-relaxed text-muted">{t('personalCenter.apiPanel.regenerateDesc')}</p>
              </div>
            ),
            footer: () => (
              <>
                <Button variant="secondary" onClick={() => (a.confirmOpen.value = false)}>
                  {t('personalCenter.apiPanel.cancel')}
                </Button>
                <Button variant="danger" loading={a.submitting.value} onClick={() => void a.confirmRegenerate()}>
                  {a.submitting.value ? t('personalCenter.apiPanel.regenerating') : t('personalCenter.apiPanel.regenerateConfirm')}
                </Button>
              </>
            ),
          }}
        </Modal>
      </Card>
    )
  },
})
