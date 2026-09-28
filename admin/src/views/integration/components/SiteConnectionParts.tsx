import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, CheckCircle2, ClipboardPaste, Loader2, PlugZap, RefreshCw } from 'lucide-vue-next'
import { Badge, Button, cn, FormField, Input, Select } from '@/components/ui'
import type { SiteConnectionConfig, SiteConnectionProtocolDef, SiteConnectionProtocolField } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { featureLabelKey } from '../integrationUtils'
import type { ConnectionWizard } from '../useConnectionWizard'

/** Capability ids → localized badges (unknown ids are shown verbatim). */
export const CapabilityBadges = defineComponent({
  name: 'CapabilityBadges',
  props: {
    items: { type: Array as PropType<string[]>, default: () => [] },
    emptyText: { type: String, default: '' },
  },
  setup(props) {
    const { t } = useI18n()
    return () =>
      props.items.length === 0 ? (
        props.emptyText ? <span class="text-xs text-muted">{props.emptyText}</span> : null
      ) : (
        <div class="flex flex-wrap gap-1.5">
          {props.items.map((id) => {
            const key = featureLabelKey(id)
            return (
              <Badge key={id} tone="secondary">
                {key ? t(key) : id}
              </Badge>
            )
          })}
        </div>
      )
  },
})

/** Protocol cards (name + description + capabilities), all coming from the backend registry. */
export const ProtocolPicker = defineComponent({
  name: 'ProtocolPicker',
  props: {
    protocols: { type: Array as PropType<SiteConnectionProtocolDef[]>, default: () => [] },
    modelValue: { type: String, default: '' },
  },
  emits: { 'update:modelValue': (_id: string) => true },
  setup(props, { emit }) {
    return () => (
      <div role="radiogroup" class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {props.protocols.map((p) => {
          const active = p.id === props.modelValue
          return (
            <button
              type="button"
              role="radio"
              aria-checked={active}
              key={p.id}
              data-protocol={p.id}
              onClick={() => emit('update:modelValue', p.id)}
              class={cn(
                'flex flex-col gap-2 rounded-zs border p-4 text-left transition-all',
                active ? 'border-primary bg-primary-soft shadow-glow' : 'border-line bg-surface-strong hover:border-line-strong',
              )}
            >
              <div class="flex items-center gap-2 font-medium text-fg">
                <span class={cn('h-3.5 w-3.5 shrink-0 rounded-full border-2', active ? 'border-primary bg-primary' : 'border-line-strong')} />
                {getLocalizedText(p.name) || p.id}
                <code class="ml-auto font-mono text-[11px] text-muted">{p.id}</code>
              </div>
              {getLocalizedText(p.description) && <p class="text-xs leading-relaxed text-muted">{getLocalizedText(p.description)}</p>}
              <CapabilityBadges items={p.capabilities} />
            </button>
          )
        })}
      </div>
    )
  },
})

