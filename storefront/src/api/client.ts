import i18n from '@/i18n'
import { isPublicAuthEndpoint } from '@/utils/authEndpoints'

export interface Pagination {
  page: number
  page_size: number
  total: number
  total_page: number
}

/** Response envelope shared by every backend endpoint. */
export interface ApiResponse<T> {
  status_code: number
  msg: string
  data: T
  pagination?: Pagination
}

export class ApiError extends Error {
  readonly statusCode: number
  readonly httpStatus: number
  readonly silent: boolean
  constructor(message: string, statusCode: number, httpStatus: number, silent = false) {
    super(message)
    this.name = 'ApiError'
    this.statusCode = statusCode
    this.httpStatus = httpStatus
    this.silent = silent
  }
}

export type QueryValue = string | number | boolean | null | undefined
export type QueryParams = Record<string, QueryValue>

export interface RequestOptions {
  params?: object
  headers?: Record<string, string>
  /** Business errors are not logged when true. */
  silentBusinessError?: boolean
  credentials?: RequestCredentials
}

const API_BASE_URL: string = import.meta.env.VITE_API_BASE_URL || ''
export const API_PREFIX = '/api/v1'
const TIMEOUT_MS = 10_000

const translate = (key: string, params?: Record<string, unknown>): string =>
  params ? i18n.global.t(key, params) : i18n.global.t(key)

export const buildUrl = (base: string, path: string, params?: object): string => {
  const url = `${base}${path}`
  if (!params) return url
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params as Record<string, unknown>)) {
    if (value !== undefined && value !== null && value !== '') search.append(key, String(value))
  }
  const qs = search.toString()
  return qs ? `${url}?${qs}` : url
}

const httpErrorMessage = (status: number): string => {
  switch (status) {
    case 401:
      return translate('common.api.unauthorized')
    case 403:
      return translate('common.api.forbidden')
    case 404:
      return translate('common.api.notFound')
    case 500:
      return translate('common.api.serverError')
    case 502:
      return translate('common.api.badGateway')
    case 503:
      return translate('common.api.serviceUnavailable')
    default:
      return translate('common.api.requestFailedStatus', { status })
  }
}

const currentLocale = (): string => String(i18n.global.locale.value || '')

const handleUnauthorized = (injectAuth: boolean, path: string, hadAuthHeader: boolean) => {
  if (!injectAuth || isPublicAuthEndpoint(path) || !hadAuthHeader) return
  localStorage.removeItem('user_token')
  localStorage.removeItem('user_profile')
  window.location.href = '/auth/login'
}

const isEnvelope = (value: unknown): value is ApiResponse<unknown> =>
  typeof value === 'object' && value !== null && 'status_code' in value

async function send(
  injectAuth: boolean,
  method: string,
  path: string,
  body: unknown,
  opts: RequestOptions,
): Promise<Response> {
  const url = buildUrl(`${API_BASE_URL}${API_PREFIX}`, path, opts.params)
  const headers: Record<string, string> = { ...opts.headers }
  const locale = currentLocale()
  if (locale) headers['X-Lang'] = locale
  if (injectAuth) {
    const token = localStorage.getItem('user_token')
    if (token && !headers.Authorization) headers.Authorization = `Bearer ${token}`
  }
  const isForm = typeof FormData !== 'undefined' && body instanceof FormData
  if (body !== undefined && !isForm) headers['Content-Type'] = 'application/json'

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS)
  try {
    return await fetch(url, {
      method,
      headers,
      body: isForm ? (body as FormData) : body !== undefined ? JSON.stringify(body) : undefined,
      credentials: opts.credentials,
      signal: controller.signal,
    })
  } catch {
    throw new ApiError(translate('common.api.networkError'), -1, 0)
  } finally {
    clearTimeout(timer)
  }
}

function createClient(injectAuth: boolean) {
  async function request<T>(method: string, path: string, body: unknown, opts: RequestOptions = {}): Promise<ApiResponse<T>> {
    const response = await send(injectAuth, method, path, body, opts)
    const hadAuth = injectAuth && (!!localStorage.getItem('user_token') || !!opts.headers?.Authorization)
    let payload: unknown
    try {
      payload = await response.json()
    } catch {
      if (!response.ok) {
        if (response.status === 401) handleUnauthorized(injectAuth, path, hadAuth && !opts.headers?.Authorization)
        throw new ApiError(httpErrorMessage(response.status), response.status, response.status)
      }
      throw new ApiError(translate('common.api.responseMissing'), -1, response.status)
    }
    if (!isEnvelope(payload)) {
      throw new ApiError(translate('common.api.responseMissing'), -1, response.status)
    }
    const envelope = payload as ApiResponse<T>
    const isBearerRequest = hadAuth && !opts.headers?.Authorization
    if (!response.ok) {
      if (response.status === 401) handleUnauthorized(injectAuth, path, isBearerRequest)
      throw new ApiError(envelope.msg || httpErrorMessage(response.status), envelope.status_code, response.status)
    }
    if (envelope.status_code !== 0) {
      if (envelope.status_code === 401) handleUnauthorized(injectAuth, path, isBearerRequest)
      const silent = Boolean(opts.silentBusinessError)
      if (!silent && import.meta.env.DEV) console.warn('API Error:', envelope.msg)
      throw new ApiError(envelope.msg || translate('common.api.requestFailed'), envelope.status_code, response.status, silent)
    }
    return envelope
  }

  async function blob(path: string, opts: RequestOptions = {}): Promise<Blob> {
    const response = await send(injectAuth, 'GET', path, undefined, opts)
    if (!response.ok) throw new ApiError(httpErrorMessage(response.status), response.status, response.status)
    return response.blob()
  }

  return {
    get: <T>(path: string, opts?: RequestOptions) => request<T>('GET', path, undefined, opts),
    delete: <T>(path: string, opts?: RequestOptions) => request<T>('DELETE', path, undefined, opts),
    post: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('POST', path, body ?? {}, opts),
    put: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('PUT', path, body ?? {}, opts),
    patch: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('PATCH', path, body ?? {}, opts),
    blob,
  }
}

/** Public client (no auth header). */
export const api = createClient(false)
/** Client that injects `Authorization: Bearer <user_token>`. */
export const userApi = createClient(true)

/** Extracts a human readable message from anything thrown. */
export const errorMessage = (error: unknown, fallback = ''): string => {
  if (error instanceof Error && error.message) return error.message
  if (typeof error === 'string' && error) return error
  return fallback || translate('common.api.requestFailed')
}

export const isSilentError = (error: unknown): boolean => error instanceof ApiError && error.silent
