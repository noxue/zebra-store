import { defineComponent, reactive, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Dialog, FormField, IdCell, Select, Textarea } from '@/components/ui'
import { adminAPI } from '@/api/admin'
import type { AdminCardSecret } from '@/api/types'
import { CARD_SECRET_STATUSES } from '../cardSecretUtils'

/** Edit one card secret (secret text + status) → `updateCardSecret`. */
export default defineComponent({
  name: 'CardSecretEditModal',
  props: {
    modelValue: { type: Boolean, default: false },
    cardSecret: { type: Object as PropType<AdminCardSecret | null>, default: null },
  },
  emits: { 'update:modelValue': (_v: boolean) => true, success: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const submitting = ref(false)
    const error = ref('')
    const form = reactive({ id: 0, secret: '', status: 'available' as string })

    watch(
      () => [props.cardSecret, props.modelValue] as const,
      ([secret, open]) => {
        if (secret && open) {
          form.id = secret.id
          form.secret = secret.secret || ''
          form.status = secret.status || 'available'
          error.value = ''
        }
      },
      { immediate: true },
    )

    const close = () => {
      emit('update:modelValue', false)
      error.value = ''
    }

    const submit = async () => {
      if (!form.id || submitting.value) return
      submitting.value = true
      error.value = ''
      try {
        await adminAPI.updateCardSecret(form.id, { secret: form.secret, status: form.status })
        close()
        emit('success')
      } catch (err) {
        error.value = err instanceof Error && err.message ? err.message : t('admin.cardSecrets.errors.updateFailed')
      } finally {
        submitting.value = false
      }
    }

    return () => (
      <Dialog modelValue={props.modelValue} onUpdate:modelValue={(v) => !v && close()} title={t('admin.cardSecrets.editTitle')} size="md">
        {{
          default: () => (
            <form
              class="space-y-4"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void submit()
              }}
            >
              <div class="flex items-center gap-2 text-xs text-muted">
                <span>{t('admin.cardSecrets.editId')}:</span>
                {form.id ? <IdCell value={form.id} /> : <span>-</span>}
              </div>
              <FormField label={t('admin.cardSecrets.editSecret')}>
                <Textarea v-model={form.secret} rows={3} mono placeholder={t('admin.cardSecrets.editSecretPlaceholder')} />
              </FormField>
              <FormField label={t('admin.cardSecrets.editStatus')}>
                <Select v-model={form.status} options={CARD_SECRET_STATUSES.map((s) => ({ value: s, label: t(`admin.cardSecrets.status.${s}`) }))} />
              </FormField>
              {error.value && <div class="rounded-zs-sm border border-line bg-danger-soft p-3 text-sm text-danger-text">{error.value}</div>}
            </form>
          ),
          footer: () => (
            <>
              <Button onClick={close}>{t('admin.common.cancel')}</Button>
              <Button variant="primary" loading={submitting.value} onClick={() => void submit()}>
                {submitting.value ? t('admin.common.loading') : t('admin.common.save')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )
  },
})