/** Adapter-specific fields rendered from the protocol definition. */
export const ProtocolFields = defineComponent({
  name: 'ProtocolFields',
  props: {
    fields: { type: Array as PropType<SiteConnectionProtocolField[]>, default: () => [] },
    config: { type: Object as PropType<SiteConnectionConfig>, required: true },
    errors: { type: Object as PropType<Record<string, string>>, default: () => ({}) },
    editing: Boolean,
  },
  emits: { change: (_key: string, _value: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const wide = (f: SiteConnectionProtocolField) => f.kind === 'url'
    const renderControl = (f: SiteConnectionProtocolField) => {
      const value = props.config[f.key] ?? ''
      const placeholder = f.kind === 'secret' && props.editing ? t('siteConnections.secretKeepPlaceholder') : getLocalizedText(f.placeholder) || ''
      const update = (v: string | number) => emit('change', f.key, String(v))
      if (f.kind === 'select') {
        return (
          <Select
            modelValue={value}
            placeholder={placeholder}
            options={(f.options ?? []).map((o) => ({ value: o.value, label: getLocalizedText(o.label) || o.value }))}
            onUpdate:modelValue={update}
          />
        )
      }
      return (
        <Input
          modelValue={value}
          name={`config.${f.key}`}
          type={f.kind === 'secret' ? 'password' : 'text'}
          autocomplete={f.kind === 'secret' ? 'new-password' : 'off'}
          mono={f.kind !== 'text'}
          placeholder={placeholder}
          onUpdate:modelValue={update}
        />
      )
    }
    return () => (
      <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        {props.fields.map((f) => (
          <div key={f.key} class={wide(f) ? 'md:col-span-2' : undefined}>
            <FormField label={getLocalizedText(f.label) || f.key} required={f.required && !(props.editing && f.kind === 'secret')} error={props.errors[f.key]}>
              {renderControl(f)}
            </FormField>
          </div>
        ))}
      </div>
    )
  },
})

/** 「粘贴连接码」: parses on paste, blur or Enter. */
export const ConnectionCodeBox = defineComponent({
  name: 'ConnectionCodeBox',
  props: { wizard: { type: Object as PropType<ConnectionWizard>, required: true } },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const w = props.wizard
      return (
        <div class="rounded-zs border border-dashed border-primary/45 bg-primary-soft/60 p-4">
          <div class="flex items-center gap-2 text-sm font-semibold text-fg">
            <ClipboardPaste class="h-4 w-4 text-primary" />
            {t('siteConnections.connectionCode.title')}
          </div>
          <p class="mt-1 text-xs leading-relaxed text-muted">{t('siteConnections.connectionCode.hint')}</p>
          <div class="mt-3 flex gap-2">
            <div class="min-w-0 flex-1">
              <Input
                v-model={w.code.value}
                name="connection_code"
                mono
                autocomplete="off"
                placeholder={t('siteConnections.connectionCode.placeholder')}
                onPaste={w.onCodePaste}
                onBlur={() => void w.parseCode()}
                onKeydown={(e: KeyboardEvent) => {
                  // Enter parses the code; it must not submit the surrounding connection form.
                  if (e.key !== 'Enter' || e.isComposing) return
                  e.preventDefault()
                  void w.parseCode()
                }}
              />
            </div>
            <Button loading={w.parsing.value} disabled={!w.code.value.trim()} onClick={() => void w.parseCode()}>
              {t('siteConnections.connectionCode.parse')}
            </Button>
          </div>
          <p class="mt-2 text-[11px] text-muted">{w.parsing.value ? t('siteConnections.connectionCode.parsing') : t('siteConnections.connectionCode.orManual')}</p>
        </div>
      )
    }
  },
})

