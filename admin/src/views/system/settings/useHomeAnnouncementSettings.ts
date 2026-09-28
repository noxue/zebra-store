import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { toDateTimeLocal, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { SUPPORTED_LANGS, asRecord, asString, createLocalizedField, isRichTextEmpty, normalizeLocalizedField } from './settingsUtils'

export type AnnouncementType = 'normal' | 'info' | 'warning'

/** 首页公告 (key `home_announcement`). */
export function useHomeAnnouncementSettings() {
  const submitting = ref(false)
  const form = reactive({
    enabled: false,
    type: 'normal' as AnnouncementType,
    title: createLocalizedField(),
    content: createLocalizedField(),
    start_at: '',
    end_at: '',
  })

  const load = (raw: unknown) => {
    const d = asRecord(raw)
    form.enabled = d.enabled === true
    form.type = d.type === 'info' || d.type === 'warning' ? d.type : 'normal'
    form.title = normalizeLocalizedField(d.title)
    form.content = normalizeLocalizedField(d.content)
    form.start_at = toDateTimeLocal(asString(d.start_at))
    form.end_at = toDateTimeLocal(asString(d.end_at))
  }

  const save = async () => {
    submitting.value = true
    try {
      const content = createLocalizedField()
      SUPPORTED_LANGS.forEach((lang) => {
        content[lang] = isRichTextEmpty(form.content[lang]) ? '' : form.content[lang]
      })
      await adminAPI.updateSettings({
        key: 'home_announcement',
        value: {
          enabled: form.enabled,
          type: form.type,
          title: { ...form.title },
          content,
          start_at: toRFC3339(form.start_at) || '',
          end_at: toRFC3339(form.end_at) || '',
        },
      })
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type HomeAnnouncementSettingsModel = ReturnType<typeof useHomeAnnouncementSettings>
