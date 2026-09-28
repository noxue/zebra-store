/** Prefix relative upload paths (e.g. /uploads/x.png) with VITE_API_BASE_URL. */
export function getImageUrl(path: string | undefined | null): string {
  if (!path) return ''
  if (/^(https?:)?\/\//i.test(path) || path.startsWith('data:') || path.startsWith('blob:')) return path
  const base: string = import.meta.env.VITE_API_BASE_URL || ''
  return `${base}${path.startsWith('/') ? path : `/${path}`}`
}

export function getFirstImageUrl(images: string[] | null | undefined): string {
  if (!Array.isArray(images)) return ''
  return getImageUrl(images[0] || '')
}
