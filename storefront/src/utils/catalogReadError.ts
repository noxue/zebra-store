import { ApiError } from '@/api/client'
export type CatalogReadError = 'rateLimited' | 'accessDenied' | 'unavailable' | 'conditionsChanged' | 'invalidResponse' | 'requestFailed'
export class CatalogResponseError extends Error { constructor() { super('Invalid catalog response'); this.name = 'CatalogResponseError' } }
/** Safe user-facing categories; supplier/server response text is never copied to the UI. */
export function catalogReadError(error: unknown): CatalogReadError {
  if (error instanceof CatalogResponseError) return 'invalidResponse'
  if (error instanceof ApiError) {
    if (error.statusCode === 429 || error.httpStatus === 429) return 'rateLimited'
    if ([401, 403].includes(error.statusCode) || [401, 403].includes(error.httpStatus)) return 'accessDenied'
    if (error.httpStatus >= 500 || error.statusCode >= 500) return 'unavailable'
    if (error.statusCode === 400 || error.statusCode === 404 || error.httpStatus === 400 || error.httpStatus === 404) return 'conditionsChanged'
  }
  return 'requestFailed'
}
