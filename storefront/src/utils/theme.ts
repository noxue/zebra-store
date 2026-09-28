import { ref, watch } from 'vue'

const THEME_KEY = 'dujiao_theme'
export type ThemeMode = 'light' | 'dark'

const systemTheme = (): ThemeMode =>
  typeof window !== 'undefined' && window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'

const savedTheme = (): ThemeMode | null => {
  try {
    const v = localStorage.getItem(THEME_KEY)
    return v === 'dark' || v === 'light' ? v : null
  } catch {
    return null
  }
}

const theme = ref<ThemeMode>(savedTheme() || systemTheme())
let userChosen = savedTheme() !== null

const apply = (mode: ThemeMode) => {
  if (typeof document === 'undefined') return
  document.documentElement.classList.toggle('dark', mode === 'dark')
}
apply(theme.value)
watch(theme, (mode) => {
  apply(mode)
  if (userChosen) localStorage.setItem(THEME_KEY, mode)
})

/** Applies `theme.default_mode` from site config when the user never chose. */
export const applyDefaultThemeMode = (mode: string | undefined) => {
  if (userChosen || !mode || mode === 'system') return
  if (mode === 'light' || mode === 'dark') theme.value = mode
}

export const useTheme = () => ({
  theme,
  toggleTheme: () => {
    userChosen = true
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
  },
})
