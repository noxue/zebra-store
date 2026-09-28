//! Order status machine (`allowedTransitions`, `CalcParentStatus`, refund status sync).

use zs_shared::money::Amount;

use super::model::{Order, OrderItem, OrderStatus, fulfillment_type};

/// Whether `current → target` is allowed for a single order (`IsTransitionAllowed`).
pub fn is_transition_allowed(current: OrderStatus, target: OrderStatus) -> bool {
    use OrderStatus::*;
    if current == target {
        return true;
    }
    match current {
        PendingPayment => matches!(target, Paid | Canceled),
        Paid => matches!(
            target,
            Fulfilling | PartiallyDelivered | Delivered | PartiallyRefunded | Refunded
        ),
        Fulfilling => matches!(
            target,
            PartiallyDelivered | Delivered | PartiallyRefunded | Refunded
        ),
        PartiallyDelivered => {
            matches!(target, Delivered | Completed | PartiallyRefunded | Refunded)
        }
        Delivered => matches!(target, Completed | PartiallyRefunded | Refunded),
        Completed => matches!(target, PartiallyRefunded | Refunded),
        PartiallyRefunded => matches!(target, Refunded),
        Canceled | Refunded => false,
    }
}

/// Aggregates the children's statuses into the parent's (`CalcParentStatus`).
pub fn calc_parent_status(children: &[OrderStatus], current: OrderStatus) -> OrderStatus {
    use OrderStatus::*;
    if children.is_empty() {
        return current;
    }
    let count = |s: OrderStatus| children.iter().filter(|c| **c == s).count();
    let total = children.len();
    let (delivered, completed) = (count(Delivered), count(Completed));
    let (refunded, partially_refunded) = (count(Refunded), count(PartiallyRefunded));
    if count(Canceled) == total {
        return Canceled;
    }
    if refunded == total {
        return Refunded;
    }
    if refunded > 0 || partially_refunded > 0 {
        return PartiallyRefunded;
    }
    if completed == total {
        return Completed;
    }
    if delivered + completed == total {
        return Delivered;
    }
    if delivered + completed > 0 {
        return PartiallyDelivered;
    }
    if count(Fulfilling) > 0 {
        return Fulfilling;
    }
    if count(Paid) > 0 {
        return Paid;
    }
    if count(PendingPayment) > 0 {
        return PendingPayment;
    }
    current
}

/// Refund status an order should be in given its totals (`expectedRefundStatus`).
pub fn expected_refund_status(order: &Order) -> Option<OrderStatus> {
    if order.status == OrderStatus::Canceled || order.paid_at.is_none() {
        return None;
    }
    let total = order.total_amount;
    if !total.is_positive() || !order.refunded_amount.is_positive() {
        return None;
    }
    Some(if order.refunded_amount >= total {
        OrderStatus::Refunded
    } else {
        OrderStatus::PartiallyRefunded
    })
}

/// A paid child with manual/upstream items goes to `fulfilling` (`shouldMarkFulfilling`).
pub fn should_mark_fulfilling(items: &[OrderItem]) -> bool {
    items.iter().any(|i| {
        let ft = i.fulfillment_type.trim();
        ft.is_empty() || ft == fulfillment_type::MANUAL || ft == fulfillment_type::UPSTREAM
    })
}

/// Every item is auto-delivered (`shouldAutoFulfill`).
pub fn should_auto_fulfill(items: &[OrderItem]) -> bool {
    !items.is_empty()
        && items
            .iter()
            .all(|i| i.fulfillment_type.trim() == fulfillment_type::AUTO)
}

/// The whole order is auto-delivered (`isOrderFullyAutoFulfill`, NTF-01).
pub fn is_fully_auto(order: &Order) -> bool {
    if order.children.is_empty() {
        should_auto_fulfill(&order.items)
    } else {
        order.children.iter().all(|c| should_auto_fulfill(&c.items))
    }
}

/// At least one item needs a human (`hasManualFulfillmentItems`, NTF-07): blank counts as manual,
/// upstream does not.
pub fn has_manual_items(items: &[OrderItem]) -> bool {
    items
        .iter()
        .any(|i| fulfillment_type::normalize(&i.fulfillment_type) == fulfillment_type::MANUAL)
}

