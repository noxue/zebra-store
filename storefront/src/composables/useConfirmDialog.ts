import { readonly, ref } from 'vue'

export interface ConfirmDialogOptions {
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  variant?: 'danger' | 'default'
}

const visible = ref(false)
const options = ref<ConfirmDialogOptions>({ title: '', message: '' })
let resolver: ((value: boolean) => void) | null = null

/** Promise based global confirm dialog (rendered once in App). */
export function useConfirmDialog() {
  const confirm = (opts: ConfirmDialogOptions): Promise<boolean> => {
    resolver?.(false)
    options.value = opts
    visible.value = true
    return new Promise<boolean>((resolve) => {
      resolver = resolve
    })
  }
  const settle = (value: boolean) => {
    visible.value = false
    resolver?.(value)
    resolver = null
  }
  return {
    visible: readonly(visible),
    options: readonly(options),
    confirm,
    handleConfirm: () => settle(true),
    handleCancel: () => settle(false),
  }
}
