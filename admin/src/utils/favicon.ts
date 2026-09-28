import { getImageUrl } from './image'

const DEFAULT_SITE_ICON = '/favicon.svg'

export function resolveSiteIconHref(value: unknown): string {
  const icon = String(value || '').trim()
  return icon ? getImageUrl(icon) : DEFAULT_SITE_ICON
}

export function applySiteIcon(value: unknown) {
  let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
  if (!link) {
    link = document.createElement('link')
    link.rel = 'icon'
    document.head.appendChild(link)
  }
  link.href = resolveSiteIconHref(value)
}
