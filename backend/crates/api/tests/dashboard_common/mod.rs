//! Seeding helpers of the dashboard integration tests (rows inserted directly
//! through the sea-orm entities).

#![allow(
    dead_code,
    reason = "shared by several dashboard test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use chrono::{DateTime, Utc};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{
    card_secrets, coupon_usages, coupons, order_items, order_refund_records, orders,
    payment_channels, payments, product_mappings, product_skus, products, sku_mappings,
    user_oauth_identities, users, wallet_accounts,
};

pub fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

pub fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// Inserts a live order and returns its id.
pub async fn order(
    db: &DatabaseConnection,
    no: &str,
    parent: Option<i64>,
    status: &str,
    total: &str,
    created: &str,
) -> i64 {
    orders::ActiveModel {
        order_no: Set(no.into()),
        parent_id: Set(parent),
        user_id: Set(1),
        status: Set(status.into()),
        currency: Set("cny".into()),
        total_amount: Set(dec(total)),
        created_at: Set(at(created)),
        updated_at: Set(at(created)),
        guest_email: Set(String::new()),
        guest_password: Set(String::new()),
        guest_locale: Set(String::new()),
        original_amount: Set(Decimal::ZERO),
        discount_amount: Set(Decimal::ZERO),
        member_discount_amount: Set(Decimal::ZERO),
        promotion_discount_amount: Set(Decimal::ZERO),
        wholesale_discount_amount: Set(Decimal::ZERO),
        wallet_paid_amount: Set(Decimal::ZERO),
        online_paid_amount: Set(Decimal::ZERO),
        refunded_amount: Set(Decimal::ZERO),
        affiliate_code: Set(String::new()),
        reseller_domain: Set(String::new()),
        reseller_profit_amount: Set(Decimal::ZERO),
        client_ip: Set(String::new()),
        risk_ip: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// Inserts an order item.
#[expect(
    clippy::too_many_arguments,
    reason = "test fixture mirrors the columns"
)]
pub async fn item(
    db: &DatabaseConnection,
    order_id: i64,
    product_id: i64,
    sku_id: i64,
    title: &str,
    qty: i32,
    total: &str,
    coupon: &str,
    cost: &str,
) {
    order_items::ActiveModel {
        order_id: Set(order_id),
        product_id: Set(product_id),
        sku_id: Set(sku_id),
        title_json: Set(Some(json!({"zh-CN": title}))),
        quantity: Set(qty),
        unit_price: Set(dec(total) / Decimal::from(qty)),
        total_price: Set(dec(total)),
        coupon_discount: Set(dec(coupon)),
        cost_price: Set(dec(cost)),
        fulfillment_type: Set("auto".into()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        original_unit_price: Set(Decimal::ZERO),
        original_total_price: Set(Decimal::ZERO),
        member_discount: Set(Decimal::ZERO),
        promotion_discount: Set(Decimal::ZERO),
        wholesale_discount: Set(Decimal::ZERO),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

/// Inserts a payment channel and returns its id.
pub async fn channel(db: &DatabaseConnection, name: &str) -> i64 {
    payment_channels::ActiveModel {
        name: Set(name.into()),
        provider_type: Set("epay".into()),
        channel_type: Set("alipay".into()),
        interaction_mode: Set("redirect".into()),
        is_active: Set(true),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        icon: Set(String::new()),
        fee_rate: Set(Decimal::ZERO),
        fixed_fee: Set(Decimal::ZERO),
        min_amount: Set(Decimal::ZERO),
        max_amount: Set(Decimal::ZERO),
        hide_amount_out_range: Set(false),
        sort_order: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// Inserts a payment.
#[expect(
    clippy::too_many_arguments,
    reason = "test fixture mirrors the columns"
)]
pub async fn payment(
    db: &DatabaseConnection,
    order_id: i64,
    channel_id: i64,
    provider: &str,
    status: &str,
    amount: &str,
    fee: &str,
    fee_policy: &str,
    created: &str,
) {
    payments::ActiveModel {
        order_id: Set(order_id),
        channel_id: Set(channel_id),
        provider_type: Set(provider.into()),
        channel_type: Set(if provider == "wallet" {
            "balance"
        } else {
            "alipay"
        }
        .into()),
        interaction_mode: Set("redirect".into()),
        amount: Set(dec(amount)),
        fee_amount: Set(dec(fee)),
        fee_policy: Set(fee_policy.into()),
        currency: Set("CNY".into()),
        status: Set(status.into()),
        created_at: Set(at(created)),
        updated_at: Set(at(created)),
        fee_rate: Set(Decimal::ZERO),
        fixed_fee: Set(Decimal::ZERO),
        exception_code: Set(String::new()),
        provider_ref: Set(String::new()),
        gateway_order_no: Set(String::new()),
        pay_url: Set(String::new()),
        qr_code: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

/// Inserts a refund record.
pub async fn refund(
    db: &DatabaseConnection,
    order_id: i64,
    amount: &str,
    fee: &str,
    created: &str,
) {
    order_refund_records::ActiveModel {
        order_id: Set(order_id),
        type_: Set("wallet".into()),
        amount: Set(dec(amount)),
        payment_fee_refunded: Set(dec(fee) > Decimal::ZERO),
        payment_fee_refunded_amount: Set(dec(fee)),
        currency: Set("CNY".into()),
        created_at: Set(at(created)),
        updated_at: Set(at(created)),
        user_id: Set(0),
        guest_email: Set(String::new()),
        remark: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

/// Inserts an active product and returns its id.
pub async fn product(
    db: &DatabaseConnection,
    slug: &str,
    fulfillment: &str,
    manual_stock: i32,
) -> i64 {
    products::ActiveModel {
        slug: Set(slug.into()),
        title_json: Set(Some(json!({"zh-CN": slug}))),
        fulfillment_type: Set(fulfillment.into()),
        manual_stock_total: Set(manual_stock),
        payment_channel_ids: Set(String::new()),
        is_active: Set(true),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        category_id: Set(0),
        price_amount: Set(Decimal::ZERO),
        cost_price_amount: Set(Decimal::ZERO),
        purchase_type: Set(String::new()),
        min_purchase_quantity: Set(0),
        max_purchase_quantity: Set(0),
        stock_display_mode: Set(String::new()),
        manual_stock_locked: Set(0),
        manual_stock_sold: Set(0),
        is_affiliate_enabled: Set(false),
        is_mapped: Set(false),
        sort_order: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// Inserts a SKU and returns its id.
pub async fn sku(
    db: &DatabaseConnection,
    product_id: i64,
    code: &str,
    stock: i32,
    active: bool,
) -> i64 {
    product_skus::ActiveModel {
        product_id: Set(product_id),
        sku_code: Set(code.into()),
        spec_values_json: Set(Some(json!({"size": code}))),
        manual_stock_total: Set(stock),
        is_active: Set(active),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        price_amount: Set(Decimal::ZERO),
        cost_price_amount: Set(Decimal::ZERO),
        manual_stock_locked: Set(0),
        manual_stock_sold: Set(0),
        sort_order: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

/// Inserts `n` card secrets with `status`.
pub async fn secrets(
    db: &DatabaseConnection,
    product_id: i64,
    sku_id: i64,
    n: usize,
    status: &str,
) {
    for i in 0..n {
        card_secrets::ActiveModel {
            product_id: Set(product_id),
            sku_id: Set(sku_id),
            secret: Set(format!("S-{product_id}-{sku_id}-{status}-{i}")),
            status: Set(status.into()),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }
}

/// Maps an upstream product with SKU mappings `(local_sku_id, stock, active)`.
pub async fn upstream_mapping(db: &DatabaseConnection, product_id: i64, skus: &[(i64, i32, bool)]) {
    let mapping = product_mappings::ActiveModel {
        connection_id: Set(1),
        local_product_id: Set(product_id),
        upstream_product_id: Set(product_id + 1000),
        is_active: Set(true),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        upstream_fulfillment_type: Set(String::new()),
        upstream_status: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    for (local, stock, active) in skus {
        sku_mappings::ActiveModel {
            product_mapping_id: Set(mapping.id),
            local_sku_id: Set(*local),
            upstream_sku_id: Set(*local + 1000),
            upstream_stock: Set(*stock),
            upstream_is_active: Set(*active),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
            upstream_price: Set(Decimal::ZERO),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }
}

/// Inserts a verified, active user with a bcrypt password; returns its id.
pub async fn user(db: &DatabaseConnection, email: &str, password: &str, created: &str) -> i64 {
    users::ActiveModel {
        email: Set(email.into()),
        password_hash: Set(bcrypt::hash(password, 4).unwrap()),
        display_name: Set(email.split('@').next().unwrap_or_default().into()),
        locale: Set("zh-CN".into()),
        status: Set("active".into()),
        email_verified_at: Set(Some(at(created))),
        created_at: Set(at(created)),
        updated_at: Set(at(created)),
        password_setup_required: Set(false),
        member_level_id: Set(0),
        total_recharged: Set(Decimal::ZERO),
        total_spent: Set(Decimal::ZERO),
        admin_note: Set(String::new()),
        token_version: Set(0),
        totp_secret: Set(String::new()),
        totp_pending_secret: Set(String::new()),
        recovery_codes: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

pub async fn wallet(db: &DatabaseConnection, user_id: i64, balance: &str) {
    wallet_accounts::ActiveModel {
        user_id: Set(user_id),
        balance: Set(dec(balance)),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

pub async fn oauth(db: &DatabaseConnection, user_id: i64, provider: &str, username: &str) -> i64 {
    user_oauth_identities::ActiveModel {
        user_id: Set(user_id),
        provider: Set(provider.into()),
        provider_user_id: Set(format!("{provider}-{user_id}")),
        username: Set(username.into()),
        avatar_url: Set(String::new()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

pub async fn coupon(db: &DatabaseConnection, code: &str, kind: &str, scope: &str) -> i64 {
    coupons::ActiveModel {
        code: Set(code.into()),
        type_: Set(kind.into()),
        value: Set(dec("5")),
        scope_type: Set("product".into()),
        scope_ref_ids: Set(scope.into()),
        is_active: Set(true),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        min_amount: Set(Decimal::ZERO),
        max_discount: Set(Decimal::ZERO),
        usage_limit: Set(0),
        used_count: Set(0),
        per_user_limit: Set(0),
        disabled_wholesale_price: Set(false),
        per_item_discount: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
    .id
}

pub async fn coupon_usage(
    db: &DatabaseConnection,
    coupon_id: i64,
    user_id: i64,
    order_id: i64,
    discount: &str,
) {
    coupon_usages::ActiveModel {
        coupon_id: Set(coupon_id),
        user_id: Set(user_id),
        order_id: Set(order_id),
        discount_amount: Set(dec(discount)),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

/// Stores a settings key directly.
pub async fn setting(db: &DatabaseConnection, key: &str, value: Value) {
    use zs_domain::settings::SettingsStore;
    zs_infra::db::repo::settings::SeaSettingsStore::new(db.clone())
        .set(key, &value)
        .await
        .unwrap();
}
