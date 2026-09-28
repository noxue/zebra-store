import { defineComponent, reactive, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Dialog, FormField, Input, Select, Textarea } from '@/components/ui'
import type { AdminResellerProfile, AdminResellerProfileUpdatePayload } from '@/api/types'
import { notifyError } from '@/utils/notify'
import { validateMarkupRange } from '../resellerUtils'

/**
 * Edit reseller operations config (default/max markup, settlement status, reason).
 * Shared by the profiles list (`variant="list"`) and the profile detail page (`variant="detail"`),
 * which use slightly different label keys in the original.
 */
export const ProfileEditDialog = defineComponent({
  name: 'ResellerProfileEditDialog',
  props: {
    modelValue: Boolean,
    profile: {
      type: Object as PropType<AdminResellerProfile | null>,
      default: null,
    },
    saving: Boolean,
    variant: { type: String as PropType<'list' | 'detail'>, default: 'list' },
  },
  emits: {
    'update:modelValue': (_v: boolean) => true,
    submit: (_p: AdminResellerProfileUpdatePayload) => true,
  },
  setup(props, { emit }) {
    const { t } = useI18n()
    const form = reactive({
      defaultMarkup: '0.00',
      maxMarkup: '0.00',
      settlementStatus: 'normal',
      reason: '',
    })

    watch(
      () => props.modelValue,
      (open) => {
        if (!open || !props.profile) return
        form.defaultMarkup = props.profile.default_markup_percent || '0.00'
        form.maxMarkup = props.profile.max_markup_percent || '0.00'
        form.settlementStatus = props.profile.settlement_status || 'normal'
        form.reason = ''
      },
      { immediate: true },
    )

    const submit = () => {
      const err = validateMarkupRange(form.defaultMarkup, form.maxMarkup)
      if (err) {
        notifyError(t(err))
        return
      }
      emit('submit', {
        default_markup_percent: form.defaultMarkup.trim() || '0.00',
        max_markup_percent: form.maxMarkup.trim() || '0.00',
        settlement_status: form.settlementStatus,
        reason: form.reason.trim() || undefined,
      })
    }

    return () => {
      const detail = props.variant === 'detail'
      const label = (listKey: string, detailKey: string) => t(detail ? `admin.resellerProfileDetail.editDialog.${detailKey}` : listKey)
      return (
        <Dialog
          modelValue={props.modelValue}
          onUpdate:modelValue={(v: boolean) => emit('update:modelValue', v)}
          title={t('admin.resellerProfiles.actions.editDialogTitle', {
            id: props.profile?.id ?? '-',
          })}
          size="md"
        >
          {{
            default: () => (
              <div class="grid gap-4 sm:grid-cols-2">
                <FormField label={label('admin.resellerProfiles.table.defaultMarkup', 'defaultMarkup')}>
                  <Input v-model={form.defaultMarkup} placeholder="0.00" mono />
                </FormField>
                <FormField label={label('admin.resellerProfiles.table.maxMarkup', 'maxMarkup')}>
                  <Input v-model={form.maxMarkup} placeholder="0.00" mono />
                </FormField>
                <div class="sm:col-span-2">
                  <FormField label={label('admin.resellerProfiles.table.settlement', 'settlement')}>
                    <Select
                      v-model={form.settlementStatus}
                      options={[
                        {
                          label: t('admin.resellerProfiles.settlement.normal'),
                          value: 'normal',
                        },
                        {
                          label: detail ? t('admin.resellerProfileDetail.editDialog.settlementFrozen') : t('admin.resellerProfiles.settlement.frozen'),
                          value: 'frozen',
                        },
                      ]}
                    />
                  </FormField>
                </div>
                <div class="sm:col-span-2">
                  <FormField label={t('admin.resellerProfiles.actions.operationReason')}>
                    <Textarea v-model={form.reason} rows={3} />
                  </FormField>
                </div>
                <p class="text-xs leading-relaxed text-muted sm:col-span-2">
                  {detail ? t('admin.resellerProfileDetail.editDialog.hint') : t('admin.resellerProfiles.actions.editHint')}
                </p>
              </div>
            ),
            footer: () => (
              <>
                <Button onClick={() => emit('update:modelValue', false)}>{t('admin.common.cancel')}</Button>
                <Button variant="primary" loading={props.saving} onClick={submit}>
                  {t('admin.resellerProfiles.actions.saveConfig')}
                </Button>
              </>
            ),
          }}
        </Dialog>
      )
    }
  },
})

export default ProfileEditDialog
