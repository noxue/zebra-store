import { defineComponent, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import QRCode from 'qrcode'
import { Copy, ShieldCheck } from 'lucide-vue-next'
import { Button, Dialog, FormField, Input, Loader } from '@/components/ui'
import { adminAPI } from '@/api/admin'
import { ApiError } from '@/api/client'
import type { SetupTwoFAResponse } from '@/api/types'
import { copyText } from '@/utils/clipboard'
import { notifyError, notifySuccess } from '@/utils/notify'
import { isTotpCode } from './useSecurity'

/** Bind an authenticator app: QR + secret, then confirm with a 6-digit code (`2fa/setup` → `2fa/enable`). */
export const Setup2FAModal = defineComponent({
  name: 'Setup2FAModal',
  props: { modelValue: { type: Boolean, default: false } },
  emits: { 'update:modelValue': (_v: boolean) => true, enabled: (_codes: string[]) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const setupData = ref<SetupTwoFAResponse | null>(null)
    const qrDataUrl = ref('')
    const code = ref('')
    const loading = ref(false)
    const submitting = ref(false)

    const close = () => emit('update:modelValue', false)

    async function init() {
      loading.value = true
      setupData.value = null
      qrDataUrl.value = ''
      code.value = ''
      try {
        const data = (await adminAPI.setup2FA()).data
        if (!data) throw new Error('empty')
        setupData.value = data
        qrDataUrl.value = await QRCode.toDataURL(data.otpauth_url, { margin: 1, width: 240 })
      } catch (err: unknown) {
        // API errors are already toasted by the client; only QR / empty payload failures reach here un-notified.
        if (!(err instanceof ApiError)) notifyError(t('admin.twofa.errors.setupFailed'))
        close()
      } finally {
        loading.value = false
      }
    }

    async function submit() {
      if (!isTotpCode(code.value)) {
        notifyError(t('admin.twofa.errors.codeFormat'))
        return
      }
      submitting.value = true
      try {
        const res = await adminAPI.enable2FA({ code: code.value.trim() })
        emit('enabled', res.data?.recovery_codes ?? [])
        close()
      } catch {
        /* already notified */
      } finally {
        submitting.value = false
      }
    }

    async function copySecret() {
      if (!setupData.value?.secret) return
      try {
        await copyText(setupData.value.secret)
        notifySuccess(t('admin.twofa.recovery.copied'))
      } catch {
        notifyError(t('admin.common.copyFailed'))
      }
    }

    watch(
      () => props.modelValue,
      (open) => open && void init(),
      { immediate: true },
    )

    return () => (
      <Dialog
        modelValue={props.modelValue}
        onUpdate:modelValue={(v) => emit('update:modelValue', v)}
        title={t('admin.twofa.setup.title')}
        description={t('admin.twofa.setup.description')}
        size="md"
      >
        {{
          default: () =>
            loading.value ? (
              <div class="flex flex-col items-center gap-3 py-8 text-sm text-muted">
                <Loader />
                {t('admin.twofa.setup.generating')}
              </div>
            ) : setupData.value ? (
              <div class="space-y-4">
                <div class="flex justify-center">
                  <div class="rounded-zs-lg border border-line bg-surface-solid p-3 shadow-zs-sm">
                    {/* the generated PNG carries its own white quiet zone, so it stays scannable in dark mode */}
                    <img src={qrDataUrl.value} alt="QR" class="h-56 w-56 rounded-zs-sm" />
                  </div>
                </div>
                <FormField label={t('admin.twofa.setup.secretLabel')}>
                  <div class="flex gap-2">
                    <code class="flex-1 break-all rounded-zs-sm bg-surface-muted px-3 py-2 font-mono text-xs text-fg">{setupData.value.secret}</code>
                    <Button size="sm" onClick={copySecret}>
                      <Copy class="h-3.5 w-3.5" />
                      {t('admin.twofa.setup.copy')}
                    </Button>
                  </div>
                </FormField>
                <FormField label={t('admin.twofa.setup.codeLabel')}>
                  <Input
                    id="totp-setup-code"
                    v-model={code.value}
                    maxlength={6}
                    mono
                    placeholder="123456"
                    autocomplete="one-time-code"
                    inputClass="tracking-[0.4em] text-center text-base"
                    onEnter={submit}
                  />
                </FormField>
              </div>
            ) : null,
          footer: () => (
            <>
              <Button disabled={submitting.value} onClick={close}>
                {t('admin.common.cancel')}
              </Button>
              <Button variant="primary" loading={submitting.value} disabled={loading.value || !isTotpCode(code.value)} onClick={submit}>
                <ShieldCheck class="h-4 w-4" />
                {submitting.value ? t('admin.common.submitting') : t('admin.twofa.setup.confirm')}
              </Button>
            </>
          ),
        }}
      </Dialog>
    )
  },
})
