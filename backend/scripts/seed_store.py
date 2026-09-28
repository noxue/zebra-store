#!/usr/bin/env python3
"""Seeds the AI / cloud account test catalogue through the admin API.

Usage: scripts/seed_store.py [base_url] [admin_user] [admin_password]
Defaults: http://localhost:8081 admin Admin12345.
Uploads the covers from scripts/covers (run scripts/gen_covers.py first) and
creates categories, products, SKUs and clearly-marked TEST card secrets.
Idempotent per slug: products/categories that already exist are skipped.
"""
import json
import os
import sys
import urllib.request
import uuid

BASE = (sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8081").rstrip("/") + "/api/v1"
USER = sys.argv[2] if len(sys.argv) > 2 else "admin"
PASSWORD = sys.argv[3] if len(sys.argv) > 3 else "Admin12345"
COVERS = os.path.join(os.path.dirname(__file__), "covers")
TOKEN = ""


def request(req):
    if TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    with urllib.request.urlopen(req) as res:
        out = json.loads(res.read() or b"null")
    if out.get("status_code") != 0:
        raise SystemExit(f"{req.get_method()} {req.full_url} FAILED {out.get('status_code')} {out.get('msg')}")
    return out.get("data")


def call(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    out = request(req)
    print(f"{method:6} {path}")
    return out


def upload(path):
    boundary = uuid.uuid4().hex
    with open(path, "rb") as f:
        content = f.read()
    body = (
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"scene\"\r\n\r\nproduct\r\n"
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{os.path.basename(path)}\"\r\n"
        f"Content-Type: image/png\r\n\r\n"
    ).encode() + content + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(BASE + "/admin/upload", data=body, method="POST")
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    url = request(req)["url"]
    print(f"UPLOAD {os.path.basename(path)} -> {url}")
    return url


def zh(text, en=None):
    return {"zh-CN": text, "zh-TW": text, "en-US": en or text}


def secrets(prefix, n):
    return [f"TEST-{prefix}-{i:03d} | 测试卡密，非真实账号" for i in range(1, n + 1)]


CATEGORIES = [
    ("ai-accounts", zh("AI 账号", "AI Accounts"), 30),
    ("cloud-accounts", zh("云服务账号", "Cloud Accounts"), 20),
    ("google-accounts", zh("谷歌账号", "Google Accounts"), 10),
]

NOTICE = "<p>⚠️ 本商品为<strong>测试商品</strong>，卡密为测试数据，不代表真实账号。</p>"

# slug, category, title, description, price, fulfillment, tags, skus, secrets per sku(or product)
PRODUCTS = [
    ("chatgpt-plus", "ai-accounts", zh("ChatGPT Plus 成品账号", "ChatGPT Plus Account"),
     zh("独享成品号，自动发货，登录即用"), "auto", ["AI", "自动发货", "热卖"],
     [("month", zh("月卡", "1 Month"), 139), ("quarter", zh("季卡", "3 Months"), 399)], 8),
    ("chatgpt-team", "ai-accounts", zh("ChatGPT Team 席位", "ChatGPT Team Seat"),
     zh("团队席位邀请，人工处理，工作时间 30 分钟内"), "manual", ["AI", "人工发货"],
     [("seat-month", zh("单席位 / 月", "1 Seat / Month"), 169)], 0),
    ("claude-pro", "ai-accounts", zh("Claude Pro 成品账号", "Claude Pro Account"),
     zh("独享成品号，自动发货，长文与编程利器"), "auto", ["AI", "自动发货", "热卖"],
     [("month", zh("月卡", "1 Month"), 149)], 10),
    ("claude-max", "ai-accounts", zh("Claude Max 高额度账号", "Claude Max Account"),
     zh("高额度套餐，适合重度用户，自动发货"), "auto", ["AI", "自动发货"],
     [("5x", zh("5x 额度", "5x Usage"), 729), ("20x", zh("20x 额度", "20x Usage"), 1399)], 3),
    ("aws-account", "cloud-accounts", zh("AWS 账号", "AWS Account"),
     zh("多规格可选，含控制台登录信息，自动发货"), "auto", ["云服务", "自动发货"],
     [("fresh", zh("新注册号", "Fresh"), 69), ("credit", zh("带 Credits 额度", "With Credits"), 299)], 6),
    ("gcp-account", "cloud-accounts", zh("GCP 账号", "GCP Account"),
     zh("含试用额度，自动发货"), "auto", ["云服务", "自动发货"],
     [("trial", zh("$300 试用额度", "$300 Trial"), 89)], 8),
    ("google-account", "google-accounts", zh("Google 账号", "Google Account"),
     zh("邮箱账号，新号 / 老号可选，自动发货"), "auto", ["谷歌", "自动发货"],
     [("new", zh("新号", "New"), 9.9), ("aged", zh("老号（1 年+）", "Aged 1y+"), 29)], 15),
]


def main():
    global TOKEN
    TOKEN = call("POST", "/admin/login", {"username": USER, "password": PASSWORD})["token"]

    existing_cats = {c["slug"]: c["id"] for c in (call("GET", "/admin/categories") or [])}
    cat_ids = {}
    for slug, name, order in CATEGORIES:
        cat_ids[slug] = existing_cats.get(slug) or call(
            "POST", "/admin/categories", {"name": name, "slug": slug, "sort_order": order})["id"]

    listed = call("GET", "/admin/products?page=1&page_size=200") or []
    existing = {p["slug"] for p in listed}
    for i, (slug, cat, title, desc, fulfil, tags, skus, per_sku) in enumerate(PRODUCTS):
        if slug in existing:
            print(f"skip   {slug} (exists)")
            continue
        image = upload(os.path.join(COVERS, f"{slug}.png"))
        body = {
            "category_id": cat_ids[cat], "slug": slug, "title": title, "description": desc,
            "content": zh(NOTICE + "<p>购买后在订单详情页查看卡密。</p>"),
            "images": [image], "tags": tags,
            "price_amount": min(s[2] for s in skus), "purchase_type": "guest",
            "fulfillment_type": fulfil, "is_active": True, "sort_order": 100 - i,
            "min_purchase_quantity": 1, "max_purchase_quantity": 5, "stock_display_mode": "exact",
            "skus": [{"sku_code": code, "spec_values": spec, "price_amount": price, "is_active": True,
                      **({"manual_stock_total": 50} if fulfil == "manual" else {})}
                     for code, spec, price in skus],
        }
        if fulfil == "manual":
            body["manual_stock_total"] = 50
        product = call("POST", "/admin/products", body)
        if fulfil == "auto" and per_sku:
            detail = call("GET", f"/admin/products/{product['id']}")
            for sku in detail.get("skus") or []:
                code = sku["sku_code"]
                call("POST", "/admin/card-secrets/batch", {
                    "product_id": product["id"], "sku_id": sku["id"], "batch_no": f"TEST-{slug}-{code}",
                    "note": "test data", "deduplicate": True,
                    "secrets": secrets(f"{slug}-{code}".upper(), per_sku)})
    print("done")


if __name__ == "__main__":
    main()
