import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue'

/** Official Telegram login widget; calls `window[callbackName](user)` on auth. */
export const TelegramLoginWidget = defineComponent({
  name: 'TelegramLoginWidget',
  props: {
    botUsername: { type: String, required: true },
    callbackName: { type: String, required: true },
  },
  emits: { error: () => true },
  setup(props, { emit }) {
    const container = ref<HTMLDivElement | null>(null)
    const render = () => {
      const el = container.value
      if (!el) return
      el.innerHTML = ''
      if (!props.botUsername) return
      const script = document.createElement('script')
      script.async = true
      script.src = 'https://telegram.org/js/telegram-widget.js?22'
      script.setAttribute('data-telegram-login', props.botUsername)
      script.setAttribute('data-size', 'large')
      script.setAttribute('data-userpic', 'false')
      script.setAttribute('data-radius', '20')
      script.setAttribute('data-request-access', 'write')
      script.setAttribute('data-onauth', `${props.callbackName}(user)`)
      script.onerror = () => emit('error')
      el.appendChild(script)
    }
    onMounted(render)
    watch(() => props.botUsername, render)
    onBeforeUnmount(() => {
      if (container.value) container.value.innerHTML = ''
    })
    return () => <div ref={container} class="flex min-h-11 justify-center" />
  },
})