/// Whether an admin may mark a parent `completed` (`canCompleteParentOrder`).
pub fn can_complete_parent(order: &Order) -> bool {
    order.status == OrderStatus::Delivered
        && order
            .children
            .iter()
            .all(|c| matches!(c.status, OrderStatus::Delivered | OrderStatus::Completed))
}

/// Status after a refund of the order's total (`refunded` when fully refunded).
pub fn refund_target(new_refunded: Amount, total: Amount) -> OrderStatus {
    if new_refunded >= total {
        OrderStatus::Refunded
    } else {
        OrderStatus::PartiallyRefunded
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::testkit::{item, order};
    use super::*;
    use OrderStatus::*;

    #[test]
    fn transitions_follow_the_original_table() {
        assert!(is_transition_allowed(PendingPayment, Paid));
        assert!(is_transition_allowed(PendingPayment, Canceled));
        assert!(!is_transition_allowed(PendingPayment, Delivered));
        assert!(is_transition_allowed(Paid, Fulfilling));
        assert!(!is_transition_allowed(Paid, Completed));
        assert!(!is_transition_allowed(Paid, Canceled));
        assert!(is_transition_allowed(Delivered, Completed));
        assert!(is_transition_allowed(PartiallyRefunded, Refunded));
        assert!(!is_transition_allowed(Refunded, PartiallyRefunded));
        assert!(!is_transition_allowed(Canceled, PendingPayment));
        assert!(is_transition_allowed(Canceled, Canceled));
    }

    /// RFD-02: parent aggregation of refunded children.
    #[test]
    fn rfd_02_parent_status() {
        assert_eq!(
            calc_parent_status(&[Refunded, Completed], Paid),
            PartiallyRefunded
        );
        assert_eq!(calc_parent_status(&[Refunded, Refunded], Paid), Refunded);
        assert_eq!(calc_parent_status(&[Completed, Completed], Paid), Completed);
        assert_eq!(calc_parent_status(&[Completed, Delivered], Paid), Delivered);
        assert_eq!(
            calc_parent_status(&[Completed, Fulfilling], Paid),
            PartiallyDelivered
        );
        assert_eq!(calc_parent_status(&[Paid, Fulfilling], Paid), Fulfilling);
        assert_eq!(calc_parent_status(&[Paid, Paid], Fulfilling), Paid);
        assert_eq!(calc_parent_status(&[Canceled, Canceled], Paid), Canceled);
        assert_eq!(
            calc_parent_status(&[Canceled, PendingPayment], Paid),
            PendingPayment
        );
        assert_eq!(calc_parent_status(&[], Delivered), Delivered);
    }

    #[test]
    fn refund_status_sync() {
        let mut o = order(1, Completed);
        assert_eq!(expected_refund_status(&o), None);
        o.paid_at = Some(o.created_at);
        o.refunded_amount = Amount::from(30);
        assert_eq!(expected_refund_status(&o), Some(PartiallyRefunded));
        o.refunded_amount = Amount::from(100);
        assert_eq!(expected_refund_status(&o), Some(Refunded));
        o.status = Canceled;
        assert_eq!(expected_refund_status(&o), None);
    }

    /// NTF-07: upstream items never trigger the manual-pending alert.
    #[test]
    fn ntf_07_manual_items() {
        assert!(!has_manual_items(&[]));
        assert!(!has_manual_items(&[item(1, "upstream", 1)]));
        assert!(!has_manual_items(&[item(1, "auto", 1)]));
        assert!(has_manual_items(&[item(1, "manual", 1)]));
        assert!(has_manual_items(&[item(1, "  ", 1)]));
        assert!(has_manual_items(&[
            item(1, "upstream", 1),
            item(2, "manual", 1)
        ]));
    }

    #[test]
    fn fulfilling_and_auto_rules() {
        assert!(should_mark_fulfilling(&[item(1, "upstream", 1)]));
        assert!(!should_mark_fulfilling(&[item(1, "auto", 1)]));
        assert!(should_auto_fulfill(&[item(1, "auto", 1)]));
        assert!(!should_auto_fulfill(&[]));
        let mut parent = order(1, Paid);
        let mut c1 = order(2, Paid);
        c1.items.push(item(1, "auto", 1));
        parent.children.push(c1.clone());
        assert!(is_fully_auto(&parent));
        let mut c2 = order(3, Paid);
        c2.items.push(item(2, "manual", 1));
        parent.children.push(c2);
        assert!(!is_fully_auto(&parent));
    }
}
