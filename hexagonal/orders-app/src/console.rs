//! Driving adapter #2: a command line.
//!
//! **Read this file next to `http.rs`.** Same three use cases, same core
//! object, no framework, no port number, no JSON — and, notably, no code
//! shared with the HTTP adapter beyond the calls into `OrderService`.
//!
//! It does exactly the same job as `http.rs`, in the same two directions:
//!
//! * inbound — argv becomes a `PlaceOrder` command. A bad UUID here is a
//!   *usage* error, and the core never hears about it, exactly as a malformed
//!   JSON body never reaches it over there;
//! * outbound — an `Order` becomes lines of text, and a `DomainError` becomes
//!   a process exit code. `NotFound` is `404` in `http.rs` and `exit 1` here.
//!   Neither is a fact about orders, which is why neither lives in the core.
//!
//! If you want the shortest possible summary of what hexagonal architecture
//! buys: this adapter was written after the HTTP one, and adding it required
//! changing nothing in `orders-core`.

use std::sync::Arc;

use orders_core::{DomainError, Order, OrderId, OrderService, PlaceOrder, PlaceOrderLine};
use uuid::Uuid;

const USAGE: &str = "\
usage:
  cli place <user-id> <product-id> <quantity>
  cli get <order-id>
  cli list";

/// Runs one command and returns. `Err` is printed by the caller, which then
/// exits non-zero — this adapter's equivalent of an error status code.
pub async fn run(service: Arc<OrderService>, args: &[String]) -> Result<(), DomainError> {
    let rest: &[String] = if args.is_empty() { &[] } else { &args[1..] };

    match args.first().map(String::as_str) {
        Some("place") => {
            expect_args(rest, 3)?;
            let cmd = PlaceOrder {
                user_id: orders_core::UserId(parse_uuid(&rest[0], "user id")?),
                lines: vec![PlaceOrderLine {
                    product_id: orders_core::ProductId(parse_uuid(&rest[1], "product id")?),
                    quantity: rest[2].parse().map_err(|_| {
                        DomainError::Validation(format!("quantity {} is not a number", rest[2]))
                    })?,
                }],
            };
            let order = service.place(cmd).await?;
            println!("placed order {}", order.id);
            print_order(&order);
        }
        Some("get") => {
            expect_args(rest, 1)?;
            let id = OrderId(parse_uuid(&rest[0], "order id")?);
            print_order(&service.get(id).await?);
        }
        Some("list") => {
            let orders = service.list().await?;
            if orders.is_empty() {
                println!("no orders yet");
            }
            for order in &orders {
                println!(
                    "{}  {} line(s)  {}",
                    order.id,
                    order.lines.len(),
                    money(order.total_cents)
                );
            }
        }
        _ => return Err(DomainError::Validation(USAGE.into())),
    }
    Ok(())
}

/// The console equivalent of axum rejecting a malformed body: shape problems
/// are settled here, in the adapter, so the core only ever sees a well-formed
/// command.
fn expect_args(args: &[String], n: usize) -> Result<(), DomainError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(DomainError::Validation(format!(
            "expected {n} argument(s), got {}\n{USAGE}",
            args.len()
        )))
    }
}

fn parse_uuid(raw: &str, what: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw).map_err(|_| DomainError::Validation(format!("{what} {raw} is not a UUID")))
}

fn print_order(order: &Order) {
    println!("order   {}", order.id);
    println!("user    {}", order.user_id);
    for line in &order.lines {
        println!(
            "line    {} x{} @ {}",
            line.product_id,
            line.quantity,
            money(line.unit_price_cents)
        );
    }
    println!("total   {}", money(order.total_cents));
}

fn money(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}
