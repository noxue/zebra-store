import crypto from 'node:crypto'
import { expect, request, type APIRequestContext } from '@playwright/test'
import { ADMIN_PASSWORD, ADMIN_USERNAME, API_URL, EPAY } from './env'

export interface Envelope<T> {
  status_code: number
  msg: string
  data: T
  pagination?: { page: number; page_size: number; total: number; total_page: number }
}

async function unwrap<T>(res: Awaited<ReturnType<APIRequestContext['get']>>, what: string): Promise<Envelope<T>> {
  expect(res.status(), `${what} HTTP status`).toBe(200)
  const body = (await res.json()) as Envelope<T>
  expect(body.status_code, `${what}: ${body.msg}`).toBe(0)
  return body
}

/** Direct backend client used for verification (the UI remains the system under test). */
export class Api {
  private constructor(
    private readonly ctx: APIRequestContext,
    private readonly token = '',
  ) {}

  static async anonymous() {
    return new Api(await request.newContext({ baseURL: `${API_URL}/api/v1/` }))
  }

  static async admin() {
    const ctx = await request.newContext({ baseURL: `${API_URL}/api/v1/` })
    const res = await ctx.post('admin/login', { data: { username: ADMIN_USERNAME, password: ADMIN_PASSWORD } })
    const body = await unwrap<{ token: string }>(res, 'admin login')
    return new Api(ctx, body.data.token)
  }

  private headers() {
    return this.token ? { Authorization: `Bearer ${this.token}` } : undefined
  }

  async get<T>(path: string, params?: Record<string, string | number>) {
    return unwrap<T>(await this.ctx.get(path, { params, headers: this.headers() }), `GET ${path}`)
  }

  async post<T>(path: string, data?: unknown) {
    return unwrap<T>(await this.ctx.post(path, { data, headers: this.headers() }), `POST ${path}`)
  }

  async put<T>(path: string, data?: unknown) {
    return unwrap<T>(await this.ctx.put(path, { data, headers: this.headers() }), `PUT ${path}`)
  }

  async dispose() {
    await this.ctx.dispose()
  }
}

/** epay v1 MD5 signature: md5(sorted non-empty k=v joined by & (minus sign/sign_type) + key). */
export function epaySign(params: Record<string, string>, key: string): string {
  const content = Object.keys(params)
    .filter((k) => params[k] !== '' && k !== 'sign' && k !== 'sign_type')
    .sort()
    .map((k) => `${k}=${params[k]}`)
    .join('&')
  return crypto.createHash('md5').update(content + key).digest('hex')
}

/**
 * Simulates the epay gateway's asynchronous notification for a created payment.
 * `outTradeNo` / `money` come from the pay URL the backend generated for the customer.
 */
export async function sendEpayCallback(outTradeNo: string, money: string, type = 'alipay') {
  const params: Record<string, string> = {
    pid: EPAY.merchantId,
    trade_no: `EPAY${Date.now()}`,
    out_trade_no: outTradeNo,
    type,
    name: 'e2e',
    money,
    trade_status: 'TRADE_SUCCESS',
  }
  params.sign = epaySign(params, EPAY.merchantKey)
  params.sign_type = 'MD5'
  const ctx = await request.newContext()
  try {
    const res = await ctx.post(`${API_URL}/api/v1/payments/callback`, { form: params })
    const text = await res.text()
    expect(res.status(), `epay callback: ${text}`).toBe(200)
    expect(text.trim(), 'epay callback acknowledgement').toBe('success')
  } finally {
    await ctx.dispose()
  }
}

/** Extracts out_trade_no / money from an epay submit.php redirect URL. */
export function parseEpayPayUrl(url: string) {
  const u = new URL(url)
  const outTradeNo = u.searchParams.get('out_trade_no') ?? ''
  const money = u.searchParams.get('money') ?? ''
  expect(outTradeNo, `out_trade_no in ${url}`).not.toBe('')
  return { outTradeNo, money, type: u.searchParams.get('type') ?? 'alipay', returnUrl: u.searchParams.get('return_url') ?? '' }
}

/**
 * Serves a stand-in page for the (non-existent) epay gateway host so "open payment link"
 * navigations succeed. `opened` lists the pay URLs the browser visited. With
 * `stayOnPage = true` the gateway answers 204, so the browser stays where it is (like a customer
 * who closes the cashier and comes back) — the storefront auto-opens a pending redirect payment.
 */
export async function mockEpayGateway(context: import('@playwright/test').BrowserContext) {
  const gateway = { opened: [] as string[], stayOnPage: false }
  await context.route(`${EPAY.gatewayUrl}/**`, async (route) => {
    gateway.opened.push(route.request().url())
    if (gateway.stayOnPage) {
      await route.fulfill({ status: 204, body: '' })
      return
    }
    await route.fulfill({
      status: 200,
      contentType: 'text/html; charset=utf-8',
      body: '<!doctype html><title>epay sandbox</title><h1>epay sandbox</h1><p>E2E stand-in for the payment gateway.</p>',
    })
  })
  return gateway
}
