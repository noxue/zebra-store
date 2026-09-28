#!/usr/bin/env python3
"""Configures the whole zebra-lab topology end-to-end through each system's HTTP APIs.

Idempotent: every step looks up what exists before creating it; ids / keys / tokens are
kept in /lab/state/state.json and a human summary in /lab/state/credentials.txt.

Stages (default: all, in order):
  acg-install   install 异次元 acg-faka (needs no login)
  (lab.sh then disables the acg admin-login captcha — the only non-HTTP step, see README)
  store         main Zebra: settings, seed catalogue, lab e2e product, buyer + wallet
  acg           acg-faka: settings, category/commodity/cards, supplier member + balance + app_key
  dujiao        dujiao-next: settings, product + cards, supplier user + API credential + wallet
  connections   store -> acg-faka and store -> dujiao-next connections, import + activate
  zs2           zs2 -> store (zebra-store protocol, connection code), import + activate
  resellers     3 reseller users apply -> approve -> subdomain -> site config -> pricing
  acg-shared    acg-faka buys from store through /shared/* (共享店铺)
"""
import os
import sys
import urllib.error

sys.path.insert(0, os.path.dirname(__file__))
from lab import (  # noqa: E402
    Acg, Api, ApiError, RESELLERS, USER_PASSWORD, host, load_state, log, save_state, url,
    wait_for, zebra_admin, zebra_user, MODE, STATE_DIR,
)

E = os.environ
COMPLIANCE = {
    "segment1": "我已阅读并理解上述合规声明提醒",
    "segment2": "知悉相关法律风险",
    "segment3": "并确认自行承担部署运营和收费行为产生的法律责任",
}
LAB_PRODUCT = "lab-e2e-card"      # store product sold to zs2, resellers and acg-faka
LAB_PRICE = "10.00"
ACG_ITEM = "ACG 测试点卡 10元"
ACG_CATEGORY = "Lab 游戏点卡"
DJ_SLUG = "dj-lab-card"
CARDS = 120                       # secrets per supplier product (each verify run uses one)


def zh(text, en=None):
    return {"zh-CN": text, "zh-TW": text, "en-US": en or text}


def email(local):
    return f"{local}@lab.test"


# ================================================================= Zebra helpers
def zebra_basics(admin, title, site):
    admin.post("/admin/compliance/acknowledge", COMPLIANCE)
    admin.put("/admin/settings", {"key": "registration_config", "value": {
        "registration_enabled": True, "email_verification_enabled": False,
        "email_domain_allowlist_enabled": False, "allowed_email_domains": []}})
    cfg = admin.get("/admin/settings?key=site_config") or {}
    cfg.setdefault("brand", {})
    cfg["brand"].update({"site_name": title, "site_url": url(site)})
    cfg["currency"] = "CNY"
    admin.put("/admin/settings", {"key": "site_config", "value": cfg})


def user_id(api):
    return api.get("/me")["id"]


def wallet_top_up(admin, uid, want):
    """Adds funds so the user's wallet holds at least `want`."""
    wallet = admin.get(f"/admin/users/{uid}/wallet") or {}
    bal = float((wallet.get("account") or wallet).get("balance") or 0)
    if bal < float(want):
        admin.post(f"/admin/users/{uid}/wallet/adjust",
                   {"amount": f"{float(want) - bal:.2f}", "operation": "add", "remark": "zebra-lab top-up"})


def approve_credential(admin, user_api, mail):
    """User applies for an API credential; admin approves it. Returns the credential."""
    cred = user_api.get("/api-credential", ok_codes=(0, 404)) or {}
    if not cred or cred.get("status") in (None, "", "none"):
        user_api.post("/api-credential/apply")
        cred = user_api.get("/api-credential") or {}
    if cred.get("status") == "pending_review":
        # Filter by user_id because this compatibility service rejects its search filter.
        rows, _ = admin.page(f"/admin/api-credentials?status=pending_review&user_id={user_id(user_api)}")
        for row in rows or []:
            admin.post(f"/admin/api-credentials/{row['id']}/approve")
        cred = user_api.get("/api-credential")
    if cred.get("status") != "approved":
        raise SystemExit(f"credential of {mail} not approved: {cred}")
    return cred


