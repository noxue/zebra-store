import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Eye, EyeOff, KeyRound, RefreshCw, ShieldCheck, Store } from 'lucide-vue-next'
import { CopyButton } from '@/components/common/CopyButton'
import { Alert, Badge, Button, Modal, Switch, Textarea } from '@/components/ui'
import { useCompatConnect } from '@/composables/personal/useCompatConnect'
import { formatDateTime } from '@/utils/format'

/** i18n key of each served compat protocol's setup steps. */
const PROTOCOL_KEYS: Record<string, string> = {
  'acg-faka': 'acgFaka',
  'mcy-open-api': 'mcy',
}

/** 个人中心 → API 对接 → 异次元 / 萌次元 对接: compat key for acg-faka / mcy-shop downstream sites. */
export const CompatConnectSection = defineComponent({
  name: 'CompatConnectSection',
  setup() {
    const { t } = useI18n()
    const c = useCompatConnect()
    onMounted(() => void c.load())

    /** A labelled value; `copy` is what the copy button copies (the full key even while masked). */
    const row = (label: string, value: string, copy: string, testid: string, extra?: () => unknown) => (
      <div>
        <div class="text-xs font-bold text-muted">{label}</div>
        <div class="mt-1.5 flex flex-wrap items-center gap-2">
          <code class="zs-num min-w-0 flex-1 break-all rounded-zs-sm bg-surface-muted px-3 py-2 text-sm text-fg" data-testid={testid}>
            {value || '-'}
          </code>
          {extra?.()}
          {copy && <CopyButton value={copy} label={t('apiCompat.copy')} />}
        </div>
      </div>
    )

    const credentials = () => {
      const d = c.data.value
      if (!d) return null
      if (!c.hasKey.value) {
        return (
          <div class="flex flex-col gap-3 rounded-zs border border-dashed border-line p-4 sm:flex-row sm:items-center sm:justify-between">
            <p class="text-sm text-muted">{t('apiCompat.noKey')}</p>
            <Button loading={c.busy.value} data-testid="compat-issue" onClick={() => void c.issue()}>
              <KeyRound class="size-4" />
              {t('apiCompat.issue')}
            </Button>
          </div>
        )
      }
      return (
        <div class="space-y-3 rounded-zs border border-line bg-surface-strong p-4">
          {row(t('apiCompat.siteUrl'), d.site_url, d.site_url, 'compat-site-url')}
          {row(t('apiCompat.appId'), d.app_id, d.app_id, 'compat-app-id')}
          {row(t('apiCompat.appKey'), c.displayedKey.value, d.app_key, 'compat-app-key', () => (
            <Button size="sm" variant="secondary" onClick={c.toggleReveal}>
              {c.keyRevealed.value ? <EyeOff class="size-3.5" /> : <Eye class="size-3.5" />}
              {c.keyRevealed.value ? t('apiCompat.hide') : t('apiCompat.show')}
            </Button>
          ))}
          <div class="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-3">
            <div>
              <div class="text-sm font-bold text-fg">{t('apiCompat.activeLabel')}</div>
              <div class="mt-0.5 text-xs text-muted">
                {d.is_active ? t('apiCompat.activeOn') : t('apiCompat.activeOff')}
                {d.last_used_at && ` · ${t('apiCompat.lastUsed')} ${formatDateTime(d.last_used_at)}`}
              </div>
            </div>
            <Switch modelValue={d.is_active} disabled={c.busy.value} label={t('apiCompat.activeLabel')} onUpdate:modelValue={(v: boolean) => void c.toggleActive(v)} />
          </div>
          <div class="border-t border-line pt-3">
            <div class="text-sm font-bold text-fg">{t('apiCompat.allowlistLabel')}</div>
            <p class="mt-0.5 text-xs text-muted">{t('apiCompat.allowlistHint')}</p>
            <Textarea class="mt-2" rows={2} modelValue={c.allowlist.value} placeholder={'203.0.113.7\n198.51.100.0/24'} onUpdate:modelValue={(v: string) => (c.allowlist.value = v)} />
            <div class="mt-2 flex justify-end">
              <Button size="sm" variant="secondary" disabled={!c.allowlistDirty.value || c.busy.value} onClick={() => void c.saveAllowlist()}>
                {t('apiCompat.allowlistSave')}
              </Button>
            </div>
          </div>
          <div class="flex justify-end border-t border-line pt-3">
            <Button size="sm" variant="ghost" disabled={c.busy.value} onClick={c.askIssue}>
              <RefreshCw class="size-3.5" />
              {t('apiCompat.reset')}
            </Button>
          </div>
        </div>
      )
    }

    const steps = () => {
      const protocols = c.data.value?.protocols ?? []
      if (!protocols.length) return null
      return (
        <div class="grid gap-3 md:grid-cols-2">
          {protocols.map((p) => {
            const key = PROTOCOL_KEYS[p.id] ?? p.id
            return (
              <div key={p.id} class="rounded-zs border border-line bg-surface-strong p-4" data-testid={`compat-protocol-${p.id}`}>
                <div class="flex items-center justify-between gap-2">
                  <span class="font-bold text-fg">{t(`apiCompat.protocols.${key}.name`)}</span>
                  <Badge tone={p.enabled ? 'success' : 'neutral'}>{p.enabled ? t('apiCompat.protocolOn') : t('apiCompat.protocolOff')}</Badge>
                </div>
                <p class="mt-1.5 text-xs leading-relaxed text-muted">{t(`apiCompat.protocols.${key}.steps`)}</p>
              </div>
            )
          })}
        </div>
      )
    }

    return () => (
      <section class="space-y-4 rounded-zs border border-secondary/30 bg-secondary-soft/40 p-5" data-testid="compat-connect">
        <div>
          <h3 class="flex items-center gap-2 font-bold text-fg">
            <Store class="size-4 text-secondary" />
            {t('apiCompat.title')}
          </h3>
          <p class="mt-1 text-sm leading-relaxed text-muted">{t('apiCompat.desc')}</p>
        </div>
        {c.error.value && <Alert tone="error">{c.error.value}</Alert>}
        {credentials()}
        {steps()}
        <div class="flex items-start gap-2 text-xs leading-relaxed text-muted">
          <ShieldCheck class="mt-0.5 size-3.5 shrink-0 text-warning-text" />
          <span>{t('apiCompat.security')}</span>
        </div>
        <Modal open={c.confirmingIssue.value} title={t('apiCompat.resetTitle')} size="sm" onClose={c.cancelIssue}>
          {{
            default: () => (
              <div class="flex gap-3">
                <AlertTriangle class="mt-0.5 size-5 shrink-0 text-warning-text" />
                <p class="text-sm leading-relaxed text-muted">{t('apiCompat.resetDesc')}</p>
              </div>
            ),
            footer: () => (
              <>
                <Button variant="secondary" disabled={c.busy.value} onClick={c.cancelIssue}>
                  {t('apiCompat.cancel')}
                </Button>
                <Button variant="danger" loading={c.busy.value} data-testid="compat-reset-confirm" onClick={() => void c.issue()}>
                  {t('apiCompat.reset')}
                </Button>
              </>
            ),
          }}
        </Modal>
      </section>
    )
  },
})
