import { defineComponent, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ShieldCheck } from 'lucide-vue-next'
import { Button, Dialog, cn } from '@/components/ui'
import { useComplianceStore } from '@/stores/compliance'
import { notifyError, notifySuccess } from '@/utils/notify'
import { COMPLIANCE_EXPECTED, COMPLIANCE_SEPARATORS, toAcknowledgePayload, useComplianceAck } from '@/composables/useComplianceAck'

/** Blocking compliance acknowledgement (super admin must type 4 phrases, paste disabled). */
export const ComplianceAckDialog = defineComponent({
  name: 'ComplianceAckDialog',
  props: { open: Boolean },
  emits: { 'update:open': (_v: boolean) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const store = useComplianceStore()
    const ack = useComplianceAck()
    const submitting = ref(false)
    const inputs = ref<Array<HTMLInputElement | null>>([null, null, null, null])
    const shake = ref<number | null>(null)

    watch(
      () => props.open,
      (open) => {
        if (open) {
          ack.reset()
          void nextTick(() => inputs.value[0]?.focus())
        }
      },
    )

    const blocked = (e: Event) => {
      e.preventDefault()
      notifyError(t('compliance.dialog.pasteBlocked'))
    }

    const onInput = (idx: number, e: Event) => {
      const el = e.target as HTMLInputElement
      if (!ack.accept(idx, el.value)) {
        el.value = ack.segments.value[idx] ?? ''
        notifyError(t('compliance.dialog.pasteBlocked'))
        return
      }
      if (ack.states.value[idx] === 'invalid') {
        shake.value = idx
        window.setTimeout(() => (shake.value = null), 500)
      }
    }

    const submit = async () => {
      if (!ack.allValid.value || submitting.value) return
      submitting.value = true
      try {
        const p = toAcknowledgePayload(ack.segments.value)
        await store.acknowledge(p.segment1, p.segment2, p.segment3)
        notifySuccess(t('compliance.toast.success'))
        emit('update:open', false)
      } catch (err) {
        const msg = err instanceof Error ? err.message : ''
        if (msg.includes('compliance_required_by_super_admin')) notifyError(t('compliance.toast.superRequired'))
        else if (msg.includes('text_mismatch')) notifyError(t('compliance.toast.mismatch'))
        else notifyError(t('compliance.toast.failed'))
      } finally {
        submitting.value = false
      }
    }

    return () => (
      <Dialog modelValue={props.open} size="lg" hideClose closeOnOverlay={false} title={t('compliance.dialog.title')}>
        {{
          default: () => (
            <div class="space-y-4 text-sm">
              <p class="text-muted">{t('compliance.dialog.intro')}</p>
              <div class="max-h-[40vh] space-y-3 overflow-y-auto rounded-zs border border-line bg-surface-muted/60 p-4">
                {[1, 2, 3, 4].map((n) => (
                  <div key={n}>
                    <h4 class="zs-display text-fg">{t(`compliance.dialog.section${n}Title`)}</h4>
                    <p class="mt-1 leading-relaxed text-muted">{t(`compliance.dialog.section${n}Body`)}</p>
                  </div>
                ))}
              </div>
              <p class="font-medium text-fg">{t('compliance.dialog.confirmLabel')}</p>
              <p class="rounded-zs-sm bg-primary-soft px-3 py-2 text-primary">
                {COMPLIANCE_EXPECTED.map((s, i) => `${s}${COMPLIANCE_SEPARATORS[i]}`).join('')}
              </p>
              <div class="flex flex-wrap items-center gap-1.5">
                {COMPLIANCE_EXPECTED.map((expected, idx) => (
                  <span key={idx} class="flex min-w-[10rem] flex-1 items-center gap-1.5">
                    <input
                      ref={(el) => (inputs.value[idx] = el as HTMLInputElement | null)}
                      value={ack.segments.value[idx]}
                      maxlength={expected.length}
                      placeholder={expected}
                      class={cn(
                        'h-9 min-w-0 flex-1 rounded-zs-sm border bg-surface-strong px-3 text-sm text-fg placeholder:text-muted/40 focus:outline-none',
                        ack.states.value[idx] === 'valid' ? 'border-success' : ack.states.value[idx] === 'invalid' ? 'border-danger' : 'border-line-strong focus:border-primary',
                        shake.value === idx && 'animate-bounce',
                      )}
                      onPaste={blocked}
                      onDrop={blocked}
                      onContextmenu={(e: MouseEvent) => e.preventDefault()}
                      onCompositionstart={() => (ack.composing.value[idx] = true)}
                      onCompositionend={(e: CompositionEvent) => {
                        ack.composing.value[idx] = false
                        onInput(idx, e)
                      }}
                      onInput={(e: Event) => onInput(idx, e)}
                    />
                    {COMPLIANCE_SEPARATORS[idx] && <span class="text-muted">{COMPLIANCE_SEPARATORS[idx]}</span>}
                  </span>
                ))}
              </div>
              <p class="text-xs text-muted">{t('compliance.dialog.punctuationHint')}</p>
            </div>
          ),
          footer: () => (
            <Button variant="primary" disabled={!ack.allValid.value} loading={submitting.value} onClick={submit}>
              <ShieldCheck class="h-4 w-4" />
              {t('compliance.dialog.submit')}
            </Button>
          ),
        }}
      </Dialog>
    )
  },
})

export default ComplianceAckDialog
