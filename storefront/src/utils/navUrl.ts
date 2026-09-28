/**
 * FE-06: custom navigation links are only rendered when their URL is safe:
 * external links must be http(s); internal ones a site path (never `//host` or a scheme).
 */
export const isSafeNavUrl = (url: string, external: boolean): boolean => {
  const value = url.trim()
  if (external) return /^https?:\/\/[^/\\]/i.test(value)
  return value !== '' && !/^[/\\]{2}/.test(value) && !/^[a-z][a-z0-9+.-]*:/i.test(value)
}
