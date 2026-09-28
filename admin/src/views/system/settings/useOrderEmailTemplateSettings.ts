import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { orderEmailSceneKeys } from '@/utils/orderEmailTemplates'
import { notifyFailure, tr } from './common'
import { SUPPORTED_LANGS, asRecord, asString, createLocalizedField, type LangCode } from './settingsUtils'

export type OrderEmailScene = (typeof orderEmailSceneKeys)[number]
type SubjectBody = { subject: string; body: string }
type SceneTemplate = Record<LangCode, SubjectBody>

export const ORDER_EMAIL_VARIABLES = ['order_no', 'status', 'amount', 'refund_amount', 'refund_reason', 'currency', 'fulfillment_info', 'site_name', 'site_url'] as const

const createScene = (): SceneTemplate => ({
  'zh-CN': { subject: '', body: '' },
  'zh-TW': { subject: '', body: '' },
  'en-US': { subject: '', body: '' },
})

const createTemplates = () =>
  Object.fromEntries(orderEmailSceneKeys.map((k) => [k, createScene()])) as Record<OrderEmailScene, SceneTemplate>

/** 订单邮件模板 (`settings/order-email-template`) + 恢复默认. */
export function useOrderEmailTemplateSettings() {
  const submitting = ref(false)
  const resetting = ref(false)
  const currentScene = ref<OrderEmailScene>('default')
  const form = reactive({ templates: createTemplates(), guest_tip: createLocalizedField() })

  const load = (raw: unknown) => {
    const d = asRecord(raw)
    const templates = asRecord(d.templates)
    orderEmailSceneKeys.forEach((key) => {
      const scene = asRecord(templates[key])
      SUPPORTED_LANGS.forEach((lang) => {
        const item = asRecord(scene[lang])
        form.templates[key][lang].subject = asString(item.subject)
        form.templates[key][lang].body = asString(item.body)
      })
    })
    const tip = asRecord(d.guest_tip)
    SUPPORTED_LANGS.forEach((lang) => (form.guest_tip[lang] = asString(tip[lang])))
  }

  const reload = async () => {
    try {
      load((await adminAPI.getOrderEmailTemplateSettings()).data)
    } catch {
      /* already notified */
    }
  }

  const save = async () => {
    submitting.value = true
    try {
      const templates: Record<string, unknown> = {}
      orderEmailSceneKeys.forEach((key) => {
        const scene: Record<string, SubjectBody> = {}
        SUPPORTED_LANGS.forEach((lang) => {
          scene[lang] = { subject: form.templates[key][lang].subject, body: form.templates[key][lang].body }
        })
        templates[key] = scene
      })
      await adminAPI.updateOrderEmailTemplateSettings({ templates, guest_tip: { ...form.guest_tip } })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
      await reload()
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  const resetToDefault = async () => {
    if (!(await confirmAction({ description: tr('admin.settings.orderEmailTemplate.resetConfirm'), variant: 'destructive' }))) return
    resetting.value = true
    try {
      await adminAPI.resetOrderEmailTemplateSettings()
      notifySuccess(tr('admin.settings.orderEmailTemplate.resetSuccess'))
      await reload()
    } catch (err) {
      notifyFailure(err)
    } finally {
      resetting.value = false
    }
  }

  return { form, submitting, resetting, currentScene, load, save, resetToDefault }
}

export type OrderEmailTemplateSettingsModel = ReturnType<typeof useOrderEmailTemplateSettings>
