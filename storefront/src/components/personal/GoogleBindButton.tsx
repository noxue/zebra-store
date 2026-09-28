import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue'

interface GoogleCredentialResponse {
  credential?: string
}
interface GoogleIdApi {
  initialize: (options: { client_id: string; callback: (res: GoogleCredentialResponse) => void; ux_mode?: 'popup' | 'redirect' }) => void
  renderButton: (el: HTMLElement, options: Record<string, string | number>) => void
}
declare global {
  interface Window {
    google?: { accounts?: { id?: GoogleIdApi } }
  }
}

let gisScript: Promise<void> | null = null
const loadGis = (): Promise<void> => {
  if (window.google?.accounts?.id) return Promise.resolve()
  if (!gisScript) {
    gisScript = new Promise((resolve, reject) => {
      const s = document.createElement('script')
      s.src = 'https://accounts.google.com/gsi/client'
      s.async = true
      s.onload = () => resolve()
      s.onerror = () => {
        gisScript = null
        reject(new Error('gis'))
      }
      document.head.appendChild(s)
    })
  }
  return gisScript
}

/** Google Identity Services button (popup mode) emitting the ID-token credential. */
export const GoogleBindButton = defineComponent({
  name: 'GoogleBindButton',
  props: {
    clientId: { type: String, required: true },
    locale: { type: String, default: 'zh-CN' },
  },
  emits: { credential: (_c: string) => true, scriptError: () => true },
  setup(props, { emit }) {
    const el = ref<HTMLElement | null>(null)
    let alive = true
    onMounted(async () => {
      try {
        await loadGis()
      } catch {
        emit('scriptError')
        return
      }
      const api = window.google?.accounts?.id
      if (!alive || !api || !el.value) return
      api.initialize({ client_id: props.clientId, ux_mode: 'popup', callback: (res) => emit('credential', String(res.credential || '')) })
      api.renderButton(el.value, { theme: 'outline', size: 'large', shape: 'pill', text: 'continue_with', locale: props.locale })
    })
    onBeforeUnmount(() => {
      alive = false
    })
    return () => <div ref={el} class="min-h-[44px]" />
  },
})
