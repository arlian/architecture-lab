//! # race-runner — the instrument.
//!
//! A race condition you cannot reproduce is an argument. This tool turns it
//! into a measurement, and it is the reason this lab has four services instead
//! of four blog posts.
//!
//! What it does is deliberately crude: seed a known number of units, fire every
//! reservation at the same instant, then ask the service what is left and check
//! the books. The only number that matters is the last one:
//!
//! ```text
//!   units handed out   (how many 200s × units each)
//!   units consumed     (seeded - remaining)
//!   ------------------
//!   the difference     <- stock that was sold and never subtracted
//! ```
//!
//! That difference is a lost update, expressed in the only unit anybody outside
//! engineering cares about: things you sold and do not have.
//!
//! ```text
//! cargo run -p race-runner                                  # naive, the default
//! cargo run -p race-runner -- --target all                  # all four, side by side
//! cargo run -p race-runner -- --target http://localhost:3022 --products 8
//! cargo run -p race-runner -- --target http://localhost:3021 --expect-version
//! ```
//!
//! ## Why every request must leave at once
//!
//! The requests are built as futures and handed to a single `join_all`, so none
//! of them is awaited — and therefore none of them is *sent* — until all of
//! them are ready. Loop-and-await instead, and every run passes against every
//! service, because a race needs overlap and a sequential loop never provides
//! any. The distinction between "load test" and "concurrency test" is exactly
//! this: a load test asks how fast; a concurrency test asks how *at the same
//! time*.

use std::time::Instant;

use futures::future::join_all;
use serde_json::{json, Value};

const USAGE: &str = "\
race-runner — reproduce a lost update on demand

USAGE:
    cargo run -p race-runner -- [OPTIONS]

OPTIONS:
    --target <url|all>   service to hit, or `all` for the lab's four default
                         ports in order            [default: http://localhost:3020]
    --product <name>     product key to reserve                   [default: mug]
    --stock <n>          units to seed per product before the run [default: 100]
    --concurrency <n>    reservations fired simultaneously        [default: 60]
    --units <n>          units per reservation                    [default: 2]
    --products <n>       spread the load over n products; shows the difference
                         between per-key and global locking       [default: 1]
    --expect-version     send the row version the client last saw, so the server
                         cannot retry on its behalf (optimistic-service only)
    -h, --help           print this help

THE PORTS:
    3020 naive        3021 optimistic        3022 pessimistic        3023 actor
";

const ALL_TARGETS: [&str; 4] = [
    "http://localhost:3020",
    "http://localhost:3021",
    "http://localhost:3022",
    "http://localhost:3023",
];

struct Args {
    targets: Vec<String>,
    product: String,
    stock: u64,
    concurrency: usize,
    units: u64,
    products: usize,
    expect_version: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            targets: vec![ALL_TARGETS[0].to_string()],
            product: "mug".into(),
            stock: 100,
            concurrency: 60,
            units: 2,
            products: 1,
            expect_version: false,
        }
    }
}

/// Pull the value that follows a flag, or explain which flag was left dangling.
fn next_value(raw: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    raw.next().ok_or_else(|| format!("`{flag}` needs a value"))
}

fn parse_number<T: std::str::FromStr>(raw: &str, flag: &str) -> Result<T, String> {
    raw.parse()
        .map_err(|_| format!("`{flag}` must be a number, got {raw:?}"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut raw = std::env::args().skip(1);

    while let Some(flag) = raw.next() {
        match flag.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--target" => {
                let value = next_value(&mut raw, &flag)?;
                args.targets = if value == "all" {
                    ALL_TARGETS.iter().map(|t| t.to_string()).collect()
                } else {
                    vec![value.trim_end_matches('/').to_string()]
                };
            }
            "--product" => args.product = next_value(&mut raw, &flag)?,
            "--stock" => args.stock = parse_number(&next_value(&mut raw, &flag)?, &flag)?,
            "--concurrency" => {
                args.concurrency = parse_number(&next_value(&mut raw, &flag)?, &flag)?
            }
            "--units" => args.units = parse_number(&next_value(&mut raw, &flag)?, &flag)?,
            "--products" => args.products = parse_number(&next_value(&mut raw, &flag)?, &flag)?,
            "--expect-version" => args.expect_version = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }

    if args.concurrency == 0 || args.products == 0 || args.units == 0 {
        return Err("`--concurrency`, `--products` and `--units` must be greater than zero".into());
    }

    Ok(args)
}

/// What one reservation turned into. The mapping from status code to meaning is
/// part of the lab: each strategy can only produce some of these, and which
/// ones it can produce is the strategy's whole personality.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// 200 — units were handed out.
    Granted,
    /// 409 — the row moved; the request was refused, not applied.
    Conflict,
    /// 422 — honestly out of stock.
    Rejected,
    /// 503 — the service shed the request rather than queue it.
    Busy,
    /// Anything else, including transport failures.
    Error(String),
}

