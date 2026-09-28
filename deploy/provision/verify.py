#!/usr/bin/env python3
"""End-to-end verification of the provisioned zebra-lab topology.

Places real orders (paid from wallets / balances) and asserts the whole chain:
  a  store order of an acg-faka product    -> procured from acg-faka  -> card delivered
  b  store order of a dujiao-next product  -> procured from dujiao    -> card delivered
  c  zs2 order of a store product          -> store order paid from zs2's store wallet -> card at zs2
  d  order on each reseller subsite        -> reseller price + profit snapshot + ledger
                                              -> settles to available -> withdraw -> admin pays
  e  acg-faka buys from store via /shared/* (共享店铺) -> card delivered at acg-faka
Prints a PASS/FAIL table; exit code 1 when anything failed.
"""
import json
import os
import sys
import time
import traceback
from decimal import ROUND_HALF_UP, Decimal

sys.path.insert(0, os.path.dirname(__file__))
from lab import Api, ApiError, RESELLERS, USER_PASSWORD, load_state, log, url, wait_for, zebra_admin, zebra_user  # noqa: E402
from provision import acg_admin, acg_member, email  # noqa: E402

E = os.environ
RESULTS = []


def money(v):
    return Decimal(str(v)).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)


def acg_config(raw):
    """Accept both JSON and acg-faka's legacy INI-style commodity config."""
    if isinstance(raw, dict):
        return raw
    if not isinstance(raw, str) or not raw.strip():
        return {}
    try:
        parsed = json.loads(raw)
        return parsed if isinstance(parsed, dict) else {}
    except json.JSONDecodeError:
        parsed = {}
        section = None
        for line in raw.replace("\\n", "\n").splitlines():
            line = line.strip()
            if line.startswith("[") and line.endswith("]"):
                section = line[1:-1]
                parsed.setdefault(section, {})
            elif section and "=" in line:
                key, value = line.split("=", 1)
                parsed[section][key.strip()] = value.strip()
        return parsed


def check(name, fn):
    selected = E.get("VERIFY_ONLY", "").strip()
    if selected and not name.startswith(selected):
        return
    t0 = time.monotonic()
    try:
        detail = fn()
        RESULTS.append((name, "PASS", detail, time.monotonic() - t0))
    except Exception as e:  # noqa: BLE001 — every failure becomes a FAIL row
        traceback.print_exc()
        RESULTS.append((name, "FAIL", f"{type(e).__name__}: {e}"[:160], time.monotonic() - t0))


def order_and_wait(api, ref, timeout, qty=1):
    """Creates + pays an order from the wallet; waits for the delivered card text."""
    out = api.post("/orders/create-and-pay", {
        "items": [{"product_id": ref["product_id"], "sku_id": ref["sku_id"], "quantity": qty}],
        "channel_id": 0, "use_balance": True})
    no = out["order_no"]
    if not out.get("order_paid"):
        raise AssertionError(f"order {no} not paid from wallet: {out}")

    def delivered():
        order = api.get(f"/orders/{no}")
        payloads = [((c.get("fulfillment") or {}).get("payload") or "") for c in order.get("children") or []]
        if not order.get("children"):
            payloads.append(((order.get("fulfillment") or {}).get("payload")) or "")
        text = "\n".join(p for p in payloads if p)
        return (order, text) if text else None

    order, text = wait_for(delivered, timeout, 2, f"delivery of {no}")
    return no, order, text


def wallet_balance(admin, uid):
    w = admin.get(f"/admin/users/{uid}/wallet") or {}
    return money((w.get("account") or w).get("balance") or 0)


def procurement(admin, order_no):
    """Procurement order of (a child of) local order `order_no`."""
    rows, _ = admin.page("/admin/procurement-orders?page=1&page_size=50")
    return next((r for r in rows or [] if str(r.get("local_order_no", "")).startswith(order_no)), None)


