import { computed, defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Palette, RotateCcw } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Mascot, RadioGroup, cn } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { getImageUrl } from '@/utils/image'
import { DEFAULT_THEME_COLORS, isHexColor, toColorInputValue, type ThemeForm, type ThemeMode } from './settingsUtils'
import { ToggleRow } from './SettingsUi'

type ColorKey = 'primary_color' | 'secondary_color' | 'accent_color'

/** Zebra Store addition: edits `site_config.theme` (colours, images, effects, default colour mode) with a live preview. */
export default defineComponent({
  name: 'SettingsThemeSection',
  props: {
    theme: { type: Object as PropType<ThemeForm>, required: true },
  },
  emits: { resetColors: () => true },
  setup(props, { emit }) {
    const { t } = useI18n()

    const colorFields: { key: ColorKey; label: string }[] = [
      { key: 'primary_color', label: 'admin.themeSettings.primaryColor' },
      { key: 'secondary_color', label: 'admin.themeSettings.secondaryColor' },
      { key: 'accent_color', label: 'admin.themeSettings.accentColor' },
    ]

    const effective = (key: ColorKey) => toColorInputValue(props.theme[key], DEFAULT_THEME_COLORS[key])

    /** Scoped CSS-variable overrides so the preview (and the Mascot inside it) uses the edited colours. */
    const previewVars = computed(() => ({
      '--zs-primary': effective('primary_color'),
      '--zs-secondary': effective('secondary_color'),
      '--zs-accent': effective('accent_color'),
    }))
    const gradient = computed(
      () => `linear-gradient(135deg, ${effective('primary_color')}, ${effective('secondary_color')} 55%, ${effective('accent_color')})`,
    )

    const renderColor = (key: ColorKey, label: string) => {
      const value = props.theme[key]
      const invalid = value.trim() !== '' && !isHexColor(value)
      return (
        <FormField key={key} label={t(label)} error={invalid ? t('admin.systemSettings.invalidColor') : undefined}>
          <div class="flex items-center gap-2">
            <input
              type="color"
              aria-label={t(label)}
              data-color={key}
              value={effective(key)}
              class="h-9 w-12 shrink-0 cursor-pointer rounded-zs-sm border border-line-strong bg-surface-strong p-1"
              onInput={(e: Event) => (props.theme[key] = (e.target as HTMLInputElement).value)}
            />
            <Input v-model={props.theme[key]} mono maxlength={7} placeholder={DEFAULT_THEME_COLORS[key]} />
          </div>
        </FormField>
      )
    }

    const modeOptions = computed(() => [
      { label: t('admin.themeSettings.modeSystem'), value: 'system' },
      { label: t('admin.themeSettings.modeLight'), value: 'light' },
      { label: t('admin.themeSettings.modeDark'), value: 'dark' },
    ])

    return () => {
      const th = props.theme
      const bg = th.background_image ? getImageUrl(th.background_image) : ''
      const mascot = th.mascot_image ? getImageUrl(th.mascot_image) : ''
      return (
        <Card description={t('admin.themeSettings.description')}>
          {{
            title: () => (
              <>
                <Palette class="h-4 w-4 text-primary" />
                {t('admin.themeSettings.title')}
              </>
            ),
            extra: () => (
              <Button size="sm" onClick={() => emit('resetColors')}>
                <RotateCcw class="h-3.5 w-3.5" />
                {t('admin.themeSettings.resetColors')}
              </Button>
            ),
            default: () => (
              <div class="grid grid-cols-1 gap-6 xl:grid-cols-[minmax(0,1fr)_280px]">
                <div class="space-y-6">
                  <div class="grid grid-cols-1 gap-5 md:grid-cols-3">{colorFields.map((c) => renderColor(c.key, c.label))}</div>

                  <div class="grid grid-cols-1 gap-5 md:grid-cols-3">
                    <FormField label={t('admin.themeSettings.backgroundImage')}>
                      <MediaPicker v-model={th.background_image} scene="common" size="sm" />
                    </FormField>
                    <FormField label={t('admin.themeSettings.mascotImage')} hint={t('admin.themeSettings.mascotHint')}>
                      <MediaPicker v-model={th.mascot_image} scene="common" size="sm" />
                    </FormField>
                    <FormField label={t('admin.themeSettings.loginBackground')}>
                      <MediaPicker v-model={th.login_background} scene="common" size="sm" />
                    </FormField>
                  </div>

                  <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
                    <ToggleRow v-model={th.effects.sakura} label={t('admin.themeSettings.effectSakura')} />
                    <ToggleRow v-model={th.effects.sparkle} label={t('admin.themeSettings.effectSparkle')} />
                  </div>

                  <FormField label={t('admin.themeSettings.defaultMode')}>
                    <RadioGroup
                      modelValue={th.default_mode}
                      options={modeOptions.value}
                      onUpdate:modelValue={(v: string | number) => (th.default_mode = String(v) as ThemeMode)}
                    />
                  </FormField>
                </div>

                <div class="space-y-2" style={previewVars.value}>
                  <p class="text-xs font-medium text-fg/85">{t('admin.themeSettings.preview')}</p>
                  <div class="relative overflow-hidden rounded-zs-lg border border-line bg-surface-strong">
                    {bg && <img src={bg} alt="" class="absolute inset-0 h-full w-full object-cover opacity-40" />}
                    <div class="relative space-y-3 p-4">
                      <div class="h-10 rounded-zs shadow-zs-sm" style={{ background: gradient.value }} data-testid="theme-preview-gradient" />
                      <div class="flex gap-2">
                        {(['primary_color', 'secondary_color', 'accent_color'] as const).map((k) => (
                          <span key={k} class="h-6 flex-1 rounded-full border border-line" style={{ background: effective(k) }} />
                        ))}
                      </div>
                      <div class="flex items-end justify-between gap-3">
                        <div class="space-y-2">
                          <span class="inline-flex h-7 items-center rounded-full px-3 text-xs font-medium text-on-primary" style={{ background: gradient.value }}>
                            {t('admin.settings.actions.save')}
                          </span>
                          <p class="text-xs font-semibold" style={{ color: effective('primary_color') }}>
                            ✦ {t('admin.themeSettings.primaryColor')}
                          </p>
                        </div>
                        {mascot ? (
                          <img src={mascot} alt="" class="h-28 w-28 object-contain" />
                        ) : (
                          <div class={cn('shrink-0')}>
                            <Mascot size={112} mood="wink" />
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            ),
          }}
        </Card>
      )
    }
  },
})
