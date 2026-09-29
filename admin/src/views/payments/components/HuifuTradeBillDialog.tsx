import { defineComponent, ref, watch, type PropType } from 'vue'
import { Download, RefreshCw } from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import { adminAPI, type AdminPaymentChannel, type AdminTradeBillQuery } from '@/api/admin'
import { Badge, Button, DateTimeInput, Dialog, FormField } from '@/components/ui'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { notifySuccess } from '@/utils/notify'

const today = () => {
  const d = new Date()
  const local = new Date(d.getTime() - d.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 10)
}

const compactDate = (value: string) => value.replaceAll('-', '')

export const HuifuTradeBillDialog = defineComponent({
  name: 'HuifuTradeBillDialog',
  props: {
    modelValue: Boolean,
    channel: { type: Object as PropType<AdminPaymentChannel | null>, default: null },
  },
  emits: { 'update:modelValue': (_value: boolean) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const fileDate = ref(today())
    const loading = ref(false)
    const downloading = ref('')
    const result = ref<AdminTradeBillQuery | null>(null)

    watch(
      () => props.modelValue,
      (open) => {
        if (open) {
          fileDate.value = today()
          result.value = null
        }
      },
    )

    const query = async () => {
      if (!props.channel || !fileDate.value) return
      loading.value = true
      try {
        const response = await adminAPI.queryTradeBill(props.channel.id, compactDate(fileDate.value))
        result.value = response.data
      } finally {
        loading.value = false
      }
    }

    const download = async (fileId: string, fileName: string) => {
      if (!props.channel) return
      downloading.value = fileId
      try {
        const response = await adminAPI.downloadTradeBill(props.channel.id, compactDate(fileDate.value), fileId)
        downloadBlob(response.data, filenameFromDisposition(response.headers['content-disposition'], fileName || 'huifu-trade-bill.zip'))
        notifySuccess(t('admin.paymentChannels.tradeBill.downloaded'))
      } finally {
        downloading.value = ''
      }
    }

    return () => (
      <Dialog
        modelValue={props.modelValue}
        onUpdate:modelValue={(value: boolean) => emit('update:modelValue', value)}
        title={t('admin.paymentChannels.tradeBill.title')}
        description={t('admin.paymentChannels.tradeBill.description')}
        size="lg"
      >
        {{
          default: () => (
            <div class="space-y-5">
              <FormField label={t('admin.paymentChannels.tradeBill.fileDate')} hint={t('admin.paymentChannels.tradeBill.fileDateHint')} required>
                <div class="flex gap-2">
                  <DateTimeInput type="date" v-model={fileDate.value} />
                  <Button variant="primary" loading={loading.value} disabled={!fileDate.value} onClick={query}>
                    <RefreshCw class="h-4 w-4" />
                    {t('admin.paymentChannels.tradeBill.query')}
                  </Button>
                </div>
              </FormField>

              {result.value && (
                <div class="space-y-3 border-t border-line pt-4">
                  {result.value.files.length > 0 ? (
                    result.value.files.map((file) => (
                      <div key={file.file_id} class="flex flex-wrap items-center justify-between gap-3 rounded-zs border border-line bg-surface-muted/40 p-3">
                        <div class="min-w-0">
                          <div class="break-all text-sm font-medium text-fg">{file.file_name || file.file_id}</div>
                          <div class="mt-1 text-xs text-muted">{file.file_date}</div>
                        </div>
                        <Button size="sm" loading={downloading.value === file.file_id} onClick={() => download(file.file_id, file.file_name)}>
                          <Download class="h-4 w-4" />
                          {t('admin.paymentChannels.tradeBill.download')}
                        </Button>
                      </div>
                    ))
                  ) : (
                    <p class="text-sm text-muted">{t('admin.paymentChannels.tradeBill.noFile')}</p>
                  )}
                  {result.value.tasks.map((task, index) => (
                    <div key={`${task.data_date}-${index}`} class="flex flex-wrap items-center gap-2 text-xs text-muted">
                      <Badge tone={task.task_stat === 'S' ? 'success' : task.task_stat === 'F' ? 'danger' : 'warning'}>{task.task_stat || '-'}</Badge>
                      <span>{task.data_date}</span>
                      <span>{task.task_start_time || '-'}</span>
                      <span>→</span>
                      <span>{task.task_end_time || '-'}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ),
          footer: () => <Button onClick={() => emit('update:modelValue', false)}>{t('admin.common.cancel')}</Button>,
        }}
      </Dialog>
    )
  },
})

export default HuifuTradeBillDialog
