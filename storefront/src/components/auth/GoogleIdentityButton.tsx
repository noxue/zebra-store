import { defineComponent, nextTick, onBeforeUnmount, onMounted, ref, watch, type PropType } from 'vue'
import { LoaderCircle } from 'lucide-vue-next'
import {
  createGoogleButtonConfiguration,
  createGoogleIdentityConfiguration,
  loadGoogleIdentityScript,
  resolveGoogleButtonWidth,
  type GoogleAccountsID,
} from '@/utils/auth/googleIdentity'
import { resolveGoogleRedirectIntentRefreshDelay, type GoogleRedirectPreparedIntent } from '@/utils/auth/googleRedirect'

/**
 * Google Identity Services button. Popup mode emits `credential`; redirect mode
 * (iOS) prepares a server state via `prepareRedirect` and lets GIS post to the backend.
 */
export const GoogleIdentityButton = defineComponent({
  name: 'GoogleIdentityButton',
  props: {
    clientId: { type: String, required: true },
    locale: { type: String, default: '' },
    text: { type: String as PropType<'signin_with' | 'continue_with'>, default: 'signin_with' },
    disabled: Boolean,
    loadingLabel: { type: String, default: 'Loading Google sign-in' },
    uxMode: { type: String as PropType<'popup' | 'redirect'>, default: 'popup' },
    loginUri: { type: String, default: '' },
    prepareRedirect: { type: Function as PropType<() => Promise<GoogleRedirectPreparedIntent>>, default: undefined },
  },
  emits: { credential: (_c: string) => true, error: (_e: Error) => true },
  setup(props, { emit }) {
    const container = ref<HTMLDivElement | null>(null)
    const loading = ref(true)
    let accounts: GoogleAccountsID | null = null
    let version = 0
    let active = false
    let redirectState = ''
    let refreshTimer: ReturnType<typeof setTimeout> | undefined
    let observer: ResizeObserver | null = null
    let lastWidth = 0

    const toError = (e: unknown) => (e instanceof Error ? e : new Error(String(e)))

    const render = () => {
      const el = container.value
      if (!el || !accounts || !active || !el.isConnected) return
      const width = resolveGoogleButtonWidth(el.getBoundingClientRect().width)
      try {
        el.replaceChildren()
        accounts.renderButton(el, createGoogleButtonConfiguration({ locale: props.locale, text: props.text, width, state: redirectState }))
        lastWidth = width || 0
      } catch (e) {
        emit('error', toError(e))
      }
    }

    const init = async () => {
      const current = ++version
      clearTimeout(refreshTimer)
      accounts = null
      redirectState = ''
      container.value?.replaceChildren()
      if (!props.clientId.trim()) {
        loading.value = false
        return
      }
      loading.value = true
      try {
        let issuedAt = 0
        if (props.uxMode === 'redirect') {
          if (!props.prepareRedirect) throw new Error('Google redirect intent preparation is unavailable')
          const prepared = await props.prepareRedirect()
          if (!active || current !== version) return
          redirectState = prepared.state
          issuedAt = prepared.issuedAt
        }
        const id = await loadGoogleIdentityScript()
        id.initialize(
          createGoogleIdentityConfiguration(
            props.clientId,
            (credential) => {
              if (active && current === version && !props.disabled) emit('credential', credential)
            },
            () => {
              if (active && current === version) emit('error', new Error('Google credential is missing'))
            },
            { uxMode: props.uxMode, loginUri: props.loginUri },
          ),
        )
        if (!active || current !== version) return
        accounts = id
        void nextTick(render)
        if (issuedAt > 0) {
          refreshTimer = setTimeout(() => {
            if (active && current === version) void init()
          }, resolveGoogleRedirectIntentRefreshDelay(issuedAt))
        }
      } catch (e) {
        if (active && current === version) emit('error', toError(e))
      } finally {
        if (active && current === version) loading.value = false
      }
    }

    onMounted(() => {
      active = true
      void init()
      if (typeof ResizeObserver !== 'undefined' && container.value) {
        observer = new ResizeObserver((entries) => {
          const width = resolveGoogleButtonWidth(entries[0]?.contentRect.width)
          if (width && Math.abs(width - lastWidth) >= 8) void nextTick(render)
        })
        observer.observe(container.value)
      }
    })
    watch(() => [props.clientId, props.uxMode, props.loginUri], () => void init())
    watch(() => [props.locale, props.text], () => void nextTick(render))
    onBeforeUnmount(() => {
      active = false
      version += 1
      clearTimeout(refreshTimer)
      observer?.disconnect()
      accounts?.cancel?.()
      accounts = null
    })

    return () => (
      <div class={['relative flex min-h-11 w-full justify-center overflow-hidden', props.disabled && 'pointer-events-none opacity-60']} aria-disabled={props.disabled}>
        <div ref={container} class="flex min-h-11 w-full justify-center" />
        {loading.value && (
          <div class="absolute inset-0 flex items-center justify-center rounded-full border border-line bg-surface-strong text-muted" role="status">
            <LoaderCircle class="size-4 zs-spin" />
            <span class="sr-only">{props.loadingLabel}</span>
          </div>
        )}
      </div>
    )
  },
})
