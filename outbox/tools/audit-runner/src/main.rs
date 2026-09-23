//! # audit-runner — reconcile two sets of books.
//!
//! Place N orders at once, wait for the outbox to drain, then compare what
//! orders-service committed against what the ledger heard:
//!
//! ```text
//!   lost        = orders committed - unique events received
//!   duplicates  = deliveries       - unique events received
//! ```
//!
//! Every number is a delta across the run, so services never need a reset and
//! runs can be repeated back to back.
//!
//! ```text
//! cargo run -p audit-runner                    # both modes
//! cargo run -p audit-runner -- --target http://localhost:3031 --orders 500
//! ```

use futures::future::join_all;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const USAGE: &str = "\
audit-runner — find the events a dual write loses

OPTIONS:
    --target <url|all>   orders-service to hit    [default: all = :3030 naive, :3031 outbox]
    --ledger <url>       ledger-service           [default: http://localhost:3040]
    --orders <n>         orders placed at once    [default: 200]
    --amount <n>         amount per order         [default: 10]
";

const ALL_TARGETS: [&str; 2] = ["http://localhost:3030", "http://localhost:3031"];

struct Args {
    targets: Vec<String>,
    ledger: String,
    orders: usize,
    amount: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        targets: ALL_TARGETS.iter().map(|t| t.to_string()).collect(),
        ledger: "http://localhost:3040".into(),
        orders: 200,
        amount: 10,
    };
    let mut raw = std::env::args().skip(1);
    while let Some(flag) = raw.next() {
        let mut value = || raw.next().ok_or_else(|| format!("`{flag}` needs a value"));
        match flag.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--target" => {
                let v = value()?;
                if v != "all" {
                    args.targets = vec![v.trim_end_matches('/').to_string()];
                }
            }
            "--ledger" => args.ledger = value()?.trim_end_matches('/').to_string(),
            "--orders" => args.orders = value()?.parse().map_err(|_| "`--orders` must be a number")?,
            "--amount" => args.amount = value()?.parse().map_err(|_| "`--amount` must be a number")?,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(args)
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value, reqwest::Error> {
    client.get(url).send().await?.error_for_status()?.json().await
}

fn n(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

#[tokio::main]
async fn main() {
    let args = parse_args().unwrap_or_else(|err| {
        eprintln!("error: {err}\n\n{USAGE}");
        std::process::exit(2);
    });
    let client = reqwest::Client::new();

    let mut any_lost = false;
    for target in &args.targets {
        match audit(&client, target, &args).await {
            Ok(lost) => any_lost |= lost > 0,
            Err(err) => eprintln!("\n  {target}: could not run: {err}\n  is it up? see run-all.ps1\n"),
        }
    }
    // Non-zero when anything was lost, so this works as a CI check. Duplicates
    // do not fail the run: at-least-once is the contract, the inbox is the fix.
    if any_lost {
        std::process::exit(1);
    }
}

async fn audit(client: &reqwest::Client, target: &str, args: &Args) -> Result<u64, reqwest::Error> {
    let before = get_json(client, &format!("{target}/stats")).await?;
    let mode = before["mode"].as_str().unwrap_or("unknown").to_string();
    let books_url = format!("{}/books/{mode}", args.ledger);
    let books_before = get_json(client, &books_url).await?;

    // Build every request first, send them together.
    let attempts = (0..args.orders).map(|_| {
        client
            .post(format!("{target}/orders"))
            .json(&json!({ "amount": args.amount }))
            .send()
    });
    let answered_201 = join_all(attempts)
        .await
        .iter()
        .filter(|r| matches!(r, Ok(resp) if resp.status().as_u16() == 201))
        .count() as u64;

    // Let the relay drain. Naive has no outbox, so this returns at once.
    let deadline = Instant::now() + Duration::from_secs(10);
    let after = loop {
        let s = get_json(client, &format!("{target}/stats")).await?;
        if n(&s, "/pending_outbox") == 0 || Instant::now() > deadline {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let books_after = get_json(client, &books_url).await?;

    let delta = |a: &Value, b: &Value, p: &str| n(b, p).saturating_sub(n(a, p));
    let committed = delta(&before, &after, "/orders");
    let revenue = delta(&before, &after, "/revenue");
    let deliveries = delta(&books_before, &books_after, "/deliveries");
    let unique = delta(&books_before, &books_after, "/with_inbox/events");
    let rev_raw = delta(&books_before, &books_after, "/without_inbox/revenue");
    let rev_inbox = delta(&books_before, &books_after, "/with_inbox/revenue");
    let lost = committed.saturating_sub(unique);

    let rule = "-".repeat(56);
    println!("\n{rule}\n  {mode}  ({target})\n{rule}");
    println!("  orders placed        {:>6}", args.orders);
    println!("  client saw 201       {:>6}", answered_201);
    println!("  orders committed     {:>6}", committed);
    println!("  events delivered     {:>6}", deliveries);
    println!("  unique events        {:>6}", unique);
    println!("{rule}");
    println!("  LOST                 {:>6}", lost);
    println!("  duplicates           {:>6}", deliveries.saturating_sub(unique));
    println!("{rule}");
    println!("  revenue in orders db {:>6}", revenue);
    println!("  ledger, no inbox     {:>6}", rev_raw);
    println!("  ledger, with inbox   {:>6}", rev_inbox);
    if n(&after, "/pending_outbox") > 0 {
        println!("  (outbox still had {} pending at the deadline)", n(&after, "/pending_outbox"));
    }
    Ok(lost)
}
