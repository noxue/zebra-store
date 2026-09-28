# Zebra Store 对接协议 v1 — 请求/响应示例

> 由 `backend/crates/api/tests/integration_zebra_store.rs` 端到端测试捕获
> （`ZS_WRITE_EXAMPLES=1 cargo test -p zs-api --test integration_zebra_store`）。
> 两个 Zebra Store 实例在同一测试进程中运行：一个为供货方，一个为采购方。
> 签名头（`ZS-Key` / `ZS-Timestamp` / `ZS-Nonce` / `ZS-Signature`）每次请求都会携带，示例中省略；密钥与卡密已替换为占位符。

## 解析连接码（采购方管理端）

```http
POST /api/v1/admin/site-connections/parse-code

{"code": "zsc1_…"}
```

```json
{
  "status_code": 0,
  "msg": "success",
  "data": {
    "name": "",
    "base_url": "http://127.0.0.1:61817",
    "api_key": "bb9d6d0925d7c7fa737b2fddd6af3cc7e02b8248079a01be62b5d2515c365072",
    "api_secret": "<secret>",
    "protocol": "zebra-store"
  }
}
```

## 握手预检（采购方管理端）

```http
POST /api/v1/admin/site-connections/handshake

{"base_url": "http://127.0.0.1:…", "api_key": "…", "api_secret": "…", "protocol": "zebra-store"}
```

```json
{
  "status_code": 0,
  "msg": "success",
  "data": {
    "ok": true,
    "protocol": "zebra-store",
    "version": "1.0",
    "site": {
      "name": "",
      "url": "http://127.0.0.1:61817",
      "currency": "CNY"
    },
    "features": [
      "categories",
      "incremental_changes",
      "push_events",
      "quote",
      "multi_item",
      "idempotency",
      "encrypted_delivery"
    ],
    "capabilities": [
      "categories",
      "incremental_changes",
      "push_events",
      "quote",
      "multi_item",
      "idempotency",
      "encrypted_delivery"
    ],
    "limits": {
      "requests_per_minute": 120,
      "max_items_per_order": 20,
      "quote_ttl_seconds": 600,
      "changes_retention_days": 7
    },
    "account": {
      "balance": "100.00",
      "currency": "CNY"
    },
    "suggested_exchange_rate": "1",
    "suggested_callback_url": "http://localhost/api/v1/zs/events"
  }
}
```

## 握手 §4

```http
GET /api/v1/zs/handshake
```

```json
{
  "ok": true,
  "data": {
    "protocol": "zebra-store",
    "version": "1.0",
    "site": {
      "name": "",
      "url": "http://localhost",
      "currency": "CNY"
    },
    "features": [
      "changes",
      "webhooks",
      "quote",
      "multi_item",
      "idempotency",
      "encrypted_delivery"
    ],
    "limits": {
      "requests_per_minute": 120,
      "max_items_per_order": 20,
      "quote_ttl_seconds": 600,
      "changes_retention_days": 7
    },
    "account": {
      "user_id": 1,
      "balance": "100.00",
      "currency": "CNY",
      "member_level": null
    },
    "catalog": {
      "latest_seq": 0
    },
    "server_time": "2026-09-25T08:30:55Z"
  }
}
```

## 变更流 §5（首次快照）

```http
GET /api/v1/zs/catalog/changes?limit=2
```

