import { defineComponent, onBeforeUnmount, onMounted, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw } from 'lucide-vue-next'
import { captchaAPI } from '@/api/catalog'
import type { CaptchaController } from '@/composables/useCaptcha'
import { Input } from '@/components/ui'

interface TurnstileRenderOptions {
  sitekey: string
  theme?: 'auto' | 'light' | 'dark'
  callback?: (token: string) => void
  'expired-callback'?: () => void
  'error-callback'?: () => void
}
interface TurnstileAPI {
  render: (container: HTMLElement, options: TurnstileRenderOptions) => string
  reset: (widgetId?: string) => void
  remove: (widgetId: string) => void
}
declare global {
  interface Window {
    turnstile?: TurnstileAPI
  }
}

let turnstileScript: Promise<void> | null = null
const loadTurnstile = (): Promise<void> => {
  if (window.turnstile) return Promise.resolve()
  if (!turnstileScript) {
    turnstileScript = new Promise((resolve, reject) => {
      const script = document.createElement('script')
      script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit'
      script.async = true
      script.onload = () => resolve()
      script.onerror = () => reject(new Error('turnstile'))
      document.head.appendChild(script)
    })
  }
  return turnstileScript
}

/** Renders the captcha widget for a `useCaptcha()` controller (nothing when disabled). */
export const CaptchaField = defineComponent({
  name: 'CaptchaField',
  props: { captcha: { type: Object as PropType<CaptchaController>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const image = ref('')
    const loading = ref(false)
    const container = ref<HTMLElement | null>(null)
    let widgetId = ''

    const loadImage = async () => {
      loading.value = true
      try {
        const res = await captchaAPI.image()
        image.value = res.data.image_base64
        props.captcha.setPayload({ captcha_id: res.data.captcha_id, captcha_code: '' })
      } catch {
        image.value = ''
      } finally {
        loading.value = false
      }
    }

    const renderTurnstile = async () => {
      try {
        await loadTurnstile()
      } catch {
        return
      }
      if (!window.turnstile || !container.value) return
      if (widgetId) window.turnstile.remove(widgetId)
      container.value.innerHTML = ''
      widgetId = window.turnstile.render(container.value, {
        sitekey: props.captcha.siteKey.value,
        theme: 'auto',
        callback: (token) => {
          props.captcha.setPayload({ turnstile_token: token || '' })
        },
        'expired-callback': () => {
          props.captcha.setPayload({})
        },
        'error-callback': () => {
          props.captcha.setPayload({})
        },
      })
    }

    const refresh = () => {
      if (props.captcha.provider.value === 'image') void loadImage()
      else if (props.captcha.provider.value === 'turnstile' && window.turnstile && widgetId) window.turnstile.reset(widgetId)
    }

    const init = () => {
      if (!props.captcha.enabled.value) return
      if (props.captcha.provider.value === 'image') void loadImage()
      if (props.captcha.provider.value === 'turnstile') void renderTurnstile()
    }

    onMounted(() => {
      props.captcha.bindRefresher(refresh)
      init()
    })
    watch(() => [props.captcha.enabled.value, props.captcha.provider.value], init)
    onBeforeUnmount(() => {
      props.captcha.bindRefresher(null)
      if (widgetId && window.turnstile) window.turnstile.remove(widgetId)
    })

    return () => {
      if (!props.captcha.enabled.value) return null
      if (props.captcha.provider.value === 'turnstile') return <div ref={container} class="min-h-[65px]" />
      return (
        <div class="flex items-center gap-3">
          <Input
            class="flex-1"
            modelValue={props.captcha.payload.value.captcha_code || ''}
            placeholder={t('auth.common.captchaCodePlaceholder')}
            autocomplete="off"
            onUpdate:modelValue={(v: string) => {
              props.captcha.setPayload({ ...props.captcha.payload.value, captcha_code: v })
            }}
          />
          <button
            type="button"
            class="relative flex h-11 w-32 shrink-0 items-center justify-center overflow-hidden rounded-zs border border-line bg-surface-strong"
            title={t('auth.common.captchaRefresh')}
            onClick={() => void loadImage()}
          >
            {image.value ? <img src={image.value} alt="captcha" class="size-full object-contain" /> : <RefreshCw class={['size-4 text-muted', loading.value && 'zs-spin']} />}
          </button>
        </div>
      )
    }
  },
})
