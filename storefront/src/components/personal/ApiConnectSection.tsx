import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Check, Copy, Eye, EyeOff, Hourglass, KeyRound, Link2, RefreshCw, Sparkles } from 'lucide-vue-next'
import { CopyButton } from '@/components/common/CopyButton'
import { Alert, Badge, Button, Modal } from '@/components/ui'
import type { ApiConnect } from '@/composables/personal/useApiConnect'
import { formatDateTime } from '@/utils/format'
import { CompatConnectSection } from './CompatConnectSection'

/**
 * 个人中心 → API 对接 →「一键对接」: connection code + dual-secret rotation, plus the
 * 异次元 / 萌次元 compat credentials for acg-faka / mcy-shop downstream sites.
 */
export const ApiConnectSection = defineComponent({
  name: 'ApiConnectSection',
  props: {
    connect: { type: Object as PropType<ApiConnect>, required: true },
    protocols: { type: Array as PropType<string[]>, default: () => [] },
  },
  setup(props) {
    const { t } = useI18n()

    const rotationNotice = () => {
      const r = props.connect.rotation.value
      if (!r.pending) return null
      return (
        <div class="flex items-start gap-3 rounded-zs border border-warning/40 bg-warning-soft p-4" data-testid="rotation-pending">
          <Hourglass class="mt-0.5 size-4 shrink-0 text-warning-text" />
          <div class="text-sm">
            <div class="font-bold text-fg">{t('apiConnect.pendingTitle')}</div>
            <p class="mt-0.5 text-muted">
              {r.expiresAt ? t('apiConnect.pendingDesc', { time: formatDateTime(r.expiresAt) }) : t('apiConnect.pendingNoTime')}
            </p>
          </div>
        </div>
      )
    }

    const codeCard = () => {
      const c = props.connect
      if (!c.code.value) return null
      return (
        <div class="zs-pop space-y-3 rounded-zs border border-success/40 bg-success-soft p-5" data-testid="connection-code">
          <h4 class="flex items-center gap-2 font-bold text-success-text">
            <Link2 class="size-4" />
            {t('apiConnect.codeTitle')}
          </h4>
          <Alert tone="warning">{t('apiConnect.codeWarning')}</Alert>
          <code class="block max-h-32 overflow-y-auto break-all rounded-zs-sm bg-surface-strong px-3 py-2 font-mono text-xs text-fg" data-testid="connection-code-value">
            {c.displayedCode.value}
          </code>
          <div class="flex flex-wrap items-center gap-2">
            <Button size="sm" onClick={() => void c.copyCode()}>
              {c.codeCopied.value ? <Check class="size-3.5" /> : <Copy class="size-3.5" />}
              {c.codeCopied.value ? t('apiConnect.copied') : t('apiConnect.copyCode')}
            </Button>
            <Button size="sm" variant="secondary" onClick={c.toggleReveal}>
              {c.codeRevealed.value ? <EyeOff class="size-3.5" /> : <Eye class="size-3.5" />}
              {c.codeRevealed.value ? t('apiConnect.hide') : t('apiConnect.show')}
            </Button>
            <Button size="sm" variant="ghost" onClick={c.dismissCode}>
              {t('apiConnect.dismiss')}
            </Button>
          </div>
        </div>
      )
    }

    const secretCard = () => {
      const c = props.connect
      if (!c.rotatedSecret.value) return null
      return (
        <div class="zs-pop rounded-zs border border-success/40 bg-success-soft p-5" data-testid="rotated-secret">
          <h4 class="flex items-center gap-2 font-bold text-success-text">
            <KeyRound class="size-4" />
            {t('apiConnect.secretTitle')}
          </h4>
          <p class="mt-1 text-sm text-muted">{t('apiConnect.secretWarning')}</p>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <code class="zs-num min-w-0 flex-1 break-all rounded-zs-sm bg-surface-strong px-3 py-2 text-sm font-bold text-fg">{c.rotatedSecret.value}</code>
            <CopyButton value={c.rotatedSecret.value} label={t('apiConnect.copySecret')} />
            <Button size="sm" variant="ghost" onClick={c.dismissSecret}>
              {t('apiConnect.dismiss')}
            </Button>
          </div>
        </div>
      )
    }

    return () => {
      const c = props.connect
      const kind = c.confirming.value
      return (
        <section class="space-y-4 rounded-zs border border-primary/30 bg-primary-soft/50 p-5" data-testid="api-connect">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
              <h3 class="flex items-center gap-2 font-bold text-fg">
                <Sparkles class="size-4 text-primary" />
                {t('apiConnect.title')}
              </h3>
              <p class="mt-1 text-sm leading-relaxed text-muted">{t('apiConnect.desc')}</p>
              <p class="mt-1 text-xs text-muted">{t('apiConnect.benefits')}</p>
            </div>
            {props.protocols.length > 0 && (
              <div class="flex flex-wrap items-center gap-1.5 text-xs text-muted">
                {t('apiConnect.protocols')}
                {props.protocols.map((p) => (
                  <Badge key={p} tone="accent">
                    {p}
                  </Badge>
                ))}
              </div>
            )}
          </div>
          <div class="flex flex-wrap gap-2">
            <Button loading={c.busy.value && kind === 'code'} disabled={c.busy.value} onClick={() => c.ask('code')}>
              <Link2 class="size-4" />
              {t('apiConnect.generate')}
            </Button>
            <Button variant="secondary" loading={c.busy.value && kind === 'rotate'} disabled={c.busy.value} onClick={() => c.ask('rotate')}>
              <RefreshCw class="size-4" />
              {t('apiConnect.rotate')}
            </Button>
          </div>
          {c.error.value && <Alert tone="error">{c.error.value}</Alert>}
          {rotationNotice()}
          {codeCard()}
          {secretCard()}
          <CompatConnectSection />
          <Modal
            open={kind !== null}
            title={kind === 'rotate' ? t('apiConnect.confirmRotateTitle') : t('apiConnect.confirmCodeTitle')}
            size="sm"
            onClose={c.cancel}
          >
            {{
              default: () => (
                <div class="flex gap-3">
                  <AlertTriangle class="mt-0.5 size-5 shrink-0 text-warning-text" />
                  <p class="text-sm leading-relaxed text-muted">{kind === 'rotate' ? t('apiConnect.confirmRotateDesc') : t('apiConnect.confirmCodeDesc')}</p>
                </div>
              ),
              footer: () => (
                <>
                  <Button variant="secondary" disabled={c.busy.value} onClick={c.cancel}>
                    {t('apiConnect.cancel')}
                  </Button>
                  <Button loading={c.busy.value} data-testid="api-connect-confirm" onClick={() => void c.confirm()}>
                    {kind === 'rotate' ? t('apiConnect.rotate') : t('apiConnect.generate')}
                  </Button>
                </>
              ),
            }}
          </Modal>
        </section>
      )
    }
  },
})