```json
{
  "ok": true,
  "data": {
    "changes": [
      {
        "seq": 1,
        "type": "product.upserted",
        "product_id": 1,
        "sku_id": null,
        "at": "2026-09-25T08:30:55.013411Z",
        "data": {
          "product": {
            "id": 1,
            "slug": "p1",
            "seo_meta": {},
            "title": {
              "zh-CN": "商品 p1"
            },
            "description": {},
            "content": {},
            "images": [],
            "tags": [],
            "price_amount": "10.00",
            "fulfillment_type": "auto",
            "manual_form_schema": {},
            "is_active": true,
            "category_id": 1,
            "skus": [
              {
                "id": 1,
                "sku_code": "A",
                "spec_values": {},
                "price_amount": "5.00",
                "stock_status": "low_stock",
                "stock_quantity": 3,
                "is_active": true
              }
            ],
            "created_at": "2026-09-25T08:30:54.937490Z",
            "updated_at": "2026-09-25T08:30:54.937490Z"
          }
        }
      },
      {
        "seq": 2,
        "type": "product.upserted",
        "product_id": 2,
        "sku_id": null,
        "at": "2026-09-25T08:30:55.013411Z",
        "data": {
          "product": {
            "id": 2,
            "slug": "p2",
            "seo_meta": {},
            "title": {
              "zh-CN": "商品 p2"
            },
            "description": {},
            "content": {},
            "images": [],
            "tags": [],
            "price_amount": "10.00",
            "fulfillment_type": "manual",
            "manual_form_schema": {},
            "is_active": true,
            "category_id": 2,
            "skus": [
              {
                "id": 2,
                "sku_code": "M",
                "spec_values": {},
                "price_amount": "8.00",
                "stock_status": "in_stock",
                "stock_quantity": 30,
                "is_active": true
              }
            ],
            "created_at": "2026-09-25T08:30:54.941579Z",
            "updated_at": "2026-09-25T08:30:54.941579Z"
          }
        }
      }
    ],
    "next_cursor": "2",
    "has_more": true
  }
}
```

## 商品游标列表 §5

```http
GET /api/v1/zs/catalog/products?limit=1
```

```json
{
  "ok": true,
  "data": {
    "items": [
      {
        "id": 1,
        "slug": "p1",
        "seo_meta": {},
        "title": {
          "zh-CN": "商品 p1"
        },
        "description": {},
        "content": {},
        "images": [],
        "tags": [],
        "price_amount": "10.00",
        "fulfillment_type": "auto",
        "manual_form_schema": {},
        "is_active": true,
        "category_id": 1,
        "skus": [
          {
            "id": 1,
            "sku_code": "A",
            "spec_values": {},
            "price_amount": "5.00",
            "stock_status": "low_stock",
            "stock_quantity": 3,
            "is_active": true
          }
        ],
        "created_at": "2026-09-25T08:30:54.937490Z",
        "updated_at": "2026-09-25T08:30:54.937490Z",
        "version": "23a7d73465e0aab2"
      }
    ],
    "next_cursor": "1",
    "has_more": true,
    "total": 3
  }
}
```

## 事件推送 §6：catalog.changed（供货方 → 采购方 POST /api/v1/zs/events，带 ZS-Event-Id）

```http
POST {buyer}/api/v1/zs/events
```

```json
{
  "id": "evt_e46b562313f24adf8b94d847c7ebd386",
  "type": "catalog.changed",
  "created_at": "2026-09-25T08:30:55Z",
  "data": {
    "latest_seq": 3
  }
}
```

## 变更流 §5（改价 / 补货 / 删除）

```http
GET /api/v1/zs/catalog/changes?since=…
```

```json
{
  "ok": true,
  "data": {
    "changes": [
      {
        "seq": 4,
        "type": "sku.price",
        "product_id": 1,
        "sku_id": 1,
        "at": "2026-09-25T08:30:55.038852Z",
        "data": {
          "price_amount": "6.00"
        }
      },
      {
        "seq": 5,
        "type": "sku.stock",
        "product_id": 1,
        "sku_id": 1,
        "at": "2026-09-25T08:30:55.038852Z",
        "data": {
          "stock_quantity": 5,
          "stock_status": "low_stock"
        }
      },
      {
        "seq": 6,
        "type": "product.deleted",
        "product_id": 2,
        "sku_id": null,
        "at": "2026-09-25T08:30:55.038852Z",
        "data": {}
      }
    ],
    "next_cursor": "6",
    "has_more": false
  }
}
```

## 事件推送 §6/§7：order.delivered（交付已加密）

```http
POST {buyer}/api/v1/zs/events
```

