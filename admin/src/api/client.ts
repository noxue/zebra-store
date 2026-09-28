import i18n from '@/i18n'
import type { ApiResponse } from './types'
import { notifyError } from '@/utils/notify'
import { adminUrl } from '@/utils/adminBase'

export type { ApiResponse }

export const TOKEN_KEY = 'admin_token'

const t = (key: string, params?: Record<string, unknown>) =>
  params ? i18n.global.t(key, params) : i18n.global.t(key)

/** Error thrown for any failed request. The user has already been notified (toast) unless `silent`. */
export class ApiError extends Error {
  readonly statusCode: number
  readonly httpStatus: number
  readonly notified: boolean
  constructor(message: string, statusCode: number, httpStatus: number, notified: boolean) {
    super(message)
    this.name = 'ApiError'
    this.statusCode = statusCode
    this.httpStatus = httpStatus
    this.notified = notified
  }
}

export type QueryParams = Record<string, unknown>

export interface RequestOptions {
  params?: QueryParams
  headers?: Record<string, string>
  /** DELETE request body */
  data?: unknown
  /** Do not show an error toast */
  silent?: boolean
  timeout?: number
}

export interface BlobResult {
  data: Blob
  headers: Record<string, string>
}

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || ''
const API_PREFIX = '/api/v1'
const DEFAULT_TIMEOUT_MS = 10_000
const UPLOAD_TIMEOUT_MS = 120_000
const SILENT_MESSAGES = new Set(['compliance_required', 'compliance_required_by_super_admin'])

export function buildUrl(path: string, params?: QueryParams): string {
  const url = `${API_BASE_URL}${API_PREFIX}${path}`
  if (!params) return url
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue
    if (Array.isArray(value)) value.forEach((v) => search.append(key, String(v)))
    else search.append(key, String(value))
  }
  const qs = search.toString()
  return qs ? `${url}?${qs}` : url
}

const isLoginEndpoint = (path: string) => /\/admin\/login\b/.test(path)

function redirectToLogin() {
  ;['admin_token', 'admin_is_super', 'admin_roles', 'admin_permissions'].forEach((k) => localStorage.removeItem(k))
  if (!window.location.pathname.endsWith('/login')) window.location.href = adminUrl('/login')
}

function httpErrorMessage(status: number): string {
  switch (status) {
    case 401:
      return t('common.api.unauthorized')
    case 403:
      return t('common.api.forbidden')
    case 404:
      return t('common.api.notFound')
    case 500:
      return t('common.api.serverError')
    case 502:
      return t('common.api.badGateway')
    case 503:
      return t('common.api.serviceUnavailable')
    default:
      return t('common.api.requestFailedStatus', { status })
  }
}

function fail(message: string, statusCode: number, httpStatus: number, opts: RequestOptions): never {
  const silent = !!opts.silent || SILENT_MESSAGES.has(message)
  if (!silent) notifyError(message)
  throw new ApiError(message, statusCode, httpStatus, !silent)
}

async function readJson(res: Response): Promise<ApiResponse<unknown> | null> {
  try {
    return (await res.json()) as ApiResponse<unknown>
  } catch {
    return null
  }
}

async function send(method: string, path: string, body: unknown, opts: RequestOptions): Promise<Response> {
  const headers: Record<string, string> = { ...opts.headers }
  const locale = i18n.global.locale.value
  if (locale) headers['X-Lang'] = locale
  const token = localStorage.getItem(TOKEN_KEY)
  if (token) headers['Authorization'] = `Bearer ${token}`
  const isForm = body instanceof FormData
  if (body !== undefined && !isForm) headers['Content-Type'] = 'application/json'

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), opts.timeout ?? (isForm ? UPLOAD_TIMEOUT_MS : DEFAULT_TIMEOUT_MS))
  try {
    return await fetch(buildUrl(path, opts.params), {
      method,
      headers,
      body: isForm ? body : body !== undefined ? JSON.stringify(body) : undefined,
      signal: controller.signal,
    })
  } catch {
    return fail(t('common.api.networkError'), -1, 0, opts)
  } finally {
    clearTimeout(timer)
  }
}

async function request<T>(method: string, path: string, body: unknown, opts: RequestOptions = {}): Promise<ApiResponse<T>> {
  const res = await send(method, path, body, opts)
  const payload = await readJson(res)
  if (!payload) {
    if (!res.ok) {
      if (res.status === 401 && !isLoginEndpoint(path)) redirectToLogin()
      return fail(httpErrorMessage(res.status), res.status, res.status, opts)
    }
    return fail(t('common.api.responseMissing'), -1, res.status, opts)
  }
  if (!res.ok) {
    if (res.status === 401 && !isLoginEndpoint(path)) redirectToLogin()
    return fail(payload.msg || httpErrorMessage(res.status), payload.status_code ?? res.status, res.status, opts)
  }
  if (typeof payload.status_code !== 'undefined' && payload.status_code !== 0) {
    if (payload.status_code === 401 && !isLoginEndpoint(path)) {
      redirectToLogin()
    }
    return fail(payload.msg || t('common.api.requestFailed'), payload.status_code, res.status, opts)
  }
  return payload as ApiResponse<T>
}

async function requestBlob(method: string, path: string, body: unknown, opts: RequestOptions = {}): Promise<BlobResult> {
  const res = await send(method, path, body, opts)
  const contentType = res.headers.get('content-type') || ''
  const blob = await res.blob()
  const readMsg = async (fallback: string) => {
    try {
      const parsed = JSON.parse(await blob.text()) as { msg?: string }
      return parsed.msg || fallback
    } catch {
      return fallback
    }
  }
  if (!res.ok) {
    if (res.status === 401 && !isLoginEndpoint(path)) redirectToLogin()
    const fallback = httpErrorMessage(res.status)
    return fail(contentType.includes('application/json') ? await readMsg(fallback) : fallback, res.status, res.status, opts)
  }
  if (contentType.includes('application/json')) {
    return fail(await readMsg(t('common.api.requestFailed')), -1, res.status, opts)
  }
  const headers: Record<string, string> = {}
  res.headers.forEach((value, key) => {
    headers[key] = value
  })
  return { data: blob, headers }
}

export const api = {
  get: <T>(path: string, opts?: RequestOptions) => request<T>('GET', path, opts?.data, opts),
  post: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('POST', path, body, opts),
  put: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('PUT', path, body, opts),
  patch: <T>(path: string, body?: unknown, opts?: RequestOptions) => request<T>('PATCH', path, body, opts),
  delete: <T>(path: string, opts?: RequestOptions) => request<T>('DELETE', path, opts?.data, opts),
  getBlob: (path: string, opts?: RequestOptions) => requestBlob('GET', path, undefined, opts),
  postBlob: (path: string, body?: unknown, opts?: RequestOptions) => requestBlob('POST', path, body, opts),
}

/** Extract a readable message from anything thrown by an API call. */
export function errorMessage(err: unknown, fallback = ''): string {
  if (err instanceof Error && err.message) return err.message
  return fallback
}
