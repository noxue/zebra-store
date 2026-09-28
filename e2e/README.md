# Zebra Store E2E (Playwright)

End-to-end business flows through the **real UIs** (admin + storefront) against the **Rust backend**.

| Spec | Flow |
|---|---|
| `tests/01-admin-setup.spec.ts` | admin login → compliance acknowledgement → site settings (name, logo upload, theme color, email verification off) → category → auto-delivery product with 2 SKUs → card secrets (CSV import + pasted batch) → epay payment channel → banner, blog post, coupon, member level, gift cards |
| `tests/02-storefront-guest.spec.ts` | home shows site name/theme → product detail → buy now → guest checkout (email + order password) → gateway redirect → signed epay callback → guest order shows the delivered card secret → guest order lookup |
| `tests/03-storefront-member.spec.ts` | register (no email code) → login → cart → checkout with coupon (25.00 → 22.50) → wallet recharge via epay → pay the pending order with balance → personal center (orders, wallet, gift-card redeem, security, profile) |
| `tests/04-admin-followup.spec.ts` | admin order list/detail shows both orders → refund 5.00 to wallet → refund records → dashboard KPIs non-zero |

Every test fails on **browser console errors, page errors, HTTP ≥ 400 on `/api/v1/*`, or any API
envelope with `status_code != 0`** (see `support/fixtures.ts`; expected errors are whitelisted per test
with `watch.allow(...)`). Key pages are captured to `screenshots/`.

Payments: the epay gateway host (`https://epay.e2e.invalid`) is stubbed in the browser
(`mockEpayGateway`), and the payment is completed by POSTing a correctly signed epay v1 (MD5)
notification to `/api/v1/payments/callback` (`support/api.ts#sendEpayCallback`), then following
the gateway's `return_url` like a real customer.

The specs are one ordered story sharing state via `.state/state.json` and **need a fresh database**
(run `npm test` for all specs in order; they run serially in one worker).

## Run

```bash
cd e2e && npm install            # @playwright/test 1.63 (browsers: npx playwright install chromium)

# 1. backend on :8082 with a fresh temp SQLite DB (builds backend/target-e2e if needed)
npm run backend                  # = scripts/start-backend.sh [port]; ZS_E2E_BUILD=1 forces a rebuild

# 2. dev servers pointing at it (any free ports)
(cd ../storefront && VITE_API_TARGET=http://localhost:8082 npm run dev -- --port 5195)
(cd ../admin      && VITE_API_TARGET=http://localhost:8082 npm run dev -- --port 5196)

# 3. specs
E2E_ADMIN_URL=http://localhost:5196 E2E_STOREFRONT_URL=http://localhost:5195 E2E_API_URL=http://localhost:8082 npm test
npm run report                   # HTML report (traces kept for failures)
```

Environment: `E2E_ADMIN_URL` (default `http://localhost:5186`), `E2E_STOREFRONT_URL` (default
`http://localhost:5185`), `E2E_API_URL` (default `http://localhost:8082`), `E2E_ADMIN_USERNAME` /
`E2E_ADMIN_PASSWORD` (default `admin` / `Admin12345`, matching `start-backend.sh`), `E2E_RUN_ID`
(suffix for created names; defaults to a timestamp).

`start-backend.sh` raises the login rate limit (`ZS__SECURITY__LOGIN_RATE_LIMIT__MAX_ATTEMPTS`)
because the suite signs in many times from one IP.