def find_product(admin, slug):
    rows, _ = admin.page("/admin/products?page=1&page_size=200")
    for p in rows or []:
        if p.get("slug") == slug:
            return admin.get(f"/admin/products/{p['id']}")
    return None


def connection_by_name(admin, name):
    rows, _ = admin.page("/admin/site-connections?page=1&page_size=100")
    for c in rows or []:
        if c.get("name") == name:
            return c
    return None


def ensure_connection(admin, name, protocol, base_url, key, secret, markup, callback=None):
    conn = connection_by_name(admin, name)
    if conn:
        return conn
    hs = admin.post("/admin/site-connections/handshake",
                    {"base_url": base_url, "api_key": key, "api_secret": secret, "protocol": protocol})
    if not hs.get("ok"):
        raise SystemExit(f"handshake {name} failed: {hs}")
    if callback is None:
        callback = hs.get("suggested_callback_url") or ""
    body = {"name": name, "base_url": base_url, "api_key": key, "api_secret": secret, "protocol": protocol,
            "callback_url": callback, "price_markup_percent": markup, "price_rounding_mode": "none",
            "exchange_rate": "1", "auto_sync_price": True}
    conn = admin.post("/admin/site-connections", body)
    conn = admin.get(f"/admin/site-connections/{conn['id']}")
    log(f"  connection {name}: status={conn.get('status')} sync={conn.get('sync_mode')}")
    return conn


def import_upstream(admin, conn_id, match, category_id=None):
    """Imports the first upstream product whose title matches; activates it. Returns local product."""
    maps, _ = admin.page(f"/admin/product-mappings?connection_id={conn_id}&page=1&page_size=100")
    items = wait_for(lambda: (admin.get(f"/admin/upstream-products?connection_id={conn_id}&page=1&page_size=100")
                              or {}).get("items"), 60, what="upstream products")
    target = next((p for p in items if match(p)), None)
    if not target:
        raise SystemExit(f"no upstream product matched on connection {conn_id}: {[p.get('title') for p in items]}")
    local_id = next((m.get("local_product_id") for m in maps or []
                     if str(m.get("upstream_product_id")) == str(target["id"])), None)
    if not local_id:
        body = {"connection_id": conn_id, "upstream_product_id": target["id"]}
        body.update({"category_id": category_id} if category_id else {"auto_create_category": True})
        local_id = admin.post("/admin/product-mappings/import", body)["local_product_id"]
    product = admin.get(f"/admin/products/{local_id}")
    if not product.get("is_active"):
        patch = {"is_active": True}
        if category_id and not product.get("category_id"):
            patch["category_id"] = category_id
        admin.patch(f"/admin/products/{local_id}", patch)
        product = admin.get(f"/admin/products/{local_id}")
    return product


def product_ref(product):
    sku = (product.get("skus") or [{}])[0]
    return {"product_id": product["id"], "sku_id": sku.get("id"), "slug": product.get("slug"),
            "price": str(sku.get("price_amount") or product.get("price_amount"))}


# ================================================================= stages
def stage_wait(st):
    """Every public hostname answers through the edge (and, in proxy/caddy mode, has a cert)."""
    import urllib.request
    checks = {"store": "/api/v1/public/config", "zs2": "/api/v1/public/config", "acg": "/",
              "dujiao": "/api/v1/public/config", **{n: "/api/v1/public/config" for n in RESELLERS}}
    for name, path in checks.items():
        def up(name=name, path=path):
            try:
                with urllib.request.urlopen(url(name) + path, timeout=10) as r:
                    return r.status < 500
            except urllib.error.HTTPError as e:
                # reseller hosts answer 404 "site unavailable" until the subdomain is assigned
                return e.code == 404 and name in RESELLERS
        wait_for(up, 300, 3, f"{url(name)}{path}")
        log(f"  {url(name)} up")


