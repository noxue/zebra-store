import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifyError, notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, asString, normalizeNumber } from './settingsUtils'

/** 邮件配置 (`settings/smtp`) + 测试邮件. */
export function useSmtpSettings() {
  const submitting = ref(false)
  const testing = ref(false)
  const form = reactive({
    enabled: false,
    host: '',
    port: 587 as number | '',
    username: '',
    password: '',
    has_password: false,
    from: '',
    from_name: '',
    use_tls: true,
    use_ssl: false,
    order_notification_enabled: true,
    verify_code: {
      expire_minutes: 10 as number | '',
      send_interval_seconds: 60 as number | '',
      max_attempts: 5 as number | '',
      length: 6 as number | '',
    },
    test_email: '',
  })

  const load = (raw: unknown) => {
    const smtp = asRecord(raw)
    form.enabled = !!smtp.enabled
    form.host = asString(smtp.host)
    form.port = normalizeNumber(smtp.port, 587)
    form.username = asString(smtp.username)
    form.password = ''
    form.has_password = !!smtp.has_password
    form.from = asString(smtp.from)
    form.from_name = asString(smtp.from_name)
    form.use_tls = !!smtp.use_tls
    form.use_ssl = !!smtp.use_ssl
    form.order_notification_enabled = smtp.order_notification_enabled !== false
    const vc = asRecord(smtp.verify_code)
    form.verify_code.expire_minutes = normalizeNumber(vc.expire_minutes, 10)
    form.verify_code.send_interval_seconds = normalizeNumber(vc.send_interval_seconds, 60)
    form.verify_code.max_attempts = normalizeNumber(vc.max_attempts, 5)
    form.verify_code.length = normalizeNumber(vc.length, 6)
  }

  const reload = async () => {
    try {
      load((await adminAPI.getSMTPSettings()).data)
    } catch {
      /* already notified */
    }
  }

  const save = async () => {
    submitting.value = true
    try {
      const res = await adminAPI.updateSMTPSettings({
        enabled: form.enabled,
        host: form.host,
        port: Number(form.port),
        username: form.username,
        password: form.password,
        from: form.from,
        from_name: form.from_name,
        use_tls: form.use_tls,
        use_ssl: form.use_ssl,
        order_notification_enabled: form.order_notification_enabled,
        verify_code: {
          expire_minutes: Number(form.verify_code.expire_minutes),
          send_interval_seconds: Number(form.verify_code.send_interval_seconds),
          max_attempts: Number(form.verify_code.max_attempts),
          length: Number(form.verify_code.length),
        },
      })
      form.password = ''
      form.has_password = !!asRecord(res.data).has_password || form.has_password
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
      await reload()
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  const sendTest = async () => {
    if (form.test_email.trim() === '') {
      notifyError(tr('admin.settings.smtp.testEmailRequired'))
      return
    }
    testing.value = true
    try {
      await adminAPI.testSMTPSettings({ to_email: form.test_email.trim() })
      notifySuccess(tr('admin.settings.smtp.testSuccess'))
    } catch (err) {
      notifyFailure(err, 'admin.settings.smtp.testFailed')
    } finally {
      testing.value = false
    }
  }

  return { form, submitting, testing, load, save, sendTest }
}

export type SmtpSettingsModel = ReturnType<typeof useSmtpSettings>
