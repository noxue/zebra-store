import { ref } from 'vue'
import { defineStore } from 'pinia'

export type ConfirmDialogVariant = 'default' | 'destructive'
export type ConfirmDialogDescriptionTone = 'default' | 'danger' | 'muted'

export interface ConfirmDialogDescriptionSegment {
  text: string
  tone?: ConfirmDialogDescriptionTone
  strong?: boolean
}

export interface ConfirmDialogOptions {
  title: string
  description: string | ConfirmDialogDescriptionSegment[]
  confirmText: string
  cancelText: string
  variant: ConfirmDialogVariant
}

const defaults: ConfirmDialogOptions = { title: '', description: '', confirmText: '', cancelText: '', variant: 'default' }

export const useConfirmStore = defineStore('admin-confirm', () => {
  const open = ref(false)
  const options = ref<ConfirmDialogOptions>({ ...defaults })
  let resolver: ((value: boolean) => void) | null = null

  const finalize = (value: boolean) => {
    const current = resolver
    resolver = null
    open.value = false
    if (current) current(value)
  }

  const ask = (next: ConfirmDialogOptions) => {
    if (resolver) finalize(false)
    options.value = { ...next }
    open.value = true
    return new Promise<boolean>((resolve) => {
      resolver = resolve
    })
  }

  return { open, options, ask, confirm: () => finalize(true), cancel: () => finalize(false) }
})