def stage_store(st):
    log("store: admin, settings, catalogue")
    admin = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    zebra_basics(admin, "斑马小铺 Zebra Store · Lab", "store")
    admin.put("/admin/settings", {"key": "home_announcement", "value": {
        "enabled": True, "type": "info", "title": zh("zebra-lab 测试站"),
        "content": zh("<p>这是多站点联调测试环境：所有卡密均为<strong>测试数据</strong>，支付仅限余额。</p>")}})

    # the test catalogue from backend/scripts/seed_store.py (reuses our admin token)
    sys.path.insert(0, "/lab/seed")
    import seed_store  # noqa: E402
    seed_store.BASE = url("store") + "/api/v1"
    seed_store.COVERS = "/lab/seed/covers"
    real_call = seed_store.call
    seed_store.call = lambda m, p, b=None: {"token": admin.token} if p == "/admin/login" else real_call(m, p, b)
    seed_store.main()

    # dedicated e2e product with plenty of cards (resellers, zs2 and acg-faka buy this one)
    product = find_product(admin, LAB_PRODUCT)
    if not product:
        cats = admin.get("/admin/categories") or []
        cat = next((c for c in cats if c.get("slug") == "lab"), None) or admin.post(
            "/admin/categories", {"name": zh("联调测试", "Lab"), "slug": "lab", "sort_order": 1})
        created = admin.post("/admin/products", {
            "category_id": cat["id"], "slug": LAB_PRODUCT, "title": zh("联调测试卡 10 元", "Lab E2E Card"),
            "description": zh("zebra-lab 端到端测试专用，自动发货"), "content": zh("<p>测试卡密，非真实商品。</p>"),
            "images": [], "tags": ["测试", "自动发货"], "price_amount": float(LAB_PRICE), "purchase_type": "member",
            "fulfillment_type": "auto", "is_active": True, "sort_order": 1,
            "min_purchase_quantity": 1, "max_purchase_quantity": 5, "stock_display_mode": "exact"})
        product = admin.get(f"/admin/products/{created['id']}")
    ref = product_ref(product)
    stock = admin.page(f"/admin/card-secrets?product_id={ref['product_id']}&status=available&page=1&page_size=1")[1]
    if (stock or {}).get("total", 0) < 100:
        start = len(st.get("store_lab_batches", [])) * 1000 + 1
        admin.post("/admin/card-secrets/batch", {
            "product_id": ref["product_id"], "sku_id": ref["sku_id"], "batch_no": f"LAB-{start}",
            "note": "zebra-lab", "deduplicate": True,
            "secrets": [f"ZS-LAB-{i:05d} | 测试卡密" for i in range(start, start + 300)]})
        st.setdefault("store_lab_batches", []).append(start)
    st["store_lab_product"] = ref

    buyer = zebra_user("store", email("buyer"), USER_PASSWORD, st, "store_buyer")
    uid = user_id(buyer)
    wallet_top_up(admin, uid, 5000)
    st["store_buyer_id"] = uid
    save_state(st)


def stage_acg_install(st):
    log("acg: install")
    s = Acg("admin", "admin")
    env = s.request("GET", "/install/env")
    if '"install":true' in env.replace(" ", ""):
        out = s.post("/install/submit", {"use_builtin": 1, "email": E["ACG_ADMIN_EMAIL"],
                                          "nickname": "admin", "login_password": E["ACG_ADMIN_PASSWORD"]},
                     expect_ok=False)
        log(f"  install: {out.get('msg')}")
    else:
        log("  already installed")


def acg_admin():
    s = Acg("admin", "admin")
    out = s.post("/admin/api/authentication/login",
                 {"username": E["ACG_ADMIN_EMAIL"], "password": E["ACG_ADMIN_PASSWORD"]}, expect_ok=False)
    if out.get("code") != 200:
        raise SystemExit(f"acg admin login failed: {out}")
    s.request("GET", "/admin/dashboard/index?agree=1")
    return s


def acg_find(s, kind, name):
    out = s.post(f"/admin/api/{kind}/data", {"page": 1, "limit": 50, "equal-name": name})
    rows = (out.get("data") or {}).get("list") or []
    return rows[0] if rows else None


def acg_member(name):
    """Registers or logs in an acg-faka member; returns the session."""
    u = Acg("user", name)
    out = u.post("/user/api/authentication/register", {"username": name, "password": USER_PASSWORD}, expect_ok=False)
    if out.get("code") != 200:
        out = u.post("/user/api/authentication/login", {"username": name, "password": USER_PASSWORD}, expect_ok=False)
        if out.get("code") != 200:
            raise SystemExit(f"acg member {name}: {out}")
    return u


