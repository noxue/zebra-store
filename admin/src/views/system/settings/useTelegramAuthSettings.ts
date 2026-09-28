import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, asString, normalizeNumber } from './settingsUtils'

/** Telegram 登录 (`settings/telegram-auth`) — bot_token / client_secret are write-only. */
export function useTelegramAuthSettings() {
  const submitting = ref(false)
  const form = reactive({
    enabled: false,
    bot_username: '',
    bot_token: '',
    has_bot_token: false,
    mini_app_url: '',
    login_expire_seconds: 300 as number | '',
    replay_ttl_seconds: 300 as number | '',
    client_secret: '',
    has_client_secret: false,
    oidc_redirect_uri: '',
    mode: '',
  })

  const load = (raw: unknown) => {
    const d = asRecord(raw)
    form.enabled = !!d.enabled
    form.bot_username = asString(d.bot_username)
    form.bot_token = ''
    form.has_bot_token = !!d.has_bot_token
    form.mini_app_url = asString(d.mini_app_url)
    form.login_expire_seconds = normalizeNumber(d.login_expire_seconds, 300)
    form.replay_ttl_seconds = normalizeNumber(d.replay_ttl_seconds, 300)
    form.client_secret = ''
    form.has_client_secret = !!d.has_client_secret
    form.oidc_redirect_uri = asString(d.oidc_redirect_uri)
    form.mode = asString(d.mode)
  }

  const save = async () => {
    submitting.value = true
    try {
      const payload: Record<string, unknown> = {
        enabled: form.enabled,
        bot_username: form.bot_username,
        mini_app_url: form.mini_app_url,
        login_expire_seconds: Number(form.login_expire_seconds),
        replay_ttl_seconds: Number(form.replay_ttl_seconds),
        oidc_redirect_uri: form.oidc_redirect_uri.trim(),
      }
      if (form.bot_token.trim() !== '') payload.bot_token = form.bot_token.trim()
      if (form.client_secret.trim() !== '') payload.client_secret = form.client_secret.trim()
      const data = asRecord((await adminAPI.updateTelegramAuthSettings(payload)).data)
      form.bot_token = ''
      form.has_bot_token = !!data.has_bot_token || form.has_bot_token
      form.client_secret = ''
      form.has_client_secret = !!data.has_client_secret || form.has_client_secret
      form.mode = asString(data.mode, form.mode)
      form.oidc_redirect_uri = typeof data.oidc_redirect_uri === 'string' ? data.oidc_redirect_uri : form.oidc_redirect_uri
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type TelegramAuthSettingsModel = ReturnType<typeof useTelegramAuthSettings>
