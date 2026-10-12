import type { ThemeForm } from './settingsUtils'

export const STOREFRONT_TEMPLATES = ['classic', 'vault', 'aurora', 'atelier', 'noir'] as const
export type StorefrontTemplate = (typeof STOREFRONT_TEMPLATES)[number]
export type ThemeColors = Pick<ThemeForm, 'primary_color' | 'secondary_color' | 'accent_color'>

export const TEMPLATE_COLORS: Record<StorefrontTemplate, ThemeColors> = {
  classic: { primary_color: '#ff5fa2', secondary_color: '#8b5cf6', accent_color: '#38bdf8' },
  vault: { primary_color: '#6366f1', secondary_color: '#22d3ee', accent_color: '#fbbf24' },
  aurora: { primary_color: '#0f766e', secondary_color: '#14b8a6', accent_color: '#a3e635' },
  atelier: { primary_color: '#b45309', secondary_color: '#7c2d12', accent_color: '#d97706' },
  noir: { primary_color: '#d4af37', secondary_color: '#b98b45', accent_color: '#f5d68a' },
}

export const TEMPLATE_BASES: Record<StorefrontTemplate, { background_color: string; surface_color: string; text_color: string }> = {
  classic: { background_color: '#fff6fb', surface_color: '#ffffff', text_color: '#2d2445' },
  vault: { background_color: '#0b1026', surface_color: '#151d3a', text_color: '#edf4ff' },
  aurora: { background_color: '#eefbf7', surface_color: '#ffffff', text_color: '#123c38' },
  atelier: { background_color: '#f8f3e9', surface_color: '#fffdf8', text_color: '#3c2f28' },
  noir: { background_color: '#0d0c0b', surface_color: '#1b1916', text_color: '#f5eee2' },
}

export const COLOR_PRESETS = [
  { key: 'sakura', colors: TEMPLATE_COLORS.classic },
  { key: 'indigo', colors: TEMPLATE_COLORS.vault },
  { key: 'aurora', colors: TEMPLATE_COLORS.aurora },
  { key: 'atelier', colors: TEMPLATE_COLORS.atelier },
  { key: 'noir', colors: TEMPLATE_COLORS.noir },
  { key: 'ocean', colors: { primary_color: '#0284c7', secondary_color: '#2563eb', accent_color: '#2dd4bf' } },
  { key: 'sunset', colors: { primary_color: '#ea580c', secondary_color: '#e11d48', accent_color: '#facc15' } },
  { key: 'lilac', colors: { primary_color: '#8b5cf6', secondary_color: '#ec4899', accent_color: '#22d3ee' } },
] as const

export const normalizeStorefrontTemplate = (value: unknown): StorefrontTemplate =>
  STOREFRONT_TEMPLATES.find((template) => template === value) || 'classic'

export const applyTemplatePreset = (theme: ThemeForm, template: StorefrontTemplate): void => {
  Object.assign(theme, TEMPLATE_COLORS[template])
  theme.background_color = ''
  theme.surface_color = ''
  theme.text_color = ''
  theme.default_mode = template === 'vault' || template === 'noir' ? 'dark' : template === 'classic' ? 'system' : 'light'
  theme.effects.sakura = template === 'classic'
  theme.effects.sparkle = template === 'classic' || template === 'aurora'
}

export const isClassicPalette = (theme: ThemeColors): boolean =>
  (Object.keys(TEMPLATE_COLORS.classic) as Array<keyof ThemeColors>)
    .every((key) => theme[key].toLowerCase() === TEMPLATE_COLORS.classic[key])