def acg_user_row(s, name):
    out = s.post("/admin/api/user/data", {"page": 1, "limit": 20, "equal-username": name})
    rows = (out.get("data") or {}).get("list") or []
    if not rows:
        raise SystemExit(f"acg user {name} not found")
    return rows[0]


def acg_recharge(s, name, want):
    row = acg_user_row(s, name)
    bal = float(row.get("balance") or 0)
    if bal < want:
        s.post("/admin/api/user/recharge", {"id": row["id"], "action": 1, "amount": f"{want - bal:.2f}",
                                              "log": "zebra-lab top-up", "total": 0})
    return acg_user_row(s, name)


def stage_acg(st):
    log("acg: settings, catalogue, supplier member")
    s = acg_admin()
    s.post("/admin/api/config/setting", {
        "logo": "/favicon.ico", "shop_name": "异次元 ACG Lab", "title": "异次元 ACG Lab", "description": "",
        "keywords": "", "notice": "zebra-lab 测试站，卡密均为测试数据", "closed_message": "维护中",
        "background_url": "/assets/admin/images/login/bg.jpg", "background_mobile_url": "",
        "user_theme": "Cartoon", "user_mobile_theme": "0", "user_center_theme": "Cartoon",
        "user_center_mobile_theme": "0", "username_len": 6, "session_expire": 0, "registered_type": 0,
        "forget_type": 0, "registered_state": 1})
    cat = acg_find(s, "category", ACG_CATEGORY)
    if not cat:
        s.post("/admin/api/category/save", {"name": ACG_CATEGORY, "status": 1, "hide": 0, "sort": 0})
        cat = acg_find(s, "category", ACG_CATEGORY)
    item = acg_find(s, "commodity", ACG_ITEM)
    if not item:
        s.post("/admin/api/commodity/save", {
            "category_id": cat["id"], "name": ACG_ITEM, "description": "<p>zebra-lab 测试点卡，自动发货</p>",
            "price": "10", "user_price": "9", "factory_price": "0", "status": 1, "api_status": 1,
            "delivery_way": 0, "delivery_auto_mode": 0, "contact_type": 0, "password_status": 0,
            "hide": 0, "sort": 0, "coupon": 0, "config": ""})
        item = acg_find(s, "commodity", ACG_ITEM)
    if int(item.get("card") or item.get("stock") or 0) < 20 and not st.get("acg_cards_loaded"):
        s.post("/admin/api/card/save", {"commodity_id": item["id"], "card_type": 0, "race_get_mode": 0, "unique": 1,
                                         "secret": "\n".join(f"ACG-LAB-{i:05d}" for i in range(1, CARDS + 1))})
        st["acg_cards_loaded"] = True
    st["acg_item"] = {"id": item["id"], "code": item.get("code")}

    acg_member("zebrasupply")
    row = acg_recharge(s, "zebrasupply", 5000)
    st["acg_supplier"] = {"app_id": str(row["id"]), "app_key": row["app_key"]}
    save_state(st)


