import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, LayoutGrid, List, ShieldCheck, Sun } from 'lucide-vue-next'
import { Card, cn, type IconComponent } from '@/components/ui'
import type { SiteSettingsModel } from './useSiteSettings'
import ThemeSection from './ThemeSection'

interface ChoiceCard {
  value: string
  title: string
  desc: string
  icon: IconComponent
}

/** Big visual radio cards (the original's `border-2` template/layout pickers). */
const ChoiceCards = defineComponent({
  name: 'SettingsChoiceCards',
  props: {
    modelValue: { type: String, required: true },
    options: { type: Array as PropType<ChoiceCard[]>, required: true },
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    return () => (
      <div role="radiogroup" class="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {props.options.map((opt) => {
          const active = props.modelValue === opt.value
          const Icon = opt.icon
          return (
            <button
              key={opt.value}
              type="button"
              role="radio"
              aria-checked={active}
              data-value={opt.value}
              onClick={() => emit('update:modelValue', opt.value)}
              class={cn(
                'relative flex flex-col items-center gap-3 rounded-zs-lg border-2 p-6 transition-all',
                active ? 'border-primary bg-primary-soft shadow-glow' : 'border-line bg-surface-strong hover:border-line-strong',
              )}
            >
              <span class={cn('flex h-16 w-16 items-center justify-center rounded-zs', active ? 'zs-gradient-bg text-on-primary' : 'bg-surface-muted text-muted')}>
                <Icon class="h-8 w-8" stroke-width={1.5} />
              </span>
              <span class="text-center">
                <span class={cn('block font-semibold', active ? 'text-primary' : 'text-fg')}>{opt.title}</span>
                <span class="mt-1 block text-xs text-muted">{opt.desc}</span>
              </span>
              {active && <CheckCircle2 class="absolute right-3 top-3 h-5 w-5 text-primary" />}
            </button>
          )
        })}
      </div>
    )
  },
})

export default defineComponent({
  name: 'SettingsTemplateTab',
  props: { model: { type: Object as PropType<SiteSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const f = props.model.form
      return (
        <div class="space-y-6">
          <Card title={t('admin.settings.template.storefrontTitle')} description={t('admin.settings.template.storefrontSubtitle')}>
            <ChoiceCards
              modelValue={f.storefront_template}
              onUpdate:modelValue={(v: string) => (f.storefront_template = v === 'vault' ? 'vault' : 'classic')}
              options={[
                { value: 'classic', title: t('admin.settings.template.classicMode'), desc: t('admin.settings.template.classicModeDesc'), icon: Sun },
                { value: 'vault', title: t('admin.settings.template.vaultMode'), desc: t('admin.settings.template.vaultModeDesc'), icon: ShieldCheck },
              ]}
            />
          </Card>
          <Card title={t('admin.settings.template.layoutTitle')} description={t('admin.settings.template.layoutSubtitle')}>
            <ChoiceCards
              modelValue={f.template_mode}
              onUpdate:modelValue={(v: string) => (f.template_mode = v === 'list' ? 'list' : 'card')}
              options={[
                { value: 'card', title: t('admin.settings.template.cardMode'), desc: t('admin.settings.template.cardModeDesc'), icon: LayoutGrid },
                { value: 'list', title: t('admin.settings.template.listMode'), desc: t('admin.settings.template.listModeDesc'), icon: List },
              ]}
            />
          </Card>
          <ThemeSection theme={f.theme} onResetColors={props.model.resetThemeColors} />
        </div>
      )
    }
  },
})
