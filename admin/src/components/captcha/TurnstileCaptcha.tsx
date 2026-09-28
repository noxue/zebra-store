import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue'

interface TurnstileRenderOptions {
  sitekey: string
  theme?: 'auto' | 'light' | 'dark'
  callback?: (token: string) => void
  'expired-callback'?: () => void
  'error-callback'?: () => void
}

interface TurnstileAPI {
  render: (container: HTMLElement, options: TurnstileRenderOptions) => string
  reset: (widgetID?: string) => void
  remove: (widgetID: string) => void
}

declare global {
  interface Window {
    turnstile?: TurnstileAPI
  }
}

let scriptPromise: Promise<void> | null = null
const loadScript = () => {
  if (window.turnstile) return Promise.resolve()
  scriptPromise ??= new Promise<void>((resolve, reject) => {
    const script = document.createElement('script')
    script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit'
    script.async = true
    script.onload = () => resolve()
    script.onerror = () => reject(new Error('failed to load turnstile script'))
    document.head.appendChild(script)
  })
  return scriptPromise
}

export const TurnstileCaptcha = defineComponent({
  name: 'TurnstileCaptcha',
  props: { modelValue: { type: String, default: '' }, siteKey: { type: String, required: true } },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit, expose }) {
    const container = ref<HTMLElement | null>(null)
    let widget = ''
    const render = async () => {
      if (!props.siteKey || !container.value) return emit('update:modelValue', '')
      await loadScript()
      if (!window.turnstile || !container.value) return
      if (widget) window.turnstile.remove(widget)
      container.value.innerHTML = ''
      widget = window.turnstile.render(container.value, {
        sitekey: props.siteKey,
        theme: 'auto',
        callback: (token) => emit('update:modelValue', token || ''),
        'expired-callback': () => emit('update:modelValue', ''),
        'error-callback': () => emit('update:modelValue', ''),
      })
    }
    const reset = () => {
      if (window.turnstile && widget) window.turnstile.reset(widget)
      emit('update:modelValue', '')
    }
    expose({ reset })
    watch(() => props.siteKey, () => render().catch(() => emit('update:modelValue', '')))
    onMounted(() => render().catch(() => emit('update:modelValue', '')))
    onBeforeUnmount(() => {
      if (window.turnstile && widget) window.turnstile.remove(widget)
    })
    return () => <div ref={container} />
  },
})

export default TurnstileCaptcha
