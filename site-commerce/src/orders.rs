//! Stock and held orders: the shop's inventory and its order book.
//!
//! Prices and quantities live here, not in the release: they move with every
//! order, while releases move with owner approvals. [`Commerce::order_hold`]
//! reserves stock and files the hold's approval card, the owner approves
//! through the inbox, and [`Commerce::order_complete`] verifies that approval
//! and finalizes — or [`Commerce::order_cancel`] restores the reservation.
//! Holding twice under one idempotency key returns one order.

use agentrix_site_inbox::{Card, CardKind, CardState, CardStore, NewCard};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::payments::Commerce;

/// What can go wrong holding or completing an order.
#[derive(Debug, Error)]
pub enum OrderError {
    /// The database failed.
    #[error("order store: {0}")]
    Store(String),

    /// No row with that id.
    #[error("unknown order {0}")]
    UnknownOrder(String),

    /// No stock row for that SKU.
    #[error("unknown sku {0}")]
    UnknownSku(String),

    /// Not enough units left.
    #[error("only {have} of {sku} left, {want} wanted")]
    OutOfStock {
        /// The SKU.
        sku: String,
        /// Units wanted.
        want: i64,
        /// Units left.
        have: i64,
    },

    /// The approval card is missing, unapproved, or names another order.
    #[error("bad order approval: {0}")]
    BadApproval(String),

    /// The order is not held.
    #[error("order {0} is not held")]
    NotHeld(String),
}

impl From<sqlx::Error> for OrderError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.to_string())
    }
}

impl From<agentrix_site_inbox::StoreError> for OrderError {
    fn from(error: agentrix_site_inbox::StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

/// A stock row: units and unit price in micro of `currency`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stock {
    /// The SKU.
    pub sku: String,
    /// Units on hand.
    pub qty: i64,
    /// Micro per unit.
    pub unit_micro: i64,
    /// The price currency.
    pub currency: String,
}

/// An order's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    /// Reserved, awaiting approval.
    Held,
    /// Approved and finalized.
    Completed,
    /// Released back to stock.
    Cancelled,
}

impl OrderState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => Self::Held,
        }
    }
}

/// A held order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// The order id.
    pub id: String,
    /// The site.
    pub site: String,
    /// The SKU.
    pub sku: String,
    /// Units ordered.
    pub qty: i64,
    /// Micro per unit at hold time.
    pub unit_micro: i64,
    /// Total micro at hold time.
    pub total_micro: i64,
    /// The price currency.
    pub currency: String,
    /// Who ordered.
    pub buyer: String,
    /// The state.
    pub state: OrderState,
    /// The hold's approval card.
    pub card_id: String,
}

/// A decoded order row.
type OrderRow = (
    String,
    String,
    i64,
    i64,
    i64,
    String,
    String,
    String,
    String,
);

/// A new hold request.
#[derive(Debug, Clone)]
pub struct NewOrder {
    /// The SKU.
    pub sku: String,
    /// Units wanted.
    pub qty: i64,
    /// Who orders.
    pub buyer: String,
    /// Holds twice under one key return one order.
    pub idempotency_key: String,
}

