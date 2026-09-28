import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Moon, Sun } from 'lucide-vue-next'
import { useTheme } from '@/utils/theme'

export const ThemeToggle = defineComponent({
  name: 'ThemeToggle',
  setup() {
    const { t } = useI18n()
    const { theme, toggleTheme } = useTheme()
    return () => (
      <button
        type="button"
        aria-label={theme.value === 'dark' ? t('zs.themeLight') : t('zs.themeDark')}
        title={theme.value === 'dark' ? t('zs.themeLight') : t('zs.themeDark')}
        class="flex size-10 items-center justify-center rounded-full text-muted transition hover:bg-primary-soft hover:text-primary-text"
        onClick={toggleTheme}
      >
        {theme.value === 'dark' ? <Sun class="size-[18px] text-gold" /> : <Moon class="size-[18px]" />}
      </button>
    )
  },
})
