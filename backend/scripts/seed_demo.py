#!/usr/bin/env python3
"""Seeds demo data into a running Zebra Store backend through the admin API.

Usage: scripts/seed_demo.py [base_url] [admin_user] [admin_password]
Defaults: http://localhost:8081 admin Admin12345. Safe to run once on an empty database.
"""
import json
import sys
import urllib.request

BASE = (sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8081").rstrip("/") + "/api/v1"
USER = sys.argv[2] if len(sys.argv) > 2 else "admin"
PASSWORD = sys.argv[3] if len(sys.argv) > 3 else "Admin12345"
TOKEN = ""


def call(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    with urllib.request.urlopen(req) as res:
        out = json.loads(res.read() or b"null")
    status = "ok" if out.get("status_code") == 0 else f"FAILED {out.get('status_code')} {out.get('msg')}"
    print(f"{method:6} {path:45} {status}")
    return out.get("data")


def zh(text, en=None):
    return {"zh-CN": text, "zh-TW": text, "en-US": en or text}


login = call("POST", "/admin/login", {"username": USER, "password": PASSWORD})
TOKEN = login["token"]

call("POST", "/admin/compliance/acknowledge", {
    "segment1": "我已阅读并理解上述合规声明提醒",
    "segment2": "知悉相关法律风险",
    "segment3": "并确认自行承担部署运营和收费行为产生的法律责任",
})
call("PUT", "/admin/settings", {"key": "registration_config", "value": {
    "registration_enabled": True, "email_verification_enabled": False,
    "email_domain_allowlist_enabled": False, "allowed_email_domains": []}})
site = call("GET", "/admin/settings?key=site_config") or {}
site.setdefault("brand", {})
site["brand"].update({
    "site_name": "斑马小铺 Zebra Store",
    "site_description": zh("二次元风格的数字商品小店，自动发货，安心购买 ✦", "Anime-style digital goods shop"),
})
call("PUT", "/admin/settings", {"key": "site_config", "value": site})
call("PUT", "/admin/settings", {"key": "home_announcement", "value": {
    "enabled": True, "type": "info", "title": zh("欢迎光临 ✿"),
    "content": zh("<p>这是局域网演示站点，支付为测试渠道。</p>")}})

games = call("POST", "/admin/categories", {"name": zh("游戏点卡", "Game Cards"), "slug": "game-cards", "sort_order": 3})
video = call("POST", "/admin/categories", {"name": zh("影音会员", "Streaming"), "slug": "streaming", "sort_order": 2})
tools = call("POST", "/admin/categories", {"name": zh("软件工具", "Software"), "slug": "software", "sort_order": 1})

steam = call("POST", "/admin/products", {
    "category_id": games["id"], "slug": "steam-100", "title": zh("Steam 充值卡 100 元", "Steam Card 100"),
    "description": zh("自动发货，付款后秒到账"), "content": zh("<p>使用说明：登录 Steam → 账户 → 兑换充值码。</p>"),
    "images": ["https://picsum.photos/seed/zebra-steam/800/600"], "tags": ["热卖", "自动发货"],
    "price_amount": 98, "purchase_type": "guest", "fulfillment_type": "auto", "is_active": True,
    "min_purchase_quantity": 1, "max_purchase_quantity": 10, "stock_display_mode": "exact", "sort_order": 10,
})
call("POST", "/admin/card-secrets/batch", {
    "product_id": steam["id"], "batch_no": "DEMO-STEAM-1", "note": "demo", "deduplicate": True,
    "secrets": [f"STEAM-DEMO-{i:04d}-ZEBRA" for i in range(1, 21)],
})
call("POST", "/admin/products", {
    "category_id": video["id"], "slug": "netflix-month", "title": zh("Netflix 月度会员", "Netflix Monthly"),
    "description": zh("人工发货，工作时间 10 分钟内处理"),
    "images": ["https://picsum.photos/seed/zebra-netflix/800/600"], "tags": ["人工发货"],
    "price_amount": 25, "purchase_type": "member", "fulfillment_type": "manual", "manual_stock_total": 50,
    "is_active": True, "sort_order": 8,
    "skus": [
        {"sku_code": "basic", "spec_values": zh("基础版", "Basic"), "price_amount": 25, "manual_stock_total": 20, "is_active": True},
        {"sku_code": "premium", "spec_values": zh("高级版", "Premium"), "price_amount": 45, "manual_stock_total": 10, "is_active": True},
    ],
})
office = call("POST", "/admin/products", {
    "category_id": tools["id"], "slug": "office-key", "title": zh("Office 365 年度订阅", "Office 365 Annual"),
    "description": zh("自动发货，正版激活码"),
    "images": ["https://picsum.photos/seed/zebra-office/800/600"], "tags": ["自动发货"],
    "price_amount": 129, "purchase_type": "guest", "fulfillment_type": "auto", "is_active": True, "sort_order": 6,
})
call("POST", "/admin/card-secrets/batch", {
    "product_id": office["id"], "batch_no": "DEMO-OFFICE-1", "note": "demo", "deduplicate": True,
    "secrets": [f"OFFICE-{i:04d}-XXXX-YYYY" for i in range(1, 11)],
})
call("POST", "/admin/promotions", {"name": "限时 8 折", "scope_ref_id": steam["id"], "type": "percent", "value": 20, "min_amount": 0, "is_active": True})
call("POST", "/admin/coupons", {"code": "WELCOME10", "type": "percent", "value": 10, "min_amount": 0,
                                "usage_limit": 100, "per_user_limit": 1, "is_active": True, "scope_ref_ids": [steam["id"], office["id"]]})
call("POST", "/admin/member-levels", {"name": zh("黄金会员", "Gold"), "slug": "gold", "discount_rate": 95,
                                      "recharge_threshold": 100, "spend_threshold": 500, "sort_order": 1, "is_active": True})
call("POST", "/admin/banners", {"name": "首页横幅", "position": "home_hero", "title": zh("欢迎来到斑马小铺 ✦", "Welcome to Zebra Store"),
                                "subtitle": zh("自动发货 · 安全可靠 · 二次元风味"), "image": "https://picsum.photos/seed/zebra-hero/1600/600",
                                "link_type": "internal", "link_value": "/products", "is_active": True, "sort_order": 1})
call("POST", "/admin/posts", {"type": "blog", "slug": "how-to-buy", "title": zh("新手购买教程"), "summary": zh("三步完成购买"),
                              "content": zh("<p>1. 选择商品 2. 填写邮箱与查询密码 3. 完成支付后在订单页查看卡密。</p>"), "is_published": True})
call("POST", "/admin/posts", {"type": "notice", "slug": "open", "title": zh("开业公告"), "summary": zh("斑马小铺正式开业"),
                              "content": zh("<p>欢迎体验～</p>"), "is_published": True})
call("POST", "/admin/payment-channels", {
    "name": "支付宝（演示）", "provider_type": "epay", "channel_type": "alipay", "interaction_mode": "redirect",
    "fee_rate": 0, "is_active": True, "sort_order": 1,
    "config_json": {"epay_version": "v1", "gateway_url": "https://pay.example.com", "merchant_id": "1000", "merchant_key": "demo-key",
                    "notify_url": "http://localhost:8081/api/v1/payments/callback", "return_url": "http://localhost:5185/pay"},
})
print("done")
