import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PasswordStrength } from '@/utils/auth/validation'
import { cn } from '@/components/ui'

export const PasswordStrengthMeter = defineComponent({
  name: 'PasswordStrengthMeter',
  props: { strength: { type: String as PropType<PasswordStrength>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const levels: PasswordStrength[] = ['weak', 'medium', 'strong']
    const tone = { weak: 'bg-danger', medium: 'bg-warning', strong: 'bg-success' }
    const text = { weak: 'text-danger-text', medium: 'text-warning-text', strong: 'text-success-text' }
    return () => {
      const idx = levels.indexOf(props.strength)
      return (
        <div class="flex items-center gap-3 pt-1">
          <div class="flex flex-1 gap-1.5">
            {levels.map((_, i) => (
              <span key={i} class={cn('h-1.5 flex-1 rounded-full transition-colors', i <= idx ? tone[props.strength] : 'bg-surface-muted')} />
            ))}
          </div>
          <span class={cn('text-xs font-bold', text[props.strength])}>
            {t('zsContent.passwordStrength')}: {t(`formValidation.passwordStrength.${props.strength}`)}
          </span>
        </div>
      )
    }
  },
})