```json
{
  "id": "evt_6b590dd77ba54409a78e4ba0632c8f85",
  "type": "order.delivered",
  "created_at": "2026-09-25T08:30:55Z",
  "data": {
    "order_no": "DJ20260925163055939422",
    "downstream_order_no": "DJ20260925163055664616-01",
    "status": "completed",
    "currency": "CNY",
    "total": "6.00",
    "items": [
      {
        "sku_id": 1,
        "product_id": 1,
        "quantity": 1,
        "unit_price": "6.00",
        "subtotal": "6.00",
        "status": "completed",
        "delivery": {
          "encrypted": true,
          "alg": "A256GCM",
          "nonce": "<base64>",
          "ciphertext": "<base64>"
        }
      }
    ],
    "created_at": "2026-09-25T08:30:55Z"
  }
}
```

## 报价 §7

```http
POST /api/v1/zs/orders/quote

{"items":[{"sku_id":1,"quantity":1}]}
```

```json
{
  "ok": true,
  "data": {
    "quote_id": "q_ae16eb594fae49a2b9fda092ccce7412",
    "expires_at": "2026-09-25T08:40:55Z",
    "currency": "CNY",
    "items": [
      {
        "sku_id": 1,
        "quantity": 1,
        "unit_price": "6.00",
        "subtotal": "6.00",
        "available": true,
        "reason": null
      }
    ],
    "total": "6.00",
    "balance": "94.00",
    "sufficient_balance": true
  }
}
```

## 下单 §7（Idempotency-Key: buyer-order-1）

```http
POST /api/v1/zs/orders
Idempotency-Key: buyer-order-1

{"quote_id":"q_ae16eb594fae49a2b9fda092ccce7412","items":[{"sku_id":1,"quantity":1}],"downstream_order_no":"B-1001","trace_id":"t-1"}
```

```json
{
  "ok": true,
  "data": {
    "order_no": "DJ20260925163055457646",
    "downstream_order_no": "B-1001",
    "status": "paid",
    "currency": "CNY",
    "total": "6.00",
    "items": [
      {
        "sku_id": 1,
        "product_id": 1,
        "quantity": 1,
        "unit_price": "6.00",
        "subtotal": "6.00",
        "status": "paid",
        "delivery": null
      }
    ],
    "created_at": "2026-09-25T08:30:55Z"
  }
}
```

## 错误 §8：422 idempotency_conflict

```http
POST /api/v1/zs/orders
Idempotency-Key: buyer-order-1

（请求体与首次不同）
```

```json
{
  "ok": false,
  "error": {
    "code": "idempotency_conflict",
    "message": "Idempotency-Key was used with a different request body",
    "retryable": false,
    "request_id": "<request-id>"
  }
}
```

## 错误 §8：402 insufficient_balance

```http
POST /api/v1/zs/orders
Idempotency-Key: buyer-order-2

{"items": [{"sku_id": …, "quantity": 1}], "downstream_order_no": "B-1003"}
```

```json
{
  "ok": false,
  "error": {
    "code": "insufficient_balance",
    "message": "wallet balance is insufficient",
    "retryable": false,
    "request_id": "<request-id>"
  }
}
```

## 错误 §8：409 quote_expired

```http
POST /api/v1/zs/orders
Idempotency-Key: buyer-order-3

{"quote_id": "q_…", …}
```

```json
{
  "ok": false,
  "error": {
    "code": "quote_expired",
    "message": "quote has expired",
    "retryable": false,
    "request_id": "<request-id>"
  }
}
```

## 错误 §8：401 unauthorized（nonce 重放）

```http
GET /api/v1/zs/handshake
ZS-Nonce: fixed-nonce-0123456789（第二次）
```

```json
{
  "ok": false,
  "error": {
    "code": "unauthorized",
    "message": "unauthorized",
    "retryable": false,
    "request_id": "<request-id>"
  }
}
```

## 错误 §8：410 cursor_expired

```http
GET /api/v1/zs/catalog/changes?since=1
```

```json
{
  "ok": false,
  "error": {
    "code": "cursor_expired",
    "message": "cursor is older than the retention window; resync with /catalog/products",
    "retryable": false,
    "request_id": "<request-id>"
  }
}
```