/** Handshake outcome: loading, friendly error, or site / currency / balance / capabilities + suggestions. */
export const HandshakePanel = defineComponent({
  name: 'HandshakePanel',
  props: {
    wizard: { type: Object as PropType<ConnectionWizard>, required: true },
    exchangeRate: { type: [Number, String] as PropType<number | ''>, default: '' },
    callbackUrl: { type: String, default: '' },
  },
  setup(props) {
    const { t } = useI18n()
    const stat = (label: string, value: string) => (
      <div class="min-w-0 rounded-zs-sm bg-surface-strong px-3 py-2">
        <div class="text-[11px] text-muted">{label}</div>
        <div class="zs-num truncate text-sm font-semibold text-fg">{value || '-'}</div>
      </div>
    )
    const suggestion = (label: string, hint: string, value: string, applied: boolean, onApply: () => void, testId: string) => (
      <div class="flex flex-wrap items-center gap-2 rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
        <div class="min-w-0 flex-1">
          <div class="text-[11px] text-muted">{label}</div>
          <div class="break-all font-mono text-xs text-fg">{value}</div>
          {hint && <div class="text-[11px] text-muted">{hint}</div>}
        </div>
        <Button size="xs" variant={applied ? 'ghost' : 'soft'} disabled={applied} data-testid={testId} onClick={onApply}>
          {applied ? t('siteConnections.wizard.applied') : t('siteConnections.wizard.apply')}
        </Button>
      </div>
    )
    return () => {
      const w = props.wizard
      if (w.testing.value) {
        return (
          <div class="flex items-center gap-2 rounded-zs border border-line bg-surface-strong p-4 text-sm text-muted" data-testid="handshake-loading">
            <Loader2 class="h-4 w-4 animate-spin text-primary motion-reduce:animate-none" />
            {t('siteConnections.wizard.testing')}
          </div>
        )
      }
      const err = w.error.value
      if (err) {
        return (
          <div role="alert" class="flex gap-3 rounded-zs border border-danger/35 bg-danger-soft p-4" data-testid="handshake-error">
            <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0 text-danger-text" />
            <div class="min-w-0">
              <div class="text-sm font-semibold text-danger-text">{err.title}</div>
              <p class="mt-0.5 break-words text-xs leading-relaxed text-fg/80">{err.detail}</p>
              {err.hint && <p class="mt-1 text-xs leading-relaxed text-muted">{err.hint}</p>}
            </div>
          </div>
        )
      }
      const r = w.result.value
      if (!r) return null
      const rate = w.suggestedRate()
      const callback = r.suggested_callback_url
      return (
        <div class="zs-pop space-y-3 rounded-zs border border-success/40 bg-success-soft p-4" data-testid="handshake-result">
          <div class="flex flex-wrap items-center gap-2 text-sm font-semibold text-success-text">
            <CheckCircle2 class="h-4 w-4" />
            {t('siteConnections.wizard.successTitle', { name: r.site?.name || r.site?.url || '-' })}
          </div>
          <div class="grid grid-cols-2 gap-2 md:grid-cols-4">
            {stat(t('siteConnections.wizard.site'), r.site?.name || '')}
            {stat(t('siteConnections.wizard.currency'), r.site?.currency || '')}
            {stat(t('siteConnections.wizard.balance'), r.account ? `${r.account.balance} ${r.account.currency || ''}`.trim() : '')}
            {stat(t('siteConnections.wizard.version'), [r.protocol, r.version].filter(Boolean).join(' '))}
          </div>
          <div>
            <div class="mb-1.5 text-[11px] text-muted">{t('siteConnections.wizard.capabilities')}</div>
            <CapabilityBadges items={r.features ?? []} emptyText={t('siteConnections.wizard.noCapabilities')} />
          </div>
          {(rate !== null || callback) && (
            <div class="grid grid-cols-1 gap-2 md:grid-cols-2">
              {rate !== null &&
                suggestion(
                  t('siteConnections.wizard.suggestedRate'),
                  t('siteConnections.wizard.suggestedRateHint'),
                  String(rate),
                  Number(props.exchangeRate) === rate,
                  w.applyExchangeRate,
                  'apply-exchange-rate',
                )}
              {callback &&
                suggestion(t('siteConnections.wizard.suggestedCallback'), '', callback, props.callbackUrl === callback, w.applyCallbackUrl, 'apply-callback-url')}
            </div>
          )}
        </div>
      )
    }
  },
})

/** Registry failed to load: explain the generic fallback and offer a retry. */
export const RegistryNotice = defineComponent({
  name: 'RegistryNotice',
  props: { message: { type: String, required: true }, loading: Boolean },
  emits: { retry: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => (
      <div class="flex flex-wrap items-center gap-3 rounded-zs border border-warning/40 bg-warning-soft px-4 py-3 text-xs text-warning-text">
        <PlugZap class="h-4 w-4 shrink-0" />
        <span class="min-w-0 flex-1">{props.message}</span>
        <Button size="xs" loading={props.loading} onClick={() => emit('retry')}>
          <RefreshCw class="h-3.5 w-3.5" />
          {t('siteConnections.retry')}
        </Button>
      </div>
    )
  },
})