def stage_dujiao(st):
    log("dujiao-next: settings, catalogue, supplier credential")
    admin = zebra_admin("dujiao", E["DUJIAO_ADMIN_PASSWORD"], st, "dujiao_admin")
    admin.post("/admin/compliance/acknowledge", COMPLIANCE)
    admin.put("/admin/settings", {"key": "registration_config", "value": {
        "registration_enabled": True, "email_verification_enabled": False}})
    cfg = admin.get("/admin/settings?key=site_config") or {}
    cfg["site_name"] = "Dujiao-Next Lab"
    admin.put("/admin/settings", {"key": "site_config", "value": cfg})

    product = None
    rows, _ = admin.page("/admin/products?page=1&page_size=100")
    for p in rows or []:
        if p.get("slug") == DJ_SLUG:
            product = p
    if not product:
        cats = admin.get("/admin/categories") or []
        cat = next((c for c in cats if c.get("slug") == "lab-cards"), None) or admin.post(
            "/admin/categories", {"slug": "lab-cards", "name": zh("独角测试卡", "Dujiao Lab"), "sort_order": 0})
        product = admin.post("/admin/products", {
            "category_id": cat["id"], "slug": DJ_SLUG, "title": zh("独角测试卡 12.5 元", "Dujiao Lab Card"),
            "description": zh("zebra-lab 测试卡密，自动发货"), "price_amount": 12.5,
            "fulfillment_type": "auto", "purchase_type": "member", "is_active": True})
    detail = admin.get(f"/admin/products/{product['id']}")
    sku_id = (detail.get("skus") or [{}])[0].get("id")
    if not st.get("dujiao_cards_loaded"):
        admin.post("/admin/card-secrets/batch", {"product_id": product["id"], "sku_id": sku_id, "deduplicate": True,
                                                 "secrets": [f"DJ-LAB-{i:05d}" for i in range(1, CARDS + 1)]})
        st["dujiao_cards_loaded"] = True

    supplier = zebra_user("dujiao", email("zebra-supply"), USER_PASSWORD, st, "dujiao_supplier")
    uid = user_id(supplier)
    cred = approve_credential(admin, supplier, email("zebra-supply"))
    if not st.get("dujiao_credential", {}).get("api_secret"):
        secret = supplier.post("/api-credential/regenerate")["api_secret"]
        st["dujiao_credential"] = {"api_key": supplier.get("/api-credential")["api_key"], "api_secret": secret}
    wallet_top_up(admin, uid, 5000)
    st["dujiao_product"] = {"id": product["id"], "sku_id": sku_id}
    _ = cred
    save_state(st)


def stage_connections(st):
    log("store: supplier connections to acg-faka and dujiao-next")
    admin = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    acg = st["acg_supplier"]
    c1 = ensure_connection(admin, "异次元 ACG Lab", "acg-faka", url("acg"), acg["app_id"], acg["app_key"], "20")
    dj = st["dujiao_credential"]
    # The compatibility service refuses private callback addresses; rehearsal relies on polling.
    c2 = ensure_connection(admin, "Dujiao-Next Lab", "dujiao-next", url("dujiao"), dj["api_key"], dj["api_secret"],
                           "20", callback="" if MODE == "rehearsal" else None)
    p1 = import_upstream(admin, c1["id"], lambda p: "ACG 测试点卡" in str(p.get("title")))
    p2 = import_upstream(admin, c2["id"], lambda p: "独角测试卡" in str(p.get("title")))
    st["store_conn"] = {"acg": c1["id"], "dujiao": c2["id"]}
    st["store_acg_product"] = product_ref(p1)
    st["store_dujiao_product"] = product_ref(p2)
    log(f"  imported acg -> #{p1['id']} ({st['store_acg_product']['price']}), "
        f"dujiao -> #{p2['id']} ({st['store_dujiao_product']['price']})")
    save_state(st)


def stage_zs2(st):
    log("zs2: settings, connection code from store, import")
    store = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    zs2 = zebra_admin("zs2", E["ZS2_ADMIN_PASSWORD"], st, "zs2_admin")
    zebra_basics(zs2, "ZS2 二号店 · 下游", "zs2")

    down = zebra_user("store", email("zs2-supply"), USER_PASSWORD, st, "store_zs2_supply")
    duid = user_id(down)
    approve_credential(store, down, email("zs2-supply"))
    wallet_top_up(store, duid, 5000)
    st["store_zs2_supply_id"] = duid

    if not connection_by_name(zs2, "斑马主站 Store"):
        code = down.post("/api-credential/connection-code", headers={"Origin": url("store")})["code"]
        parsed = zs2.post("/admin/site-connections/parse-code", {"code": code})
        ensure_connection(zs2, "斑马主站 Store", "zebra-store", parsed["base_url"], parsed["api_key"],
                          parsed["api_secret"], "10", callback=url("zs2") + "/api/v1/zs/events")
    conn = connection_by_name(zs2, "斑马主站 Store")
    cats = zs2.get("/admin/categories") or []
    cat = next((c for c in cats if c.get("slug") == "from-store"), None) or zs2.post(
        "/admin/categories", {"name": zh("主站货源", "From Store"), "slug": "from-store", "sort_order": 1})
    product = import_upstream(zs2, conn["id"], lambda p: "联调测试卡" in str(p.get("title")), category_id=cat["id"])
    st["zs2_conn"] = conn["id"]
    st["zs2_product"] = product_ref(product)
    log(f"  zs2 imported store product -> #{product['id']} price {st['zs2_product']['price']}")

    buyer = zebra_user("zs2", email("buyer"), USER_PASSWORD, st, "zs2_buyer")
    wallet_top_up(zs2, user_id(buyer), 5000)
    save_state(st)