def main():
    st = load_state()
    if not st.get("resellers"):
        raise SystemExit("not provisioned — run ./lab.sh provision first")
    store = zebra_admin("store", E["STORE_ADMIN_PASSWORD"], st, "store_admin")
    zs2 = zebra_admin("zs2", E["ZS2_ADMIN_PASSWORD"], st, "zs2_admin")
    buyer = zebra_user("store", email("buyer"), USER_PASSWORD, st, "store_buyer")

    def upstream_case(ref_key, prefix, timeout):
        def run():
            no, order, text = order_and_wait(buyer, st[ref_key], timeout)
            assert text.startswith(prefix), f"card {text!r} does not come from {prefix}"
            proc = procurement(store, no)
            status = (proc or {}).get("status")
            assert status in ("fulfilled", "completed"), f"procurement status {status}"
            return (f"order {no} -> {text.splitlines()[0]} (procurement {status}, "
                    f"upstream {proc.get('upstream_order_no')}, cost {proc.get('upstream_amount')})")
        return run

    check("a  store <- acg-faka (acg-faka protocol)", upstream_case("store_acg_product", "ACG-LAB-", 120))
    # This supplier delivers asynchronously; the store polls it after the initial delay.
    check("b  store <- dujiao-next (dujiao-next protocol)", upstream_case("store_dujiao_product", "DJ-LAB-", 400))

    def zs2_case():
        before = wallet_balance(store, st["store_zs2_supply_id"])
        zs2_buyer = zebra_user("zs2", email("buyer"), USER_PASSWORD, st, "zs2_buyer")
        no, order, text = order_and_wait(zs2_buyer, st["zs2_product"], 400)
        assert text.startswith("ZS-LAB-"), f"card {text!r} is not a store card"
        proc = procurement(zs2, no)
        assert (proc or {}).get("status") in ("fulfilled", "completed"), f"zs2 procurement {proc}"
        after = wallet_balance(store, st["store_zs2_supply_id"])
        spent = before - after
        assert spent == money(st["store_lab_product"]["price"]), f"zs2 wallet on store spent {spent}"
        return f"zs2 order {no} -> {text.splitlines()[0]}; store wallet of zs2 -{spent}"

    check("c  zs2 <- store (zebra-store protocol)", zs2_case)

    base = money(st["store_lab_product"]["price"])
    for name, r in st["resellers"].items():
        def reseller_case(name=name, r=r):
            site = Api(url(name), token=buyer.token)
            cfg = site.get("/public/config")
            shown = str(cfg)
            assert r["site_name"] in shown, f"{name}: public config does not show {r['site_name']}"
            expected = money(base * (100 + Decimal(r["product_markup"])) / 100)
            profit = expected - base
            no, order, text = order_and_wait(site, st["store_lab_product"], 120)
            total = money(order.get("total_amount"))
            assert total == expected, f"{name}: paid {total}, expected {expected}"
            rs = zebra_user("store", r["email"], USER_PASSWORD, st, f"reseller_{name}")
            ro = rs.get(f"/reseller/orders/{no}")
            assert money(ro.get("profit_amount")) == profit, f"{name}: snapshot profit {ro.get('profit_amount')}"

            def entry_available():
                rows, _ = rs.page("/reseller/ledger-entries?page=1&page_size=50")
                # ledger rows carry the order id; the buyer's order detail has it too, the
                # reseller view only the paid_at the profit was posted at
                hit = [e for e in rows or [] if (order.get("id") and e.get("order_id") == order.get("id"))
                       or (not order.get("id") and e.get("created_at") == ro.get("paid_at"))]
                return hit[0] if hit and hit[0].get("status") == "available" else None

            entry = wait_for(entry_available, 150, 5, f"{name} ledger entry available")
            assert money(entry["amount"]) == profit, f"{name}: ledger amount {entry['amount']}"
            accounts = rs.get("/reseller/balance-accounts") or []
            acct = next(a for a in accounts if a.get("currency") == "CNY")
            available = money(acct["available_amount"])
            assert available >= profit, f"{name}: available {available}"
            wd = rs.post("/reseller/withdraws", {"amount": str(available), "currency": "CNY",
                                                 "channel": "alipay", "account": f"{name}@alipay.lab"})
            paid = store.post(f"/admin/resellers/withdraws/{wd['id']}/pay")
            assert (paid or {}).get("status") == "paid", f"{name}: withdraw {paid}"
            return (f"{r['site_name']}: paid {total} (base {base} +{r['product_markup']}%), profit {profit}, "
                    f"withdrew {available} -> paid")
        check(f"d  reseller {name}.{E['LAB_DOMAIN']}", reseller_case)

    def acg_shared_case():
        shared = st["acg_shared"]
        before = wallet_balance(store, st["store_acg_down"]["user_id"])
        admin = acg_admin()
        listed = admin.post("/admin/api/commodity/data", {
            "page": 1, "limit": 10, "equal-id": shared["item_id"]})
        rows = (listed.get("data") or {}).get("list") or []
        item = next((row for row in rows if int(row.get("id") or 0) == int(shared["item_id"])), None)
        assert item, f"acg shared item {shared['item_id']} not found"
        config = acg_config(item.get("config"))
        categories = config.get("category") or {}
        skus = config.get("sku") or {}
        payload = {
            "item_id": shared["item_id"], "num": 1, "pay_id": 1, "card_id": 0, "device": 0,
            "contact": "acgbuyer", "password": "", "coupon": "",
            "race": next(iter(categories), ""), "sku[]": "", "request_no": f"zl{time.time_ns():x}",
        }
        for name, options in skus.items():
            if isinstance(options, dict) and options:
                payload[f"sku[{name}]"] = next(iter(options))
        member = acg_member("acgbuyer")
        out = member.post("/user/api/order/trade", payload)
        secret = (out.get("data") or {}).get("secret") or ""
        assert "ZS-LAB-" in secret, f"acg trade secret {secret!r}"
        after = wallet_balance(store, st["store_acg_down"]["user_id"])
        spent = before - after
        assert spent == money(st["store_lab_product"]["price"]), f"store wallet of acg-down spent {spent}"
        return f"acg order {out['data'].get('tradeNo')} -> {secret.splitlines()[0]}; store wallet -{spent}"

    check("e  acg-faka <- store (/shared/* 共享店铺)", acg_shared_case)


def table():
    width = max(len(r[0]) for r in RESULTS)
    print("\n" + "=" * 100)
    print(f"{'case':{width}}  result  time   detail")
    print("-" * 100)
    for name, res, detail, secs in RESULTS:
        print(f"{name:{width}}  {res:6}  {secs:4.0f}s  {detail}")
    print("=" * 100)
    failed = [r for r in RESULTS if r[1] != "PASS"]
    print(f"{len(RESULTS) - len(failed)}/{len(RESULTS)} PASS")
    return not failed


if __name__ == "__main__":
    try:
        main()
    except ApiError as e:
        RESULTS.append(("setup", "FAIL", str(e), 0))
    ok = table()
    log("verify finished")
    sys.exit(0 if ok else 1)