struct Report {
    strategy: String,
    target: String,
    granted: usize,
    conflicts: usize,
    rejected: usize,
    busy: usize,
    errors: usize,
    first_error: Option<String>,
    units_granted: u64,
    units_consumed: u64,
    seeded: u64,
    left: u64,
    p50_ms: u128,
    p99_ms: u128,
    wall_ms: u128,
}

impl Report {
    /// Units sold that were never taken off the shelf. Zero is the only
    /// acceptable value, and it is the entire point of the lab.
    fn oversold(&self) -> u64 {
        self.units_granted.saturating_sub(self.units_consumed)
    }
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(args.concurrency.max(16))
        // No client timeout on purpose: a request that takes ten seconds is a
        // finding, not a failure, and cutting it off would hide the cost of the
        // serialising strategies.
        .build()
        .expect("failed to build HTTP client");

    let mut reports = Vec::new();
    let mut any_oversold = false;

    for target in &args.targets {
        match run_one(&client, target, &args).await {
            Ok(report) => {
                print_report(&report, &args);
                any_oversold |= report.oversold() > 0;
                reports.push(report);
            }
            Err(err) => {
                eprintln!("\n  {target}\n  could not run: {err}");
                eprintln!("  is the service up? see run-all.ps1 or the README.\n");
            }
        }
    }

    if reports.len() > 1 {
        print_comparison(&reports);
    }

    // A non-zero exit when stock was oversold, so this is usable as a check in
    // CI rather than only as a demo. A correctness property you can assert in a
    // pipeline is worth more than one you can only argue about in review.
    if any_oversold {
        std::process::exit(1);
    }
}