def stage_resellers(st):
    log("store: reseller subsites")
    from logos import logo_png

    admin = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    lab = st["store_lab_product"]
    st.setdefault("resellers", {})
    for name, (default_markup, product_markup, look) in RESELLERS.items():
        mail = email(f"reseller-{name}")
        r = zebra_user("store", mail, USER_PASSWORD, st, f"reseller_{name}")
        prof = r.get("/reseller/profile") or {}
        profile = prof.get("profile") or {}
        if not profile or profile.get("status") in (None, "", "none"):
            r.post("/reseller/apply", {"reason": f"zebra-lab {name} 分站"})
            profile = (r.get("/reseller/profile") or {}).get("profile") or {}
        pid = profile["id"]
        if profile.get("status") == "pending_review":
            admin.post(f"/admin/resellers/profiles/{pid}/approve",
                       {"default_markup_percent": default_markup, "max_markup_percent": "50"})
        domains = admin.get(f"/admin/resellers/profiles/{pid}") or {}
        if host(name) not in str(domains):
            admin.put(f"/admin/resellers/profiles/{pid}/system-domain", {"subdomain": name})
        logo = r.upload("/reseller/upload", f"{name}.png",
                        logo_png(look["letter"], *look["colors"], look["motif"]))["url"]
        r.put("/reseller/site-config", {
            "site_name": look["title"], "logo": logo, "favicon": logo,
            "announcement": {"enabled": True, "type": look["type"], "title": zh(look["title"]),
                             "content": zh(f"<p>{look['announcement']}</p>")},
            "support": {"telegram": "", "whatsapp": "", "email": f"support-{name}@lab.test", "support_url": ""},
            "footer_links": [{"name": zh(f"{look['title']} 帮助"), "url": f"{url('store')}/"}]
            if url("store").startswith("https://") else [],
            "nav_config": {"builtin": {"blog": False}, "custom_items": []},
            "seo": {"title": zh(look["title"]), "keywords": zh(f"{name},zebra-lab"),
                    "description": zh(look["announcement"]), "default_og_image": ""}})
        r.put(f"/reseller/product-settings/{lab['product_id']}", {"settings": [
            {"sku_id": 0, "is_listed": True, "pricing_mode": "markup_percent", "markup_percent": product_markup}]})
        st["resellers"][name] = {"profile_id": pid, "user_id": user_id(r), "email": mail,
                                 "default_markup": default_markup, "product_markup": product_markup,
                                 "host": host(name), "site_name": look["title"]}
        log(f"  {name}: profile #{pid} -> {host(name)} markup {product_markup}% on #{lab['product_id']}")
    save_state(st)


