import { defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Copy, Download, TriangleAlert } from 'lucide-vue-next'
import { Button, Checkbox, Dialog } from '@/components/ui'
import { copyText } from '@/utils/clipboard'
import { downloadBlob } from '@/utils/download'
import { notifyError, notifySuccess } from '@/utils/notify'

export const RECOVERY_CODES_FILENAME = 'zebra-store-2fa-recovery-codes.txt'

/** One-time display of recovery codes; cannot be dismissed until the user acknowledges saving them. */
export const RecoveryCodesModal = defineComponent({
  name: 'RecoveryCodesModal',
  props: {
    modelValue: { type: Boolean, default: false },
    codes: { type: Array as PropType<string[]>, default: () => [] },
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const acknowledged = ref(false)

    watch(
      () => props.modelValue,
      (open) => open && (acknowledged.value = false),
    )

    const close = () => {
      if (!acknowledged.value) return
      emit('update:modelValue', false)
      acknowledged.value = false
    }

    const copyAll = async () => {
      try {
        await copyText(props.codes.join('\n'))
        notifySuccess(t('admin.twofa.recovery.copied'))
      } catch {
        notifyError(t('admin.common.copyFailed'))
      }
    }

    const downloadTxt = () => downloadBlob(new Blob([props.codes.join('\n') + '\n'], { type: 'text/plain' }), RECOVERY_CODES_FILENAME)

    return () => (
      <Dialog
        modelValue={props.modelValue}
        onUpdate:modelValue={(v) => (v ? emit('update:modelValue', v) : close())}
        hideClose={!acknowledged.value}
        closeOnOverlay={false}
        size="md"
      >
        {{
          title: () => (
            <span class="flex items-center gap-2 text-danger-text">
              <TriangleAlert class="h-4 w-4" />
              {t('admin.twofa.recovery.title')}
            </span>
          ),
          default: () => (
            <div class="space-y-4">
              <p class="rounded-zs-sm bg-warning-soft px-3 py-2 text-sm text-warning-text">{t('admin.twofa.recovery.warning')}</p>
              <div class="grid grid-cols-2 gap-2 rounded-zs border border-line bg-surface-muted p-3 font-mono text-sm text-fg">
                {props.codes.map((c) => (
                  <div key={c} class="rounded-zs-sm bg-surface-strong px-2 py-1 text-center tracking-wider">
                    {c}
                  </div>
                ))}
              </div>
              <div class="flex gap-2">
                <Button class="flex-1" onClick={copyAll}>
                  <Copy class="h-4 w-4" />
                  {t('admin.twofa.recovery.copyAll')}
                </Button>
                <Button class="flex-1" onClick={downloadTxt}>
                  <Download class="h-4 w-4" />
                  {t('admin.twofa.recovery.download')}
                </Button>
              </div>
              <Checkbox v-model={acknowledged.value} label={t('admin.twofa.recovery.acknowledge')} />
            </div>
          ),
          footer: () => (
            <Button variant="primary" disabled={!acknowledged.value} onClick={close}>
              {t('admin.twofa.recovery.confirmClose')}
            </Button>
          ),
        }}
      </Dialog>
    )
  },
})
