import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Save, UserCircle } from 'lucide-vue-next'
import { Button, Card, Field, Input, Select } from '@/components/ui'
import { useProfilePanel } from '@/composables/personal/useProfilePanel'
import { PanelAlertBox, PanelHeading } from './PanelParts'

/** Nickname / language preference. */
export const ProfilePanel = defineComponent({
  name: 'ProfilePanel',
  setup() {
    const { t } = useI18n()
    const p = useProfilePanel()
    return () => (
      <Card>
        <PanelHeading title={t('personalCenter.profile.title')} description={t('personalCenter.profile.subtitle')} icon={UserCircle} />
        <PanelAlertBox alert={p.alert.value} />
        <form
          class="space-y-5"
          onSubmit={(e: Event) => {
            e.preventDefault()
            void p.save()
          }}
        >
          <Field label={t('personalCenter.profile.emailLabel')}>
            <Input modelValue={p.store.profile?.email || ''} readonly disabled />
          </Field>
          <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
            <Field label={t('personalCenter.profile.nicknameLabel')}>
              <Input v-model={p.form.nickname} placeholder={t('personalCenter.profile.nicknamePlaceholder')} maxlength={64} />
            </Field>
            <Field label={t('personalCenter.profile.localeLabel')}>
              <Select
                modelValue={p.form.locale}
                options={p.localeOptions}
                onUpdate:modelValue={(v: string | number) => {
                  p.form.locale = String(v)
                }}
              />
            </Field>
          </div>
          <div class="zs-divider" />
          <Button type="submit" loading={p.store.savingProfile}>
            <Save class="size-4" />
            {p.store.savingProfile ? t('personalCenter.profile.saving') : t('personalCenter.profile.save')}
          </Button>
        </form>
      </Card>
    )
  },
})