impl Commerce {
    /// Sets `sku`'s stock and unit price.
    pub async fn set_stock(
        &self,
        site: &str,
        sku: &str,
        qty: i64,
        unit_micro: i64,
        currency: &str,
    ) -> Result<(), OrderError> {
        sqlx::query(
            "INSERT INTO stock (site, sku, qty, unit_micro, currency) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (site, sku) DO UPDATE SET qty = excluded.qty,
             unit_micro = excluded.unit_micro, currency = excluded.currency",
        )
        .bind(site)
        .bind(sku)
        .bind(qty)
        .bind(unit_micro)
        .bind(currency)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The stock row for `sku`, or `None`.
    pub async fn stock_of(&self, site: &str, sku: &str) -> Result<Option<Stock>, OrderError> {
        let row: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT qty, unit_micro, currency FROM stock WHERE site = ? AND sku = ?",
        )
        .bind(site)
        .bind(sku)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(qty, unit_micro, currency)| Stock {
            sku: sku.to_string(),
            qty,
            unit_micro,
            currency,
        }))
    }

    /// Holds an order: reserves the units atomically and files the hold's
    /// approval card. Holding twice under one idempotency key returns the
    /// first order and files nothing new.
    pub async fn order_hold(
        &self,
        site: &str,
        cards: &CardStore,
        order: NewOrder,
    ) -> Result<(Order, Card), OrderError> {
        if let Some(existing) = self.order_by_key(&order.idempotency_key).await? {
            let card = cards.get(&existing.card_id).await?.ok_or_else(|| {
                OrderError::BadApproval(format!("order {} lost its card", existing.id))
            })?;
            return Ok((existing, card));
        }
        let stock = self
            .stock_of(site, &order.sku)
            .await?
            .ok_or_else(|| OrderError::UnknownSku(order.sku.clone()))?;
        let taken =
            sqlx::query("UPDATE stock SET qty = qty - ? WHERE site = ? AND sku = ? AND qty >= ?")
                .bind(order.qty)
                .bind(site)
                .bind(&order.sku)
                .bind(order.qty)
                .execute(&self.pool)
                .await?;
        if taken.rows_affected() == 0 {
            let have = self
                .stock_of(site, &order.sku)
                .await?
                .map(|s| s.qty)
                .unwrap_or(0);
            return Err(OrderError::OutOfStock {
                sku: order.sku,
                want: order.qty,
                have,
            });
        }
        let held = Order {
            id: format!("ord-{}", Uuid::new_v4().as_simple()),
            site: site.to_string(),
            sku: order.sku.clone(),
            qty: order.qty,
            unit_micro: stock.unit_micro,
            total_micro: stock.unit_micro * order.qty,
            currency: stock.currency.clone(),
            buyer: order.buyer.clone(),
            state: OrderState::Held,
            card_id: String::new(),
        };
        let card = cards
            .file(NewCard::new(
                site,
                CardKind::Approval,
                "Sales Rep",
                format!(
                    "Sales Rep: confirm order of {} x {} ({} micro-{})",
                    held.qty, held.sku, held.total_micro, held.currency
                ),
                serde_json::json!({ "order": held.id, "sku": held.sku, "qty": held.qty }),
            ))
            .await?;
        let mut held = held;
        held.card_id = card.id.clone();
        sqlx::query(
            "INSERT INTO orders
             (id, site, sku, qty, unit_micro, total_micro, currency, buyer, state, card_id, idempotency_key)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&held.id)
        .bind(&held.site)
        .bind(&held.sku)
        .bind(held.qty)
        .bind(held.unit_micro)
        .bind(held.total_micro)
        .bind(&held.currency)
        .bind(&held.buyer)
        .bind(held.state.as_str())
        .bind(&held.card_id)
        .bind(&order.idempotency_key)
        .execute(&self.pool)
        .await?;
        Ok((held, card))
    }

    /// Completes a held order: verifies `approval_card` is an approved
    /// `Approval` naming the order, then finalizes. Completing twice returns
    /// the order without touching stock again.
    pub async fn order_complete(
        &self,
        cards: &CardStore,
        order_id: &str,
        approval_card: &str,
    ) -> Result<Order, OrderError> {
        let order = self
            .order_by_id(order_id)
            .await?
            .ok_or_else(|| OrderError::UnknownOrder(order_id.to_string()))?;
        if order.state == OrderState::Completed {
            return Ok(order);
        }
        if order.state != OrderState::Held {
            return Err(OrderError::NotHeld(order_id.to_string()));
        }
        let card = cards
            .get(approval_card)
            .await?
            .ok_or_else(|| OrderError::BadApproval(format!("no card {approval_card}")))?;
        if card.kind != CardKind::Approval {
            return Err(OrderError::BadApproval(format!(
                "card {approval_card} is not an order approval"
            )));
        }
        if card.state != CardState::Approved {
            return Err(OrderError::BadApproval(format!(
                "card {approval_card} is not approved"
            )));
        }
        let named = card
            .payload
            .get("order")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if named != order.id {
            return Err(OrderError::BadApproval(format!(
                "card {approval_card} names a different order"
            )));
        }
        sqlx::query("UPDATE orders SET state = 'completed' WHERE id = ?")
            .bind(&order.id)
            .execute(&self.pool)
            .await?;
        Ok(Order {
            state: OrderState::Completed,
            ..order
        })
    }

    /// Cancels a held order and restores its units to stock.
    pub async fn order_cancel(&self, order_id: &str) -> Result<Order, OrderError> {
        let order = self
            .order_by_id(order_id)
            .await?
            .ok_or_else(|| OrderError::UnknownOrder(order_id.to_string()))?;
        if order.state != OrderState::Held {
            return Err(OrderError::NotHeld(order_id.to_string()));
        }
        sqlx::query("UPDATE stock SET qty = qty + ? WHERE site = ? AND sku = ?")
            .bind(order.qty)
            .bind(&order.site)
            .bind(&order.sku)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE orders SET state = 'cancelled' WHERE id = ?")
            .bind(&order.id)
            .execute(&self.pool)
            .await?;
        Ok(Order {
            state: OrderState::Cancelled,
            ..order
        })
    }

    /// The order `id`, or `None`.
    pub async fn order_by_id(&self, id: &str) -> Result<Option<Order>, OrderError> {
        let row: Option<OrderRow> = sqlx::query_as(
            "SELECT site, sku, qty, unit_micro, total_micro, currency, buyer, state, card_id
                 FROM orders WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(site, sku, qty, unit_micro, total_micro, currency, buyer, state, card_id)| Order {
                id: id.to_string(),
                site,
                sku,
                qty,
                unit_micro,
                total_micro,
                currency,
                buyer,
                state: OrderState::parse(&state),
                card_id,
            },
        ))
    }

    async fn order_by_key(&self, key: &str) -> Result<Option<Order>, OrderError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM orders WHERE idempotency_key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        match row {
            Some((id,)) => self.order_by_id(&id).await,
            None => Ok(None),
        }
    }
}

/// Creates the stock and order tables.
pub(crate) async fn migrate(pool: &sqlx::sqlite::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock (
            site TEXT NOT NULL,
            sku TEXT NOT NULL,
            qty INTEGER NOT NULL,
            unit_micro INTEGER NOT NULL,
            currency TEXT NOT NULL,
            PRIMARY KEY (site, sku)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS orders (
            id TEXT PRIMARY KEY,
            site TEXT NOT NULL,
            sku TEXT NOT NULL,
            qty INTEGER NOT NULL,
            unit_micro INTEGER NOT NULL,
            total_micro INTEGER NOT NULL,
            currency TEXT NOT NULL,
            buyer TEXT NOT NULL,
            state TEXT NOT NULL,
            card_id TEXT NOT NULL,
            idempotency_key TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}
