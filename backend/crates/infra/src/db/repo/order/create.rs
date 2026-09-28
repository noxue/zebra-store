//! Order creation in one transaction (`createOrder` transaction body).

use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
use zs_domain::catalog::ordering::{StockMove, StockTarget};
use zs_domain::marketing::coupon::CouponClaim;
use zs_domain::order::guest::hash_credential;
use zs_domain::order::model::{Order, OrderItem};
use zs_domain::order::ports::{NewOrder, Reservation};
use zs_domain::order::risk::{check_pending, pending_lock_keys, pending_query};
use zs_domain::{Error, Id, Result};

use super::{map, risk};
use crate::db::entity::{order_items, orders};
use crate::db::repo::catalog::ordering::{manual_stock_in, reserve_secrets_in};
use crate::db::repo::integration::downstream::{DUPLICATE_KEY, insert_ref_in};
use crate::db::repo::marketing::coupon::claim_in;
use crate::db::repo::reseller::ledger::create_order_snapshot_in;
use crate::db::repo::support::{DbResultExt, to_json};

fn order_model(
    o: &Order,
    parent_id: Option<Id>,
    guest_secret: &str,
    now: DateTime<Utc>,
) -> orders::ActiveModel {
    let guest_password = if o.user_id == 0 && !o.guest_password.trim().is_empty() {
        hash_credential(guest_secret, &o.guest_email, &o.guest_password)
    } else {
        String::new()
    };
    orders::ActiveModel {
        order_no: Set(o.order_no.clone()),
        parent_id: Set(parent_id),
        user_id: Set(o.user_id),
        guest_email: Set(o.guest_email.clone()),
        guest_password: Set(guest_password),
        guest_locale: Set(o.guest_locale.clone()),
        status: Set(o.status.as_str().to_owned()),
        currency: Set(o.currency.clone()),
        original_amount: Set(o.original_amount.decimal()),
        discount_amount: Set(o.discount_amount.decimal()),
        member_discount_amount: Set(o.member_discount_amount.decimal()),
        promotion_discount_amount: Set(o.promotion_discount_amount.decimal()),
        wholesale_discount_amount: Set(o.wholesale_discount_amount.decimal()),
        total_amount: Set(o.total_amount.decimal()),
        wallet_paid_amount: Set(o.wallet_paid_amount.decimal()),
        online_paid_amount: Set(o.online_paid_amount.decimal()),
        refunded_amount: Set(o.refunded_amount.decimal()),
        member_level_id: Set(o.member_level_id),
        coupon_id: Set(o.coupon_id),
        promotion_id: Set(o.promotion_id),
        affiliate_profile_id: Set(o.affiliate_profile_id),
        affiliate_code: Set(o.affiliate_code.clone()),
        reseller_id: Set(o.reseller_id),
        reseller_domain: Set(o.reseller_domain.clone()),
        reseller_profit_amount: Set(o.reseller_profit_amount.decimal()),
        client_ip: Set(o.client_ip.clone()),
        risk_ip: Set(o.risk_ip.clone()),
        expires_at: Set(o.expires_at),
        paid_at: Set(None),
        canceled_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
}

fn item_model(i: &OrderItem, order_id: Id, now: DateTime<Utc>) -> Result<order_items::ActiveModel> {
    Ok(order_items::ActiveModel {
        order_id: Set(order_id),
        product_id: Set(i.product_id),
        sku_id: Set(i.sku_id),
        title_json: Set(to_json(&i.title)?),
        sku_snapshot_json: Set(to_json(&i.sku_snapshot)?),
        tags: Set(to_json(&i.tags)?),
        original_unit_price: Set(i.original_unit_price.decimal()),
        unit_price: Set(i.unit_price.decimal()),
        cost_price: Set(i.cost_price.decimal()),
        quantity: Set(i.quantity),
        original_total_price: Set(i.original_total_price.decimal()),
        total_price: Set(i.total_price.decimal()),
        coupon_discount: Set(i.coupon_discount.decimal()),
        member_discount: Set(i.member_discount.decimal()),
        promotion_discount: Set(i.promotion_discount.decimal()),
        wholesale_discount: Set(i.wholesale_discount.decimal()),
        promotion_id: Set(i.promotion_id),
        fulfillment_type: Set(i.fulfillment_type.clone()),
        manual_form_schema_snapshot_json: Set(to_json(&i.manual_form_schema)?),
        manual_form_submission_json: Set(to_json(&i.manual_form_submission)?),
        instructions_json: Set(to_json(&i.instructions)?),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    })
}

/// Runs the whole creation on `conn` (a transaction).
pub(crate) async fn create_in<C: ConnectionTrait>(
    conn: &C,
    new: &NewOrder,
    guest_secret: &str,
    now: DateTime<Utc>,
) -> Result<Order> {
    if new.order.children.len() != new.reservations.len() || new.order.children.is_empty() {
        return Err(Error::bad_request(
            zs_domain::order::keys::ORDER_ITEM_INVALID,
        ));
    }
    if let Some(gate) = &new.risk {
        let keys = pending_lock_keys(&gate.prep, &gate.input);
        risk::lock_keys(conn, &keys, now).await?;
        let counts = risk::pending_counts(conn, &pending_query(&gate.prep, &gate.input)).await?;
        check_pending(&gate.prep, &gate.input, &counts)?;
    }

    let parent = order_model(&new.order, None, guest_secret, now)
        .insert(conn)
        .await
        .dom()?;
    if let Some(r) = &new.downstream_ref {
        let row = zs_domain::integration::downstream::NewOrderRef {
            order_id: parent.id,
            ..r.clone()
        };
        insert_ref_in(conn, &row, now).await.map_err(|e| {
            if e.key() == DUPLICATE_KEY {
                Error::bad_request(zs_domain::order::keys::DOWNSTREAM_ORDER_DUPLICATE)
            } else {
                e
            }
        })?;
    }
    let mut pricing = new.reseller.clone();
    for (idx, (child, reservation)) in new.order.children.iter().zip(&new.reservations).enumerate()
    {
        let child_row = order_model(child, Some(parent.id), guest_secret, now)
            .insert(conn)
            .await
            .dom()?;
        let mut item_id = 0;
        for item in &child.items {
            item_id = item_model(item, child_row.id, now)?
                .insert(conn)
                .await
                .dom()?
                .id;
        }
        if let Some(ctx) = pricing.as_mut() {
            ctx.bind_created_order_item(idx, child_row.id, item_id);
        }
        match *reservation {
            Reservation::None => {}
            Reservation::Secrets {
                product_id,
                sku_id,
                quantity,
            } => {
                reserve_secrets_in(conn, product_id, sku_id, quantity, child_row.id, now).await?;
            }
            Reservation::ManualSku { sku_id, quantity } => {
                manual_stock_in(conn, StockTarget::Sku(sku_id), StockMove::Reserve, quantity)
                    .await?;
            }
        }
    }
    if let Some((coupon_id, discount)) = new.coupon {
        claim_in(
            conn,
            &CouponClaim {
                coupon_id,
                user_id: new.order.user_id,
                order_id: parent.id,
                discount_amount: discount,
            },
            now,
        )
        .await?;
    }
    if let Some(ctx) = &pricing {
        create_order_snapshot_in(conn, parent.id, ctx, now).await?;
    }
    map::load(conn, parent.id)
        .await?
        .ok_or_else(|| Error::internal_msg("created order vanished"))
}