async fn run_one(
    client: &reqwest::Client,
    target: &str,
    args: &Args,
) -> Result<Report, Box<dyn std::error::Error>> {
    let names: Vec<String> = (0..args.products)
        .map(|i| {
            if args.products == 1 {
                args.product.clone()
            } else {
                format!("{}-{i}", args.product)
            }
        })
        .collect();

    // --- seed ---------------------------------------------------------------
    // Every strategy starts from the identical state, so the only variable in
    // the experiment is the code under test.
    let mut strategy = String::from("unknown");
    let mut versions = Vec::with_capacity(names.len());
    for name in &names {
        let body: Value = client
            .post(format!("{target}/stock/{name}/seed"))
            .json(&json!({ "units": args.stock }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        strategy = body["strategy"].as_str().unwrap_or("unknown").to_string();
        versions.push(body["version"].as_u64().unwrap_or(0));
    }

    // --- build every request, send none of them ------------------------------
    let attempts = (0..args.concurrency).map(|i| {
        let slot = i % names.len();
        let name = names[slot].clone();
        let mut payload = json!({ "units": args.units });
        if args.expect_version {
            payload["expected_version"] = json!(versions[slot]);
        }
        let url = format!("{target}/stock/{name}/reserve");

        async move {
            let started = Instant::now();
            let outcome = match client.post(&url).json(&payload).send().await {
                Ok(response) => match response.status().as_u16() {
                    200 => Outcome::Granted,
                    409 => Outcome::Conflict,
                    422 => Outcome::Rejected,
                    503 => Outcome::Busy,
                    other => Outcome::Error(format!("HTTP {other}")),
                },
                Err(err) => Outcome::Error(err.to_string()),
            };
            (outcome, started.elapsed())
        }
    });

    // --- and now, all at once ------------------------------------------------
    let wall = Instant::now();
    let settled = join_all(attempts).await;
    let wall_ms = wall.elapsed().as_millis();

    // --- read the books back -------------------------------------------------
    let mut left = 0u64;
    for name in &names {
        let body: Value = client
            .get(format!("{target}/stock/{name}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        left += body["available"].as_u64().unwrap_or(0);
    }

    let mut latencies: Vec<u128> = settled.iter().map(|(_, d)| d.as_millis()).collect();
    latencies.sort_unstable();

    let count = |want: &Outcome| settled.iter().filter(|(o, _)| o == want).count();
    let granted = count(&Outcome::Granted);
    let first_error = settled.iter().find_map(|(o, _)| match o {
        Outcome::Error(message) => Some(message.clone()),
        _ => None,
    });

    let seeded = args.stock * names.len() as u64;

    Ok(Report {
        strategy,
        target: target.to_string(),
        granted,
        conflicts: count(&Outcome::Conflict),
        rejected: count(&Outcome::Rejected),
        busy: count(&Outcome::Busy),
        errors: settled
            .iter()
            .filter(|(o, _)| matches!(o, Outcome::Error(_)))
            .count(),
        first_error,
        units_granted: granted as u64 * args.units,
        units_consumed: seeded.saturating_sub(left),
        seeded,
        left,
        p50_ms: percentile(&latencies, 0.50),
        p99_ms: percentile(&latencies, 0.99),
        wall_ms,
    })
}

fn percentile(sorted_ms: &[u128], p: f64) -> u128 {
    if sorted_ms.is_empty() {
        return 0;
    }
    let idx = ((sorted_ms.len() - 1) as f64 * p).round() as usize;
    sorted_ms[idx]
}

fn print_report(report: &Report, args: &Args) {
    let rule = "-".repeat(64);
    println!();
    println!("{rule}");
    println!("  {:<14}{}  ({})", "target", report.strategy, report.target);
    println!(
        "  {:<14}{} product(s) x {} units seeded = {} units on the shelf",
        "seeded",
        args.products,
        args.stock,
        report.seeded
    );
    println!(
        "  {:<14}{} concurrent x {} units = {} units requested{}",
        "attack",
        args.concurrency,
        args.units,
        args.concurrency as u64 * args.units,
        if args.expect_version {
            ", each pinned to a version"
        } else {
            ""
        }
    );
    println!("{rule}");
    println!("  200 granted   {:>6}", report.granted);
    println!("  409 conflict  {:>6}", report.conflicts);
    println!("  422 rejected  {:>6}", report.rejected);
    println!("  503 busy      {:>6}", report.busy);
    if report.errors > 0 {
        println!(
            "  ??? error     {:>6}   ({})",
            report.errors,
            report.first_error.as_deref().unwrap_or("")
        );
    }
    println!("{rule}");
    println!("  units handed out  {:>6}", report.units_granted);
    println!(
        "  units consumed    {:>6}   ({} seeded - {} left)",
        report.units_consumed, report.seeded, report.left
    );

    let oversold = report.oversold();
    if oversold > 0 {
        println!();
        println!("  *** OVERSOLD BY {oversold} UNITS ***");
        println!("  stock was promised to callers and never taken off the shelf.");
    } else {
        println!();
        println!("  CONSISTENT — every unit handed out was a unit consumed.");
    }

    println!();
    println!(
        "  latency  p50 {}ms   p99 {}ms   whole run {}ms",
        report.p50_ms, report.p99_ms, report.wall_ms
    );
    println!("{rule}");
}

fn print_comparison(reports: &[Report]) {
    println!();
    println!("  side by side");
    println!(
        "  {:<13} {:>8} {:>9} {:>9} {:>6} {:>8} {:>9}",
        "strategy", "granted", "conflicts", "oversold", "p50", "p99", "wall"
    );
    println!("  {}", "-".repeat(68));
    for r in reports {
        println!(
            "  {:<13} {:>8} {:>9} {:>9} {:>5}ms {:>6}ms {:>7}ms",
            r.strategy,
            r.granted,
            r.conflicts + r.busy,
            r.oversold(),
            r.p50_ms,
            r.p99_ms,
            r.wall_ms
        );
    }
    println!();
    println!("  `oversold` is the only column that is allowed to be non-zero nowhere.");
    println!("  everything else is a trade-off you get to pick.");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(units_granted: u64, seeded: u64, left: u64) -> Report {
        Report {
            strategy: "test".into(),
            target: "test".into(),
            granted: 0,
            conflicts: 0,
            rejected: 0,
            busy: 0,
            errors: 0,
            first_error: None,
            units_granted,
            units_consumed: seeded.saturating_sub(left),
            seeded,
            left,
            p50_ms: 0,
            p99_ms: 0,
            wall_ms: 0,
        }
    }

    #[test]
    fn a_correct_run_oversells_nothing() {
        // 50 callers took 2 units each out of 100, and 100 units left the shelf.
        assert_eq!(report(100, 100, 0).oversold(), 0);
    }

    #[test]
    fn a_lost_update_shows_up_as_units_that_never_left_the_shelf() {
        // 60 callers were each told yes — 120 units — but the shelf only went
        // down by 2, because 59 writers clobbered each other.
        assert_eq!(report(120, 100, 98).oversold(), 118);
    }

    #[test]
    fn refusing_everybody_is_consistent_even_though_it_is_useless() {
        assert_eq!(report(0, 100, 100).oversold(), 0);
    }

    #[test]
    fn percentiles_pick_a_real_sample_rather_than_interpolating() {
        let sorted = vec![1u128, 2, 3, 4, 100];
        assert_eq!(percentile(&sorted, 0.0), 1);
        assert_eq!(percentile(&sorted, 0.5), 3);
        assert_eq!(percentile(&sorted, 0.99), 100);
        assert_eq!(percentile(&[], 0.5), 0);
    }

    #[test]
    fn numbers_are_parsed_with_the_flag_named_in_the_error() {
        assert_eq!(parse_number::<u64>("42", "--stock"), Ok(42));
        assert!(parse_number::<u64>("lots", "--stock")
            .unwrap_err()
            .contains("--stock"));
    }
}
