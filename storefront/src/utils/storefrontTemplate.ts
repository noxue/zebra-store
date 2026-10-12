import type { SiteConfig } from '@/api/types'

export type StorefrontTemplate = 'classic' | 'vault' | 'aurora' | 'atelier' | 'noir'

const TEMPLATES: StorefrontTemplate[] = ['classic', 'vault', 'aurora', 'atelier', 'noir']
const CLASSIC_COLORS = ['#ff5fa2', '#8b5cf6', '#38bdf8']

export const storefrontTemplate = (value: unknown): StorefrontTemplate =>
  TEMPLATES.find((template) => template === value) || 'classic'

/** Old installations stored Sakura colours even when "vault" was selected. */
export const hasLegacyClassicPalette = (config: SiteConfig | null): boolean => {
  const colors = [config?.theme?.primary_color, config?.theme?.secondary_color, config?.theme?.accent_color]
  return storefrontTemplate(config?.storefront_template) !== 'classic'
    && config?.theme?.default_mode === 'system'
    && config?.theme?.effects?.sakura !== false
    && colors.every((color, index) => color?.toLowerCase() === CLASSIC_COLORS[index])
}
