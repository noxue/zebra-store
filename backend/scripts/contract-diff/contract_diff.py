#!/usr/bin/env python3
"""Structural API contract diff: compatibility backend vs Zebra Store (Rust).

Seeds EQUIVALENT data into both backends through their admin/user/guest APIs,
then calls the route table's GET endpoints plus key POST flows and error cases
on both, and compares the JSON responses structurally:

  * HTTP status, envelope keys, `status_code`, `msg` (exact), pagination values
  * field names present / missing (recursively; list elements are merged)
  * value types: string / number / bool / null / object / array, with string
    sub-kinds `money` ("12.30") and `time` (RFC 3339)

Volatile values (ids, timestamps, tokens, request ids, order numbers) are never
compared, only their types.  Differences matching `deviations.json` are labelled
as documented intentional deviations.

Python 3 stdlib only.  Usually started through run.sh, which boots fresh
instances; can also be pointed at already-running empty instances:

  python3 contract_diff.py --orig http://127.0.0.1:8092 --ours http://127.0.0.1:8091
  python3 contract_diff.py ... --only 'admin/products'    # regex filter on check names
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable

ADMIN_USER, ADMIN_PASS = "admin", "Admin12345"
USER_EMAIL, USER_PASS = "cd-user@example.com", "User12345"
GUEST_EMAIL, GUEST_PASS = "cd-guest@example.com", "guestpass1"
MISSING_ID = 999999

COMPLIANCE = {
    "segment1": "我已阅读并理解上述合规声明提醒",
    "segment2": "知悉相关法律风险",
    "segment3": "并确认自行承担部署运营和收费行为产生的法律责任",
}

SETTING_KEYS = [
    "site_config", "order_config", "smtp_config", "captcha_config", "telegram_auth_config",
    "google_auth_config", "dashboard_config", "notification_center_config", "affiliate_config",
    "telegram_bot_config", "telegram_bot_runtime_status", "order_email_template_config",
    "nav_config", "wallet_config", "payment_config", "registration_config",
    "order_risk_control_config", "upstream_sync_config", "callback_routes_config",
    "home_announcement",
]

MONEY_RE = re.compile(r"^-?\d+\.\d{2}$")
TIME_RE = re.compile(r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?$")


# --------------------------------------------------------------------------- HTTP
@dataclass
class Resp:
    status: int
    body: Any  # parsed JSON, or {"__raw__": content-type, "__head__": first line}
    is_json: bool


@dataclass
class Side:
    name: str
    base: str
    ids: dict = field(default_factory=dict)
    admin_token: str = ""
    user_token: str = ""

    def call(self, method: str, path: str, body: Any = None, auth: str | None = None,
             headers: dict | None = None) -> Resp:
        url = self.base + "/api/v1" + path if not path.startswith("/!") else self.base + path[2:]
        data = None
        hdrs = {"Accept": "application/json"}
        if body is not None:
            data = json.dumps(body).encode()
            hdrs["Content-Type"] = "application/json"
        if auth == "admin":
            hdrs["Authorization"] = "Bearer " + self.admin_token
        elif auth == "user":
            hdrs["Authorization"] = "Bearer " + self.user_token
        elif auth == "guest":
            raw = f"{GUEST_EMAIL}\n{GUEST_PASS}".encode()
            hdrs["Authorization"] = "Guest " + base64.urlsafe_b64encode(raw).decode().rstrip("=")
        elif auth == "badtoken":
            hdrs["Authorization"] = "Bearer not.a.jwt"
        hdrs.update(headers or {})
        req = urllib.request.Request(url, data=data, method=method, headers=hdrs)
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                status, ctype, raw = r.status, r.headers.get("Content-Type", ""), r.read()
        except urllib.error.HTTPError as e:
            status, ctype, raw = e.code, e.headers.get("Content-Type", ""), e.read()
        except Exception as e:  # connection problems
            return Resp(0, {"__error__": str(e)}, False)
        if "json" in ctype:
            try:
                return Resp(status, json.loads(raw.decode() or "null"), True)
            except ValueError:
                pass
        head = raw.decode(errors="replace").splitlines()[0][:80] if raw else ""
        return Resp(status, {"__raw__": ctype.split(";")[0], "__head__": head}, False)


def subst(value: Any, ids: dict) -> Any:
    """Replace "{name}" placeholders (whole-string -> raw value; inline -> str)."""
    if isinstance(value, str):
        m = re.fullmatch(r"\{(\w+)\}", value)
        if m:
            return ids.get(m.group(1), MISSING_ID)
        return re.sub(r"\{(\w+)\}", lambda mm: str(ids.get(mm.group(1), MISSING_ID)), value)
    if isinstance(value, list):
        return [subst(v, ids) for v in value]
    if isinstance(value, dict):
        return {k: subst(v, ids) for k, v in value.items()}
    return value


def dig(body: Any, path: str) -> Any:
    cur = body
    for part in re.findall(r"[^.\[\]]+|\[\d+\]", path):
        if cur is None:
            return None
        if part.startswith("["):
            idx = int(part[1:-1])
            cur = cur[idx] if isinstance(cur, list) and len(cur) > idx else None
        else:
            cur = cur.get(part) if isinstance(cur, dict) else None
    return cur


def id_by_order_no(suffix: str, var: str) -> Callable[[Any, dict], Any]:
    """Capture: id of the (possibly nested child) order whose order_no == ids[var] + suffix."""
    def find(body: Any, ids: dict) -> Any:
        want = f"{ids.get(var)}{suffix}"
        stack = [body]
        while stack:
            cur = stack.pop()
            if isinstance(cur, dict):
                if cur.get("order_no") == want and "id" in cur:
                    return cur["id"]
                stack.extend(cur.values())
            elif isinstance(cur, list):
                stack.extend(cur)
        return None
    return find


# --------------------------------------------------------------------------- shapes
def tag_of(v: Any) -> str:
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, (int, float)):
        return "number"
    if isinstance(v, str):
        if MONEY_RE.match(v):
            return "money"
        if TIME_RE.match(v):
            return "time"
        return "string"
    if isinstance(v, list):
        return "array"
    return "object"


def shape(v: Any) -> dict:
    s = {"t": {tag_of(v)}}
    if isinstance(v, dict):
        s["k"] = {k: shape(x) for k, x in v.items()}
    elif isinstance(v, list):
        s["e"] = None
        for x in v:
            s["e"] = merge(s["e"], shape(x))
    return s


def merge(a: dict | None, b: dict | None) -> dict | None:
    if a is None:
        return b
    if b is None:
        return a
    out = {"t": a["t"] | b["t"]}
    if "k" in a or "k" in b:
        ka, kb = a.get("k", {}), b.get("k", {})
        out["k"] = {k: merge(ka.get(k), kb.get(k)) for k in set(ka) | set(kb)}
    if "e" in a or "e" in b:
        out["e"] = merge(a.get("e"), b.get("e"))
    return out


STRINGY = {"string", "money", "time"}


def base_types(tags: set) -> set:
    return {"string" if t in STRINGY else t for t in tags} - {"null"}


@dataclass
class Diff:
    check: str
    path: str
    kind: str
    orig: str
    ours: str
    label: str = ""
    reason: str = ""


def fmt_tags(s: dict | None) -> str:
    if s is None:
        return "<absent>"
    return "|".join(sorted(s["t"]))


def diff_shapes(check: str, path: str, o: dict, r: dict, out: list, notes: list) -> None:
    to, tr = o["t"], r["t"]
    bo, br = base_types(to), base_types(tr)
    if bo and br and not (bo & br):
        out.append(Diff(check, path, "type", fmt_tags(o), fmt_tags(r)))
        return
    if not bo and br:
        out.append(Diff(check, path, "nullability", "null", fmt_tags(r)))
    elif bo and not br:
        out.append(Diff(check, path, "nullability", fmt_tags(o), "null"))
    # string sub-kinds: money must stay money
    if "money" in to and "string" in br and "money" not in tr and ("string" not in to):
        out.append(Diff(check, path, "money_format", fmt_tags(o), fmt_tags(r)))
    if "money" in tr and "money" not in to and "string" in to:
        out.append(Diff(check, path, "money_format", fmt_tags(o), fmt_tags(r)))
    if "number" in to and "money" in tr:
        pass  # already a type diff
    if "k" in o and "k" in r:
        ko, kr = o["k"], r["k"]
        for k in sorted(set(ko) | set(kr)):
            p = f"{path}.{k}" if path else k
            if k not in kr:
                out.append(Diff(check, p, "missing", fmt_tags(ko[k]), "<absent>"))
            elif k not in ko:
                out.append(Diff(check, p, "extra", "<absent>", fmt_tags(kr[k])))
            else:
                diff_shapes(check, p, ko[k], kr[k], out, notes)
    if "e" in o and "e" in r:
        if o["e"] is not None and r["e"] is not None:
            diff_shapes(check, path + "[]", o["e"], r["e"], out, notes)
        elif (o["e"] is None) != (r["e"] is None):
            notes.append(f"{check}: `{path or '.'}` empty on {'orig' if o['e'] is None else 'ours'} side only")
        else:
            notes.append(f"{check}: `{path or '.'}` empty on both sides")


def time_formats(v: Any, acc: set) -> None:
    if isinstance(v, str) and TIME_RE.match(v):
        frac = "frac" if "." in v else "nofrac"
        zone = "Z" if v.endswith("Z") else ("offset" if re.search(r"[+-]\d{2}:?\d{2}$", v) else "naive")
        acc.add(f"{frac}/{zone}")
    elif isinstance(v, dict):
        for x in v.values():
            time_formats(x, acc)
    elif isinstance(v, list):
        for x in v:
            time_formats(x, acc)


# --------------------------------------------------------------------------- checks
@dataclass
class Step:
    name: str
    method: str
    path: str
    auth: str | None = None
    body: Any = None
    capture: dict = field(default_factory=dict)  # var -> dotted path in response
    compare: bool = True
    headers: dict | None = None
    exact: list = field(default_factory=list)  # dotted paths whose VALUES must match


def G(path: str, auth: str | None = None, name: str | None = None, **kw) -> Step:
    return Step(name or f"GET {path}", "GET", path, auth, **kw)


def P(method: str, path: str, auth: str | None, body: Any, name: str | None = None, **kw) -> Step:
    return Step(name or f"{method} {path}", method, path, auth, body, **kw)


LT = lambda s: {"zh-CN": s, "zh-TW": s, "en-US": s}  # noqa: E731

SEED: list[Step] = [
    P("POST", "/admin/login", None, {"username": ADMIN_USER, "password": ADMIN_PASS},
      capture={"admin_token": "data.token"}),
    G("/admin/compliance/status", "admin", name="GET /admin/compliance/status (before ack)"),
    G("/admin/payment-channels", "admin", name="GET /admin/payment-channels (before ack, compliance_required)"),
    P("POST", "/admin/compliance/acknowledge", "admin", COMPLIANCE),
    P("PUT", "/admin/settings", "admin", {"key": "registration_config", "value": {
        "registration_enabled": True, "email_verification_enabled": False,
        "email_domain_allowlist_enabled": False, "allowed_email_domains": []}}),
    P("PUT", "/admin/settings/affiliate", "admin", {
        "enabled": True, "commission_rate": 10, "confirm_days": 7, "min_withdraw_amount": 10,
        "withdraw_channels": ["alipay"]}),
    P("POST", "/admin/categories", "admin", {"slug": "cd-cat", "name": LT("CD Category"), "icon": "", "sort_order": 1},
      capture={"category_id": "data.id"}),
    P("POST", "/admin/categories", "admin", {"parent_id": "{category_id}", "slug": "cd-sub", "name": LT("CD Sub"), "sort_order": 2},
      name="POST /admin/categories (child)", capture={"subcategory_id": "data.id"}),
    P("POST", "/admin/payment-channels", "admin", {
        "name": "CD Epay", "provider_type": "epay", "channel_type": "alipay", "interaction_mode": "redirect",
        "fee_rate": "0", "fixed_fee": "0", "min_amount": "0", "max_amount": "0",
        "payment_roles": [], "member_levels": [], "payment_types": ["order", "wallet"],
        "config_json": {"gateway_url": "https://epay.example.com", "epay_version": "v1", "merchant_id": "1001",
                        "merchant_key": "secretkey", "notify_url": "https://shop.example.com/api/v1/payments/callback",
                        "return_url": "https://shop.example.com/pay/return"},
        "is_active": True, "sort_order": 1}, capture={"channel_id": "data.id"}),
    P("POST", "/admin/products", "admin", {
        "category_id": "{subcategory_id}", "slug": "cd-auto", "title": LT("CD Auto Product"),
        "description": LT("desc"), "content": LT("content"), "instructions": LT("how"),
        "price_amount": 12.3, "cost_price_amount": 5, "images": ["/uploads/x.png"], "tags": ["hot"],
        "purchase_type": "guest", "stock_display_mode": "exact", "fulfillment_type": "auto",
        "wholesale_prices": [{"sku_code": "A1", "min_quantity": 5, "unit_price": 11}],
        "skus": [{"sku_code": "A1", "spec_values": {"zh-CN": "标准"}, "price_amount": 12.3, "cost_price_amount": 5, "is_active": True, "sort_order": 1},
                 {"sku_code": "A2", "spec_values": {"zh-CN": "高级"}, "price_amount": 20, "is_active": True, "sort_order": 2}],
        "is_affiliate_enabled": True, "is_active": True, "sort_order": 1}, capture={"product_id": "data.id"}),
    G("/admin/products/{product_id}", "admin", name="GET /admin/products/:id (seed)",
      capture={"sku_id": "data.skus[0].id", "sku2_id": "data.skus[1].id"}),
    P("POST", "/admin/products", "admin", {
        "category_id": "{subcategory_id}", "slug": "cd-manual", "title": LT("CD Manual Product"),
        "price_amount": 8, "purchase_type": "member", "fulfillment_type": "manual", "manual_stock_total": 10,
        "stock_display_mode": "status",
        "manual_form_schema": {"fields": [{"key": "account", "type": "text", "required": False, "label": LT("Account")}]},
        "skus": [{"sku_code": "M1", "price_amount": 8, "manual_stock_total": 10}],
        "is_active": True}, name="POST /admin/products (manual)", capture={"manual_product_id": "data.id"}),
    P("POST", "/admin/card-secrets/batch", "admin", {
        "product_id": "{product_id}", "sku_id": "{sku_id}", "secrets": ["CD-SECRET-1", "CD-SECRET-2", "CD-SECRET-3"],
        "batch_no": "CDBATCH1", "note": "contract"}),
    P("POST", "/admin/card-secrets/batch", "admin", {
        "product_id": "{product_id}", "sku_id": "{sku2_id}", "secrets": ["CD-S2-1", "CD-S2-2"], "batch_no": "CDBATCH2"},
      name="POST /admin/card-secrets/batch (sku2)"),
    P("POST", "/admin/authz/roles", "admin", {"role": "cd_role"}),
    P("POST", "/admin/authz/admins", "admin", {"username": "cd-admin", "password": "Admin12345x", "is_super": False}),
    P("POST", "/admin/authz/policies", "admin", {"role": "cd_role", "object": "/admin/products", "action": "GET"}),
    P("PUT", "/admin/authz/admins/2/roles", "admin", {"roles": ["cd_role"]}),
    P("POST", "/admin/channel-clients", "admin", {"name": "CD Bot", "channel_type": "telegram", "description": "cd"},
      capture={"channel_client_id": "data.id"}),
    P("POST", "/admin/banners", "admin", {
        "name": "cd-banner", "position": "home_hero", "title": LT("Banner"), "subtitle": LT("Sub"),
        "image": "/uploads/banner.png", "link_type": "external", "link_value": "https://example.com",
        "open_in_new_tab": True, "is_active": True, "sort_order": 1}, capture={"banner_id": "data.id"}),
    P("POST", "/admin/post-categories", "admin", {"slug": "cd-pc", "name": LT("News"), "is_active": True, "sort_order": 1},
      capture={"post_category_id": "data.id"}),
    P("POST", "/admin/posts", "admin", {
        "slug": "cd-post", "type": "blog", "title": LT("Hello"), "summary": LT("sum"), "content": LT("<p>x</p>"),
        "thumbnail": "", "is_published": True, "product_ids": ["{product_id}"], "category_id": "{post_category_id}"},
      capture={"post_id": "data.id"}),
    P("POST", "/admin/posts", "admin", {"slug": "cd-notice", "type": "notice", "title": LT("Notice"), "is_published": True},
      name="POST /admin/posts (notice)"),
    P("POST", "/admin/member-levels", "admin", {
        "name": LT("VIP"), "slug": "cd-vip", "icon": "", "discount_rate": 95, "recharge_threshold": 100,
        "spend_threshold": 200, "is_default": False, "sort_order": 2, "is_active": True},
      capture={"member_level_id": "data.id"}),
    P("POST", "/admin/member-level-prices/batch", "admin", {"prices": [
        {"member_level_id": "{member_level_id}", "product_id": "{product_id}", "sku_id": "{sku_id}", "price_amount": 11.5}]}),
    P("POST", "/admin/coupons", "admin", {
        "code": "CDSAVE", "type": "fixed", "value": 1, "min_amount": 0, "max_discount": 0, "usage_limit": 100,
        "per_user_limit": 5, "scope_ref_ids": ["{product_id}"], "payment_roles": [], "member_levels": [],
        "starts_at": "2020-01-01T00:00:00Z", "ends_at": "2099-01-01T00:00:00Z", "is_active": True},
      capture={"coupon_id": "data.id"}),
    P("POST", "/admin/promotions", "admin", {
        "name": "CD Promo", "type": "percent", "scope_ref_id": "{manual_product_id}", "value": 90, "min_amount": 0,
        "starts_at": "2020-01-01T00:00:00Z", "ends_at": "2099-01-01T00:00:00Z", "is_active": True},
      capture={"promotion_id": "data.id"}),
    P("POST", "/admin/gift-cards/generate", "admin", {"name": "CD Gift", "quantity": 2, "amount": "25.00",
                                                     "expires_at": "2099-01-01T00:00:00Z"}),
    G("/admin/gift-cards", "admin", name="GET /admin/gift-cards (seed)",
      capture={"gift_card_id": "data[0].id", "gift_code": "data[0].code"}),
    # users
    P("POST", "/auth/register", None, {"email": USER_EMAIL, "password": USER_PASS, "code": "", "agreement_accepted": True}),
    P("POST", "/auth/login", None, {"email": USER_EMAIL, "password": USER_PASS, "remember_me": True},
      capture={"user_token": "data.token", "user_id": "data.user.id"}),
    P("POST", "/admin/users/{user_id}/wallet/adjust", "admin", {"amount": "100.00", "operation": "add", "remark": "cd"}),
    P("POST", "/gift-cards/redeem", "user", {"code": "{gift_code}"}),
    P("POST", "/cart/items", "user", {"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 2}),
    P("POST", "/affiliate/open", "user", {}),
    # orders
    P("POST", "/orders/preview", "user", {"items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}],
                                          "coupon_code": "CDSAVE"}),
    P("POST", "/orders", "user", {"items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}]},
      capture={"user_order_no": "data.order_no"}),
    P("POST", "/orders", "user", {"items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}],
                                  "coupon_code": "CDSAVE"}, name="POST /orders (coupon)"),
    P("POST", "/payments", "user", {"order_no": "{user_order_no}", "channel_id": "{channel_id}"},
      capture={"user_payment_id": "data.payment_id"}),
    P("POST", "/orders/create-and-pay", "user", {
        "items": [{"product_id": "{product_id}", "sku_id": "{sku2_id}", "quantity": 1}], "use_balance": True},
      name="POST /orders/create-and-pay (wallet)", capture={"paid_order_no": "data.order_no"}),
    P("POST", "/orders", "user", {"items": [{"product_id": "{manual_product_id}", "sku_id": 0, "quantity": 1}],
                                  "manual_form_data": {}}, name="POST /orders (manual)",
      capture={"manual_order_no": "data.order_no"}),
    P("POST", "/payments", "user", {"order_no": "{manual_order_no}", "use_balance": True},
      name="POST /payments (wallet, manual order)"),
    P("POST", "/api-credential/apply", "user", {}),
    Step("wait for async jobs", "SLEEP", "3", compare=False),
    G("/admin/orders", "admin", name="GET /admin/orders (seed)", compare=False, capture={
        "paid_order_id": id_by_order_no("", "paid_order_no"),
        "manual_order_id": id_by_order_no("", "manual_order_no"),
        "manual_child_id": id_by_order_no("-01", "manual_order_no"),
        "user_order_id": id_by_order_no("", "user_order_no")}),
    P("POST", "/admin/fulfillments", "admin", {"order_id": "{manual_child_id}", "payload": "ACCOUNT-123"},
      name="POST /admin/fulfillments (manual)"),
    P("POST", "/admin/orders/{paid_order_id}/refund-to-wallet", "admin", {"amount": "1.00", "remark": "cd"}),
    G("/admin/order-refunds", "admin", name="GET /admin/order-refunds (seed)", capture={"refund_id": "data[0].id"}),
    P("POST", "/guest/orders/preview", None, {"email": GUEST_EMAIL, "order_password": GUEST_PASS,
                                               "items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}]}),
    P("POST", "/guest/orders/create-and-pay", None, {
        "email": GUEST_EMAIL, "order_password": GUEST_PASS, "channel_id": "{channel_id}",
        "items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}]},
      capture={"guest_order_no": "data.order_no", "guest_payment_id": "data.payment_id"}),
    P("POST", "/wallet/recharge", "user", {"amount": "10.00", "channel_id": "{channel_id}"},
      capture={"recharge_no": "data.recharge.recharge_no"}),
]


def admin_gets() -> list[Step]:
    paths = [
        "/admin/authz/me", "/admin/authz/roles", "/admin/authz/roles/role:operations/policies",
        "/admin/authz/admins", "/admin/authz/admins/1/roles", "/admin/authz/permissions/catalog",
        "/admin/authz/audit-logs", "/admin/authz/roles/cd_role/policies", "/admin/user-login-logs", "/admin/2fa/status", "/admin/compliance/status",
        "/admin/dashboard/overview", "/admin/dashboard/trends", "/admin/dashboard/rankings",
        "/admin/dashboard/inventory-alerts",
        "/admin/products", "/admin/products/{product_id}", "/admin/products/{manual_product_id}",
        "/admin/categories", "/admin/posts", "/admin/posts/{post_id}/products", "/admin/post-categories",
        "/admin/banners", "/admin/banners/{banner_id}", "/admin/media",
        "/admin/settings", "/admin/settings/smtp", "/admin/settings/captcha", "/admin/settings/telegram-auth",
        "/admin/settings/google-auth", "/admin/settings/affiliate", "/admin/settings/order-email-template",
        "/admin/settings/telegram-bot", "/admin/settings/telegram-bot/runtime-status",
        "/admin/settings/notification-center", "/admin/settings/notifications",
        "/admin/settings/notification-center/logs",
        "/admin/orders", "/admin/orders/{user_order_id}", "/admin/orders/{paid_order_id}",
        "/admin/order-refunds", "/admin/order-refunds/{refund_id}", "/admin/card-secrets", "/admin/card-secrets/stats?product_id={product_id}",
        "/admin/card-secrets/batches", "/admin/gift-cards",
        "/admin/coupons", "/admin/promotions", "/admin/member-levels", "/admin/member-level-prices",
        "/admin/payment-channels", "/admin/payment-channels/{channel_id}", "/admin/payments",
        "/admin/payments/{user_payment_id}",
        "/admin/users", "/admin/users/{user_id}", "/admin/users/{user_id}/coupon-usages",
        "/admin/users/{user_id}/wallet", "/admin/users/{user_id}/wallet/transactions", "/admin/wallet/recharges",
        "/admin/affiliates/users", "/admin/affiliates/commissions", "/admin/affiliates/withdraws",
        "/admin/resellers/operations/overview", "/admin/resellers/profiles", "/admin/resellers/domains",
        "/admin/resellers/site-configs", "/admin/resellers/product-settings", "/admin/resellers/operations/finance",
        "/admin/resellers/ledger-entries", "/admin/resellers/balance-accounts", "/admin/resellers/withdraws",
        "/admin/api-credentials", "/admin/authz/admins/2/roles", "/admin/site-connections", "/admin/product-mappings",
        "/admin/procurement-orders", "/admin/procurement-orders/stats", "/admin/reconciliation/jobs",
        "/admin/channel-clients", "/admin/channel-clients/{channel_client_id}", "/admin/telegram-bot/broadcasts", "/admin/telegram-bot/users",
        # not-found by id
        "/admin/products/" + str(MISSING_ID), "/admin/banners/" + str(MISSING_ID),
        "/admin/orders/" + str(MISSING_ID), "/admin/order-refunds/" + str(MISSING_ID),
        "/admin/payment-channels/" + str(MISSING_ID), "/admin/payments/" + str(MISSING_ID),
        "/admin/users/" + str(MISSING_ID), "/admin/api-credentials/" + str(MISSING_ID),
        "/admin/site-connections/" + str(MISSING_ID), "/admin/product-mappings/" + str(MISSING_ID),
        "/admin/procurement-orders/" + str(MISSING_ID), "/admin/reconciliation/jobs/" + str(MISSING_ID),
        "/admin/channel-clients/" + str(MISSING_ID), "/admin/telegram-bot/broadcasts/" + str(MISSING_ID),
        "/admin/resellers/profiles/" + str(MISSING_ID), "/admin/resellers/site-configs/" + str(MISSING_ID),
        # invalid id
        "/admin/products/abc", "/admin/orders/abc", "/admin/users/abc",
    ]
    steps = [G(p, "admin") for p in paths]
    steps += [G(f"/admin/settings?key={k}", "admin") for k in SETTING_KEYS]
    return steps


def other_checks() -> list[Step]:
    s: list[Step] = []
    # public
    for p in ["/public/config", "/public/products", "/public/products/cd-auto", "/public/products/cd-manual",
              "/public/products/nope-missing", "/public/categories", "/public/posts", "/public/posts?type=notice",
              "/public/posts/cd-post", "/public/posts/nope-missing", "/public/banners", "/public/post-categories",
              "/public/member-levels", "/public/captcha/image"]:
        s.append(G(p))
    s.append(G("/!/health"))
    s.append(G("/!/robots.txt"))
    s.append(G("/!/sitemap.xml"))
    s.append(P("POST", "/public/affiliate/click", None, {"affiliate_code": "NOPE0000", "visitor_key": "v1",
                                                          "landing_path": "/"}))
    # user
    for p in ["/me", "/me/login-logs", "/me/2fa/status", "/me/telegram", "/me/google", "/cart",
              "/orders", "/orders/stats", "/orders/{user_order_no}", "/orders/{paid_order_no}",
              "/orders/{manual_order_no}", "/orders/NOPE123", "/orders/{manual_order_no}/fulfillment/download", "/payments/latest?order_no={user_order_no}",
              "/wallet", "/wallet/transactions", "/wallet/recharges", "/wallet/recharges/stats",
              "/wallet/recharges/{recharge_no}", "/affiliate/dashboard", "/affiliate/commissions",
              "/affiliate/withdraws", "/api-credential", "/reseller/profile",
              "/orders/{paid_order_no}/fulfillment/download"]:
        s.append(G(p, "user"))
    s.append(P("POST", "/order/payment-channels", "user", {"amount": "12.30", "items": [
        {"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}]}))
    s.append(P("POST", "/wallet/payment-channels", "user", {"amount": "10.00"}))
    # guest
    for p in ["/guest/orders", "/guest/orders/{guest_order_no}", "/guest/payments/latest?order_no={guest_order_no}"]:
        s.append(G(p, "guest"))
    s.append(G("/guest/orders", None, name="GET /guest/orders (no guest header)"))
    s.append(P("POST", "/guest/payments", "guest", {"order_no": "{guest_order_no}", "channel_id": "{channel_id}"},
               name="POST /guest/payments (repeat)"))
    # errors / validation
    s += [
        P("POST", "/admin/login", None, {"username": ADMIN_USER, "password": "Wrong12345"}, name="admin login wrong password"),
        P("POST", "/admin/login", None, {"username": ADMIN_USER}, name="admin login missing password"),
        P("POST", "/auth/login", None, {"email": USER_EMAIL, "password": "Wrong12345"}, name="user login wrong password"),
        P("POST", "/auth/login", None, {"email": "nobody@example.com", "password": "Wrong12345"}, name="user login unknown email"),
        P("POST", "/auth/login", None, {"email": USER_EMAIL}, name="user login missing password"),
        P("POST", "/auth/register", None, {"email": USER_EMAIL, "password": USER_PASS, "agreement_accepted": True},
          name="register existing email"),
        P("POST", "/auth/register", None, {"email": "weak@example.com", "password": "weak", "agreement_accepted": True},
          name="register weak password"),
        P("POST", "/auth/register", None, {"email": "noagree@example.com", "password": USER_PASS},
          name="register without agreement"),
        G("/me", None, name="GET /me without auth"),
        G("/me", "badtoken", name="GET /me bad token"),
        G("/admin/products", None, name="GET /admin/products without auth"),
        G("/admin/products", "badtoken", name="GET /admin/products bad token"),
        G("/admin/products", "user", name="GET /admin/products with user token"),
        P("POST", "/admin/categories", "admin", {"name": LT("x")}, name="create category missing slug"),
        P("POST", "/admin/categories", "admin", {"slug": "cd-cat", "name": LT("dup")}, name="create category duplicate slug"),
        P("POST", "/admin/products", "admin", {"slug": "x"}, name="create product missing fields"),
        P("POST", "/admin/coupons", "admin", {}, name="create coupon empty body"),
        P("POST", "/admin/coupons", "admin", {"code": "X", "type": "fixed", "value": 0, "scope_ref_ids": []},
          name="create coupon zero value"),
        P("POST", "/admin/promotions", "admin", {}, name="create promotion empty body"),
        P("POST", "/admin/member-levels", "admin", {}, name="create member level empty body"),
        P("POST", "/admin/banners", "admin", {"name": ""}, name="create banner empty name"),
        P("POST", "/admin/posts", "admin", {}, name="create post empty body"),
        P("POST", "/admin/gift-cards/generate", "admin", {"name": "x"}, name="generate gift cards missing fields"),
        P("POST", "/admin/card-secrets/batch", "admin", {}, name="card secret batch empty body"),
        P("POST", "/admin/payment-channels", "admin", {"name": "x"}, name="create payment channel missing fields"),
        P("PUT", "/admin/categories/{category_id}", "admin", {}, name="update category empty body"),
        P("PUT", "/admin/products/{product_id}", "admin", {"slug": "cd-auto"}, name="update product missing fields"),
        P("POST", "/admin/compliance/acknowledge", "admin", {"segment1": "x"}, name="acknowledge missing segments"),
        P("POST", "/admin/categories", "admin", "not json", name="create category non-object body"),
        # binding:"..." rule messages (ginutil.RespondBindError) on every switched endpoint
        P("PUT", "/admin/settings", "admin", {}, name="bind PUT /admin/settings"),
        P("POST", "/admin/settings/smtp/test", "admin", {}, name="bind POST /admin/settings/smtp/test"),
        P("POST", "/admin/authz/roles", "admin", {}, name="bind POST /admin/authz/roles"),
        P("POST", "/admin/authz/policies", "admin", {'role': 'r'}, name="bind POST /admin/authz/policies"),
        P("POST", "/admin/authz/admins", "admin", {}, name="bind POST /admin/authz/admins"),
        P("PUT", "/admin/users/batch-status", "admin", {'status': 'active'}, name="bind PUT /admin/users/batch-status"),
        P("PATCH", f"/admin/orders/{MISSING_ID}", "admin", {}, name="bind PATCH /admin/orders/:id"),
        P("POST", f"/admin/orders/{MISSING_ID}/refund-to-wallet", "admin", {}, name="bind POST /admin/orders/:id/refund-to-wallet"),
        P("POST", f"/admin/orders/{MISSING_ID}/manual-refund", "admin", {}, name="bind POST /admin/orders/:id/manual-refund"),
        P("PATCH", f"/admin/order-refunds/{MISSING_ID}/payment-fee", "admin", {}, name="bind PATCH /admin/order-refunds/:id/payment-fee"),
        P("POST", "/admin/fulfillments", "admin", {}, name="bind POST /admin/fulfillments"),
        P("PATCH", f"/admin/products/{MISSING_ID}/wholesale-prices", "admin", {}, name="bind PATCH /admin/products/:id/wholesale-prices"),
        P("POST", "/admin/products/batch-delete", "admin", {'ids': []}, name="bind POST /admin/products/batch-delete"),
        P("POST", "/admin/products/batch-status", "admin", {}, name="bind POST /admin/products/batch-status"),
        P("POST", "/admin/card-secrets/export-available", "admin", {}, name="bind POST /admin/card-secrets/export-available"),
        P("PATCH", "/admin/card-secrets/batch-status", "admin", {'ids': [1]}, name="bind PATCH /admin/card-secrets/batch-status"),
        P("POST", "/admin/card-secrets/export", "admin", {'ids': [1]}, name="bind POST /admin/card-secrets/export"),
        P("POST", "/admin/post-categories", "admin", {}, name="bind POST /admin/post-categories"),
        P("PATCH", f"/admin/post-categories/{MISSING_ID}/status", "admin", {}, name="bind PATCH /admin/post-categories/:id/status"),
        P("POST", "/admin/media/batch-delete", "admin", {'ids': []}, name="bind POST /admin/media/batch-delete"),
        P("POST", "/admin/member-level-prices/batch", "admin", {}, name="bind POST /admin/member-level-prices/batch"),
        P("PATCH", "/admin/gift-cards/batch-status", "admin", {'ids': []}, name="bind PATCH /admin/gift-cards/batch-status"),
        P("POST", "/admin/gift-cards/export", "admin", {}, name="bind POST /admin/gift-cards/export"),
        P("POST", f"/admin/users/{MISSING_ID}/wallet/adjust", "admin", {}, name="bind POST /admin/users/:id/wallet/adjust"),
        P("PATCH", "/admin/affiliates/users/batch-status", "admin", {}, name="bind PATCH /admin/affiliates/users/batch-status"),
        P("PATCH", f"/admin/affiliates/users/{MISSING_ID}/status", "admin", {}, name="bind PATCH /admin/affiliates/users/:id/status"),
        P("POST", "/admin/product-mappings/import", "admin", {}, name="bind POST /admin/product-mappings/import"),
        P("POST", "/admin/product-mappings/batch-import", "admin", {'connection_id': 1, 'upstream_product_ids': []}, name="bind POST /admin/product-mappings/batch-import"),
        P("POST", "/admin/product-mappings/batch-sync", "admin", {'ids': []}, name="bind POST /admin/product-mappings/batch-sync"),
        P("POST", f"/admin/api-credentials/{MISSING_ID}/reject", "admin", {}, name="bind POST /admin/api-credentials/:id/reject"),
        P("PUT", f"/admin/site-connections/{MISSING_ID}/status", "admin", {}, name="bind PUT /admin/site-connections/:id/status"),
        P("POST", "/admin/settings/notification-center/test", "admin", {}, name="bind POST /admin/settings/notification-center/test"),
        P("POST", "/admin/channel-clients", "admin", {'name': 'Bot'}, name="bind POST /admin/channel-clients"),
        P("PUT", f"/admin/channel-clients/{MISSING_ID}/status", "admin", {'status': 2}, name="bind PUT /admin/channel-clients/:id/status"),
        P("POST", "/admin/telegram-bot/broadcasts", "admin", {}, name="bind POST /admin/telegram-bot/broadcasts"),
        P("POST", "/cart/items", "user", {}, name="bind POST /cart/items"),
        P("POST", "/orders", "user", {'coupon_code': 'x'}, name="bind POST /orders"),
        P("POST", "/orders/create-and-pay", "user", {}, name="bind POST /orders/create-and-pay"),
        P("POST", "/order/payment-channels", "user", {}, name="bind POST /order/payment-channels"),
        P("POST", "/payments", "user", {'channel_id': 1}, name="bind POST /payments"),
        P("POST", "/wallet/recharge", "user", {}, name="bind POST /wallet/recharge"),
        P("POST", "/wallet/payment-channels", "user", {}, name="bind POST /wallet/payment-channels"),
        P("POST", "/gift-cards/redeem", "user", {}, name="bind POST /gift-cards/redeem"),
        P("PUT", "/me/password", "user", {'old_password': 'x'}, name="bind PUT /me/password"),
        P("POST", "/me/email/change", "user", {}, name="bind POST /me/email/change"),
        P("POST", "/me/email/send-verify-code", "user", {}, name="bind POST /me/email/send-verify-code"),
        P("POST", "/me/2fa/enable", "user", {}, name="bind POST /me/2fa/enable"),
        P("POST", "/me/telegram/bind", "user", {'id': 1}, name="bind POST /me/telegram/bind"),
        P("POST", "/me/telegram/oidc/callback", "user", {'code': 'c'}, name="bind POST /me/telegram/oidc/callback"),
        P("POST", "/me/google/bind", "user", {}, name="bind POST /me/google/bind"),
        P("POST", "/public/affiliate/click", None, {}, name="bind POST /public/affiliate/click"),
        P("POST", "/affiliate/withdraws", "user", {'amount': '1'}, name="bind POST /affiliate/withdraws"),
        P("POST", "/guest/orders", None, {'items': []}, name="bind POST /guest/orders"),
        P("POST", "/guest/orders/preview", None, {}, name="bind POST /guest/orders/preview"),
        P("POST", "/guest/payments", None, {'order_no': 'DJ1'}, name="bind POST /guest/payments"),
        P("POST", "/auth/send-verify-code", None, {'email': 'a@b.co'}, name="bind POST /auth/send-verify-code"),
        P("POST", "/auth/login/verify-2fa", None, {'code': '1'}, name="bind POST /auth/login/verify-2fa"),
        P("POST", "/auth/google/login", None, {}, name="bind POST /auth/google/login"),
        P("POST", "/auth/telegram/login", None, {}, name="bind POST /auth/telegram/login"),
        P("POST", "/auth/telegram/oidc/callback", None, {}, name="bind POST /auth/telegram/oidc/callback"),
        P("POST", "/orders/create-and-pay", "user", {"items": [
            {"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 50}], "channel_id": "{channel_id}"},
          name="create-and-pay insufficient card secrets"),
        P("POST", "/orders", "user", {"items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 50}]},
          name="create order insufficient card secrets"),
        P("POST", "/orders/preview", "user", {"items": []}, name="preview empty items"),
        P("POST", "/orders/preview", "user", {"items": [{"product_id": MISSING_ID, "quantity": 1}]},
          name="preview missing product"),
        P("POST", "/orders/preview", "user", {"items": [{"product_id": "{product_id}", "sku_id": "{sku_id}", "quantity": 1}],
                                              "coupon_code": "NOPE"}, name="preview invalid coupon"),
        P("POST", "/guest/orders/preview", None, {"email": GUEST_EMAIL, "order_password": GUEST_PASS, "items": [
            {"product_id": "{manual_product_id}", "quantity": 1}]}, name="guest preview member-only product"),
        P("POST", "/gift-cards/redeem", "user", {"code": "NOPE-NOPE"}, name="redeem invalid gift card"),
        P("POST", "/payments", "user", {"order_no": "NOPE", "channel_id": "{channel_id}"}, name="pay missing order"),
        P("POST", "/orders/NOPE/cancel", "user", {}, name="cancel missing order"),
        P("PUT", "/admin/settings", "admin", {"key": "google_auth_config", "value": {}}, name="settings google via generic"),
        G("/admin/settings?key=nope_key", "admin", name="settings unknown key"),
        P("POST", "/orders/{user_order_no}/cancel", "user", {}, name="POST /orders/:order_no/cancel"),
        P("POST", "/orders/{user_order_no}/cancel", "user", {}, name="cancel already canceled order"),
    ]
    return s


# --------------------------------------------------------------------------- runner
@dataclass
class Result:
    step: str
    method: str
    path: str
    orig: Resp
    ours: Resp


def run_step(step: Step, side: Side) -> Resp:
    if step.method == "SLEEP":
        if side.name == "ours":
            time.sleep(float(step.path))
        return Resp(200, None, True)
    path = subst(step.path, side.ids)
    body = subst(step.body, side.ids) if step.body is not None else None
    r = side.call(step.method, path, body, step.auth, step.headers)
    for var, p in step.capture.items():
        if not r.is_json:
            val = None
        elif callable(p):
            val = p(r.body, side.ids)
        else:
            val = dig(r.body, p)
        if val is not None:
            side.ids[var] = val
            if var == "admin_token":
                side.admin_token = val
            if var == "user_token":
                side.user_token = val
    return r


def compare(res: Result, diffs: list, notes: list, timefmt: dict) -> None:
    o, r, name = res.orig, res.ours, res.step
    if o.status != r.status:
        diffs.append(Diff(name, "<http>", "http_status", str(o.status), str(r.status)))
    if not o.is_json or not r.is_json:
        if o.is_json != r.is_json:
            diffs.append(Diff(name, "<body>", "content_type",
                              "json" if o.is_json else o.body.get("__raw__", "?"),
                              "json" if r.is_json else r.body.get("__raw__", "?")))
        elif o.body.get("__raw__") != r.body.get("__raw__"):
            diffs.append(Diff(name, "<body>", "content_type", o.body.get("__raw__", "?"), r.body.get("__raw__", "?")))
        return
    ob, rb = o.body, r.body
    if isinstance(ob, dict) and isinstance(rb, dict):
        for key in ("status_code", "msg"):
            if key in ob and key in rb and ob[key] != rb[key]:
                diffs.append(Diff(name, key, "value", json.dumps(ob[key], ensure_ascii=False),
                                  json.dumps(rb[key], ensure_ascii=False)))
        po, pr = ob.get("pagination"), rb.get("pagination")
        if isinstance(po, dict) and isinstance(pr, dict):
            for key in ("page", "page_size"):
                if po.get(key) != pr.get(key):
                    diffs.append(Diff(name, f"pagination.{key}", "value", str(po.get(key)), str(pr.get(key))))
    tf_o, tf_r = set(), set()
    time_formats(ob, tf_o)
    time_formats(rb, tf_r)
    timefmt.setdefault("orig", set()).update(tf_o)
    timefmt.setdefault("ours", set()).update(tf_r)
    diff_shapes(name, "", shape(ob), shape(rb), diffs, notes)


def load_deviations(path: str) -> list:
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def label(diffs: list, devs: list) -> None:
    for d in diffs:
        for dev in devs:
            if (re.search(dev.get("check", ".*"), d.check) and re.search(dev.get("path", ".*"), d.path)
                    and re.fullmatch(dev.get("kind", ".*"), d.kind)):
                d.label, d.reason = dev.get("label", "intentional"), dev["reason"]
                break


def md_escape(s: str) -> str:
    return s.replace("|", "\\|")


def write_report(out_dir: str, results: list, diffs: list, notes: list, timefmt: dict, seed_failures: list) -> str:
    os.makedirs(out_dir, exist_ok=True)
    lines = ["# Contract diff (generated)", "",
             f"Generated {time.strftime('%Y-%m-%d %H:%M:%S')}. Endpoints/flows compared: **{len(results)}**. "
             f"Differences: **{len(diffs)}** "
             f"({sum(1 for d in diffs if not d.label)} unlabelled, {sum(1 for d in diffs if d.label)} documented).", ""]
    if seed_failures:
        lines += ["## Seed steps that failed on a side", ""] + [f"- {s}" for s in seed_failures] + [""]
    lines += ["## Timestamp formats observed", "",
              f"- orig: {', '.join(sorted(timefmt.get('orig', [])))}",
              f"- ours: {', '.join(sorted(timefmt.get('ours', [])))}", ""]
    lines += ["## Unlabelled differences", "", "| check | path | kind | orig | ours |", "|---|---|---|---|---|"]
    for d in diffs:
        if not d.label:
            lines.append(f"| {md_escape(d.check)} | `{md_escape(d.path)}` | {d.kind} | {md_escape(d.orig)} | {md_escape(d.ours)} |")
    lines += ["", "## Documented deviations", "", "| check | path | kind | orig | ours | class | reason |", "|---|---|---|---|---|---|---|"]
    for d in diffs:
        if d.label:
            lines.append(f"| {md_escape(d.check)} | `{md_escape(d.path)}` | {d.kind} | {md_escape(d.orig)} | "
                         f"{md_escape(d.ours)} | {d.label} | {md_escape(d.reason)} |")
    lines += ["", "## Coverage notes (one side returned an empty list, elements not compared)", ""]
    lines += [f"- {n}" for n in sorted(set(notes))]
    path = os.path.join(out_dir, "diff.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    raw = [{"check": r.step, "method": r.method, "path": r.path,
            "orig": {"status": r.orig.status, "body": r.orig.body},
            "ours": {"status": r.ours.status, "body": r.ours.body}} for r in results]
    with open(os.path.join(out_dir, "responses.json"), "w", encoding="utf-8") as f:
        json.dump(raw, f, ensure_ascii=False, indent=1, default=str)
    return path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--orig", required=True, help="base URL of the original Go backend (fresh DB)")
    ap.add_argument("--ours", required=True, help="base URL of the Rust backend (fresh DB)")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "out"))
    ap.add_argument("--deviations", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "deviations.json"))
    ap.add_argument("--only", default="", help="regex: only report checks whose name matches")
    args = ap.parse_args()

    orig, ours = Side("orig", args.orig.rstrip("/")), Side("ours", args.ours.rstrip("/"))
    results: list[Result] = []
    seed_failures: list[str] = []
    for step in SEED + admin_gets() + other_checks():
        ro, rr = run_step(step, orig), run_step(step, ours)
        is_seed = step in SEED
        if is_seed:
            for side, resp in (("orig", ro), ("ours", rr)):
                code = resp.body.get("status_code") if resp.is_json and isinstance(resp.body, dict) else None
                failed = resp.status != 200 or code not in (0, None)
                if failed and "before ack" not in step.name:
                    msg = resp.body.get("msg") if isinstance(resp.body, dict) else resp.body
                    seed_failures.append(f"{step.name} [{side}] -> http {resp.status} status_code={code} msg={msg}")
        if step.compare:
            results.append(Result(step.name, step.method, step.path, ro, rr))

    if args.only:
        results = [r for r in results if re.search(args.only, r.step)]
    diffs: list[Diff] = []
    notes: list[str] = []
    timefmt: dict = {}
    for res in results:
        compare(res, diffs, notes, timefmt)
    label(diffs, load_deviations(args.deviations))
    path = write_report(args.out, results, diffs, notes, timefmt, seed_failures)
    unl = [d for d in diffs if not d.label]
    print(f"compared {len(results)} endpoints/flows; {len(diffs)} differences "
          f"({len(unl)} unlabelled); seed failures: {len(seed_failures)}")
    for s in seed_failures:
        print("  SEED FAIL:", s)
    for d in unl:
        print(f"  [{d.kind}] {d.check} :: {d.path}  orig={d.orig}  ours={d.ours}")
    print("report:", path)
    return 1 if unl else 0


if __name__ == "__main__":
    sys.exit(main())
