import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, asString, normalizeNumber } from './settingsUtils'

export const DEFAULT_TURNSTILE_VERIFY_URL = 'https://challenges.cloudflare.com/turnstile/v0/siteverify'
export const CAPTCHA_SCENES = ['login', 'register_send_code', 'reset_send_code', 'guest_create_order', 'gift_card_redeem'] as const
export type CaptchaScene = (typeof CAPTCHA_SCENES)[number]

/** 验证码配置 (`settings/captcha`). */
export function useCaptchaSettings() {
  const submitting = ref(false)
  const form = reactive({
    provider: 'none',
    scenes: { login: false, register_send_code: false, reset_send_code: false, guest_create_order: false, gift_card_redeem: false } as Record<CaptchaScene, boolean>,
    image: {
      length: 5 as number | '',
      width: 240 as number | '',
      height: 80 as number | '',
      noise_count: 2 as number | '',
      show_line: 2 as number | '',
      expire_seconds: 300 as number | '',
      max_store: 10240 as number | '',
    },
    turnstile: { site_key: '', secret_key: '', has_secret: false, verify_url: DEFAULT_TURNSTILE_VERIFY_URL, timeout_ms: 2000 as number | '' },
  })

  const load = (raw: unknown) => {
    const c = asRecord(raw)
    form.provider = asString(c.provider, 'none')
    const scenes = asRecord(c.scenes)
    CAPTCHA_SCENES.forEach((k) => (form.scenes[k] = !!scenes[k]))
    const img = asRecord(c.image)
    form.image.length = normalizeNumber(img.length, 5)
    form.image.width = normalizeNumber(img.width, 240)
    form.image.height = normalizeNumber(img.height, 80)
    form.image.noise_count = normalizeNumber(img.noise_count, 2)
    form.image.show_line = normalizeNumber(img.show_line, 2)
    form.image.expire_seconds = normalizeNumber(img.expire_seconds, 300)
    form.image.max_store = normalizeNumber(img.max_store, 10240)
    const ts = asRecord(c.turnstile)
    form.turnstile.site_key = asString(ts.site_key)
    form.turnstile.secret_key = ''
    form.turnstile.has_secret = !!ts.has_secret
    form.turnstile.verify_url = asString(ts.verify_url, DEFAULT_TURNSTILE_VERIFY_URL)
    form.turnstile.timeout_ms = normalizeNumber(ts.timeout_ms, 2000)
  }

  const save = async () => {
    submitting.value = true
    try {
      const turnstile: Record<string, unknown> = {
        site_key: form.turnstile.site_key,
        verify_url: form.turnstile.verify_url,
        timeout_ms: Number(form.turnstile.timeout_ms),
      }
      if (form.turnstile.secret_key.trim() !== '') turnstile.secret_key = form.turnstile.secret_key.trim()
      const res = await adminAPI.updateCaptchaSettings({
        provider: form.provider,
        scenes: { ...form.scenes },
        image: {
          length: Number(form.image.length),
          width: Number(form.image.width),
          height: Number(form.image.height),
          noise_count: Number(form.image.noise_count),
          show_line: Number(form.image.show_line),
          expire_seconds: Number(form.image.expire_seconds),
          max_store: Number(form.image.max_store),
        },
        turnstile,
      })
      form.turnstile.secret_key = ''
      form.turnstile.has_secret = !!asRecord(asRecord(res.data).turnstile).has_secret || form.turnstile.has_secret
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
      try {
        load((await adminAPI.getCaptchaSettings()).data)
      } catch {
        /* already notified */
      }
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type CaptchaSettingsModel = ReturnType<typeof useCaptchaSettings>
