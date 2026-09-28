import i18n from '@/i18n'
import {
  useConfirmStore,
  type ConfirmDialogDescriptionSegment,
  type ConfirmDialogVariant,
} from '@/stores/confirm'

export interface ConfirmActionOptions {
  title?: string
  description: string | ConfirmDialogDescriptionSegment[]
  confirmText?: string
  cancelText?: string
  variant?: ConfirmDialogVariant
}

const t = (key: string) => i18n.global.t(key)

/** Promise-based confirm dialog: `if (!(await confirmAction({...}))) return` */
export const confirmAction = (payload: string | ConfirmActionOptions) => {
  const options: ConfirmActionOptions = typeof payload === 'string' ? { description: payload } : payload
  return useConfirmStore().ask({
    title: options.title || t('admin.common.confirm'),
    description: options.description,
    confirmText: options.confirmText || t('admin.common.confirm'),
    cancelText: options.cancelText || t('admin.common.cancel'),
    variant: options.variant || 'default',
  })
}
