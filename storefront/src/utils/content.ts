import DOMPurify from 'dompurify'
import { getImageUrl } from './image'

/** Rewrites src="/uploads/..." to absolute display URLs. */
export function processHtmlForDisplay(html: string): string {
  if (!html) return ''
  return html.replace(/src=["'](\/uploads\/.*?)["']/g, (_m, path: string) => `src="${getImageUrl(path)}"`)
}

/** Sanitises rich HTML coming from the backend before v-html style rendering. */
export function sanitizeHtml(html: string | null | undefined): string {
  if (!html) return ''
  return DOMPurify.sanitize(processHtmlForDisplay(html), {
    ADD_ATTR: ['target', 'rel'],
  })
}

/** True when an HTML string has visible text or media. */
export function hasHtmlContent(html: string | null | undefined): boolean {
  if (!html) return false
  if (/<(img|video|iframe|table)\b/i.test(html)) return true
  return html.replace(/<[^>]*>/g, '').replace(/&nbsp;/g, ' ').trim().length > 0
}
