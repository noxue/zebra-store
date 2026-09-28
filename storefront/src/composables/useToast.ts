import { readonly, ref } from 'vue'

export interface ToastAction {
  label: string
  onClick: () => void
}

export interface ToastItem {
  id: number
  message: string
  type: 'success' | 'error' | 'info'
  action?: ToastAction
}

interface ToastOptions {
  duration?: number
  action?: ToastAction
}

const toasts = ref<ToastItem[]>([])
let nextId = 0

const removeToast = (id: number) => {
  toasts.value = toasts.value.filter((item) => item.id !== id)
}

const addToast = (message: string, type: ToastItem['type'], options?: ToastOptions) => {
  const id = ++nextId
  toasts.value.push({ id, message, type, action: options?.action })
  const timer = setTimeout(() => removeToast(id), options?.duration ?? 3000)
  return {
    id,
    cancel: () => {
      clearTimeout(timer)
      removeToast(id)
    },
  }
}

export const toast = {
  success: (message: string, options?: ToastOptions) => addToast(message, 'success', options),
  error: (message: string, options?: ToastOptions) => addToast(message, 'error', options),
  info: (message: string, options?: ToastOptions) => addToast(message, 'info', options),
}

export function useToast() {
  return { toasts: readonly(toasts), removeToast, toast }
}