def stage_acg_shared(st):
    log("acg-faka -> store through /shared/* (共享店铺)")
    store = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    down = zebra_user("store", email("acg-down"), USER_PASSWORD, st, "store_acg_down")
    duid = user_id(down)
    approve_credential(store, down, email("acg-down"))
    wallet_top_up(store, duid, 5000)
    compat = down.get("/api-credential/compat", ok_codes=(0, 404)) or {}
    if not compat.get("app_key"):
        compat = down.post("/api-credential/compat/issue")
    st["store_acg_down"] = {"user_id": duid, "app_id": compat["app_id"]}

    s = acg_admin()
    stores = (s.post("/admin/api/store/data", {"page": 1, "limit": 50}).get("data") or {}).get("list") or []
    store_url = url("store").rstrip("/")
    shop = next((x for x in stores if str(x.get("domain") or "").rstrip("/") == store_url), None)
    connection = {"type": 0, "domain": url("store"), "app_id": compat["app_id"],
                  "app_key": compat["app_key"]}
    if shop:
        connection["id"] = shop["id"]
    s.post("/admin/api/store/save", connection)
    stores = (s.post("/admin/api/store/data", {"page": 1, "limit": 50}).get("data") or {}).get("list") or []
    shop = next(x for x in stores if str(x.get("domain") or "").rstrip("/") == store_url)
    items = s.post("/admin/api/store/items", {"id": shop["id"]})["data"]
    code = next(c["code"] for cat in items for c in cat.get("children") or [] if "联调测试卡" in c.get("name", ""))
    cat = acg_find(s, "category", ACG_CATEGORY)
    s.post(f"/admin/api/store/addItem?storeId={shop['id']}", {
        "category_mode": 0, "category_id": cat["id"], "item_codes[]": [code], "premium": 1, "premium_type": 0,
        "shelves": 1, "shared_sync": 0, "shared_amount_sync": 0, "shared_config_sync": 0, "image_download": 0,
        "resume_import": 1}, expect_ok=False)
    imported = s.post("/admin/api/commodity/data", {"page": 1, "limit": 50, "equal-shared_id": shop["id"]})
    rows = (imported.get("data") or {}).get("list") or []
    item = next(r for r in rows if "联调测试卡" in r.get("name", ""))
    acg_member("acgbuyer")
    acg_recharge(s, "acgbuyer", 5000)
    st["acg_shared"] = {"store_id": shop["id"], "item_id": item["id"], "price": str(item.get("user_price"))}
    log(f"  acg shared store #{shop['id']} item #{item['id']}")
    save_state(st)


def write_credentials(st):
    def pub(name):  # browser URL (rehearsal: the edge's host port)
        port = f":{E.get('EDGE_HTTP_PORT', '18480')}" if MODE == "rehearsal" else ""
        return url(name) + port

    lines = [
        f"zebra-lab ({MODE}, *.{os.environ['LAB_DOMAIN']})",
        "",
        f"Main store      {pub('store')}/   admin {pub('store')}/admin/   admin / {E['STORE_ADMIN_PASSWORD']}",
        f"Downstream zs2  {pub('zs2')}/   admin {pub('zs2')}/admin/   admin / {E['ZS2_ADMIN_PASSWORD']}",
        f"acg-faka        {pub('acg')}/   admin {pub('acg')}/admin   {E['ACG_ADMIN_EMAIL']} / {E['ACG_ADMIN_PASSWORD']}",
        f"dujiao-next     {pub('dujiao')}/   admin {pub('dujiao')}/admin/   admin / {E['DUJIAO_ADMIN_PASSWORD']}",
        "",
        f"Test users (password {USER_PASSWORD}):",
        f"  store/zs2 buyer            {email('buyer')}            (wallet topped up, pay with balance)",
        f"  store -> zs2 supply acct   {email('zs2-supply')}",
        f"  store -> acg downstream    {email('acg-down')}",
        "  acg members               zebrasupply (store's upstream account), acgbuyer",
        f"  dujiao supplier           {email('zebra-supply')}",
    ]
    for name, r in st.get("resellers", {}).items():
        lines.append(f"  reseller {name:7}          {r['email']:28} -> {pub(name)}/  ({r['site_name']})")
    path = os.path.join(STATE_DIR, "credentials.txt")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    os.chmod(path, 0o600)
    print("\n".join(lines))


STAGES = {
    "wait": stage_wait, "acg-install": stage_acg_install, "store": stage_store, "acg": stage_acg, "dujiao": stage_dujiao,
    "connections": stage_connections, "zs2": stage_zs2, "resellers": stage_resellers,
    "acg-shared": stage_acg_shared,
}


def main():
    wanted = sys.argv[1:] or [s for s in STAGES if s not in ("wait", "acg-install")]
    st = load_state()
    for name in wanted:
        if name == "credentials":
            continue
        STAGES[name](st)
    write_credentials(st)


if __name__ == "__main__":
    try:
        main()
    except ApiError as e:
        raise SystemExit(f"FAILED: {e} data={e.data}")
