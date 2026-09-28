export function getImageUrl(path: string | undefined | null): string {
  if (!path) return ''

  if (path.startsWith('http://') || path.startsWith('https://')) {
    return path
  }

  const apiBaseUrl = import.meta.env.VITE_API_BASE_URL || ''
  const normalizedPath = path.startsWith('/') ? path : `/${path}`

  return `${apiBaseUrl}${normalizedPath}`
}

export function getFirstImageUrl(images: unknown): string {
  if (!images) return ''

  let imageUrl = ''

  if (Array.isArray(images)) {
    imageUrl = String(images[0] || '')
  } else if (typeof images === 'object') {
    const nested = (images as { images?: unknown }).images
    if (Array.isArray(nested)) imageUrl = String(nested[0] || '')
  }

  return getImageUrl(imageUrl)
}
