//! The scatter-gather itself — the entire reason search-gateway exists.
//!
//! One inbound request becomes N outbound requests, all in flight at once, and
//! then has to become one response again. Everything interesting about this
//! pattern lives in the *gather* half, and it comes down to three decisions
//! that the code below makes explicitly:
//!
//! 1. **Isolation.** Each branch is wrapped in its own `timeout` and its own
//!    `Result`, so no branch can fail, hang, or slow down another. Contrast
//!    `bff/services/web-bff/src/views.rs`, which uses `try_join_all`: there,
//!    the first `Err` aborts the join and fails the client's request. Here the
//!    call is `join_all` — every branch is allowed to finish having failed.
//!
//! 2. **A deadline, not a hope.** The budget is spent *concurrently*, so the
//!    whole request costs roughly `budget`, not `n * budget`. A provider that
//!    misses it is dropped from this answer — its socket abandoned mid-flight —
//!    rather than made everyone else's problem. Fan-out without a deadline
//!    means your slowest dependency sets your latency, forever.
//!
//! 3. **Completeness is data.** The response says which branches contributed,
//!    which timed out, which failed, and how long each took. A caller that gets
//!    `degraded: true` knows it is looking at a partial view and can decide
//!    what that's worth. The alternative — silently returning fewer results —
//!    is the bug this pattern quietly produces in the wild.
//!
//! The status code is `200` in all of those cases. That is a deliberate,
//! arguable choice, and it's the first thing worth pushing back on when you
//! read this lab; see the README's "the argument to have" section.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::join_all;
use serde::Serialize;
use tokio::time::timeout;
use uuid::Uuid;

use crate::provider::SearchProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    /// Answered inside the budget.
    Ok,
    /// Still hadn't answered when the budget ran out.
    TimedOut,
    /// Answered, but with an error (or wasn't reachable at all).
    Failed,
}

#[derive(Debug, Serialize)]
pub struct ProviderReport {
    pub name: String,
    pub status: ProviderStatus,
    /// Wall-clock time this branch took, including the wait for a timeout.
    pub took_ms: u64,
    /// How many hits this branch contributed *before* merging.
    pub hits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One product in the merged answer.
///
/// `sources` is the field that only exists because of the fan-out: it says
/// which providers offered this product, which is information no single
/// provider could have produced.
#[derive(Debug, Serialize)]
pub struct MergedHit {
    pub id: Uuid,
    pub name: String,
    pub price_cents: u64,
    pub sources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    /// True if any branch did not return `ok`. The client is holding a partial
    /// answer and is being told so.
    pub degraded: bool,
    pub hits: Vec<MergedHit>,
    /// One entry per registered provider, in registry order, always — including
    /// the ones that contributed nothing. An absent provider would be invisible;
    /// a reported one is a fact you can alert on.
    pub providers: Vec<ProviderReport>,
}

pub struct ScatterGather {
    providers: Vec<Arc<dyn SearchProvider>>,
    budget: Duration,
}

impl ScatterGather {
    pub fn new(providers: Vec<Arc<dyn SearchProvider>>, budget: Duration) -> Self {
        Self { providers, budget }
    }

    pub async fn search(&self, query: &str) -> SearchResponse {
        // --- scatter ---------------------------------------------------------
        // Nothing here knows how many providers there are, and nothing changes
        // if that number does. Note `.cloned()`: each branch owns its `Arc`, so
        // the futures are independent of each other and of this loop.
        let budget = self.budget;
        let branches = self.providers.iter().cloned().map(|provider| {
            let query = query.to_string();
            async move {
                let started = Instant::now();
                let outcome = timeout(budget, provider.search(&query)).await;
                let took_ms = started.elapsed().as_millis() as u64;
                let name = provider.name().to_string();

                match outcome {
                    Ok(Ok(hits)) => {
                        let report = ProviderReport {
                            name,
                            status: ProviderStatus::Ok,
                            took_ms,
                            hits: hits.len(),
                            error: None,
                        };
                        (report, hits)
                    }
                    // The branch failed. This is where a `?` would be fatal, so
                    // there isn't one — the error becomes a field in the answer.
                    Ok(Err(err)) => {
                        tracing::warn!(provider = %name, error = %err, "provider failed");
                        let report = ProviderReport {
                            name,
                            status: ProviderStatus::Failed,
                            took_ms,
                            hits: 0,
                            error: Some(err.to_string()),
                        };
                        (report, Vec::new())
                    }
                    // Dropping the timed-out future here is what abandons the
                    // in-flight request. The provider keeps working on it and
                    // will answer into the void; see partner-provider's log.
                    Err(_) => {
                        tracing::warn!(provider = %name, took_ms, "provider missed the budget");
                        let report = ProviderReport {
                            name,
                            status: ProviderStatus::TimedOut,
                            took_ms,
                            hits: 0,
                            error: Some(format!(
                                "did not answer within the {}ms budget",
                                budget.as_millis()
                            )),
                        };
                        (report, Vec::new())
                    }
                }
            }
        });

        // `join_all`, not `try_join_all`: wait for every branch to settle, and
        // let each settle however it likes. It also preserves input order, so
        // the `providers` report always reads in registry order no matter who
        // answered first.
        let settled = join_all(branches).await;

        // --- gather ----------------------------------------------------------
        // The providers have never heard of each other, so nobody but this loop
        // can notice that two of them returned the same product. Dedupe is by
        // lower-cased name, which is crude on purpose: catalog-provider and
        // archive-provider both list a "Coffee Mug" under *different* ids,
        // because no shared identity for a product exists anywhere in this
        // system. Fan-in always needs an identity story; this one is the
        // cheapest possible, and the README suggests replacing it.
        let mut merged: Vec<MergedHit> = Vec::new();
        let mut index_of: HashMap<String, usize> = HashMap::new();
        let mut reports: Vec<ProviderReport> = Vec::new();

        for (report, hits) in settled {
            for hit in hits {
                let key = hit.name.trim().to_lowercase();
                // `.copied()` so the lookup's borrow of `index_of` ends before
                // the `None` arm inserts into it.
                match index_of.get(&key).copied() {
                    Some(idx) => {
                        let existing = &mut merged[idx];
                        if !existing.sources.contains(&report.name) {
                            existing.sources.push(report.name.clone());
                        }
                        // Two prices for the same thing: keep the cheaper, and
                        // keep the id that goes with it. Another arbitrary
                        // policy the gather half is forced to have an opinion on.
                        if hit.price_cents < existing.price_cents {
                            existing.price_cents = hit.price_cents;
                            existing.id = hit.id;
                        }
                    }
                    None => {
                        index_of.insert(key, merged.len());
                        merged.push(MergedHit {
                            id: hit.id,
                            name: hit.name,
                            price_cents: hit.price_cents,
                            sources: vec![report.name.clone()],
                        });
                    }
                }
            }
            reports.push(report);
        }

        // Ranking is the gateway's job now, because ranking needs to see all the
        // results at once and no provider ever does. Cheapest first, ties broken
        // by name so the output is stable across runs.
        merged.sort_by(|a, b| {
            a.price_cents
                .cmp(&b.price_cents)
                .then_with(|| a.name.cmp(&b.name))
        });

        let degraded = reports.iter().any(|r| r.status != ProviderStatus::Ok);

        SearchResponse {
            query: query.to_string(),
            degraded,
            hits: merged,
            providers: reports,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use crate::provider::{Hit, ProviderError};

    enum Behaviour {
        Returns(Vec<Hit>),
        Fails(&'static str),
        Stalls(Duration),
    }

    struct FakeProvider {
        name: String,
        behaviour: Behaviour,
    }

    impl FakeProvider {
        fn returning(name: &str, hits: Vec<Hit>) -> Arc<dyn SearchProvider> {
            Arc::new(Self {
                name: name.into(),
                behaviour: Behaviour::Returns(hits),
            })
        }

        fn failing(name: &str, message: &'static str) -> Arc<dyn SearchProvider> {
            Arc::new(Self {
                name: name.into(),
                behaviour: Behaviour::Fails(message),
            })
        }

        fn stalling(name: &str, how_long: Duration) -> Arc<dyn SearchProvider> {
            Arc::new(Self {
                name: name.into(),
                behaviour: Behaviour::Stalls(how_long),
            })
        }
    }

    #[async_trait]
    impl SearchProvider for FakeProvider {
        fn name(&self) -> &str {
            &self.name
        }

        async fn search(&self, _query: &str) -> Result<Vec<Hit>, ProviderError> {
            match &self.behaviour {
                Behaviour::Returns(hits) => Ok(hits.clone()),
                Behaviour::Fails(message) => Err(ProviderError((*message).to_string())),
                Behaviour::Stalls(how_long) => {
                    tokio::time::sleep(*how_long).await;
                    Ok(Vec::new())
                }
            }
        }
    }

    fn hit(name: &str, price_cents: u64) -> Hit {
        Hit {
            id: Uuid::new_v4(),
            name: name.into(),
            price_cents,
        }
    }

    fn report<'a>(resp: &'a SearchResponse, name: &str) -> &'a ProviderReport {
        resp.providers
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no report for provider `{name}`"))
    }

    #[tokio::test]
    async fn every_provider_answering_is_not_degraded() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::returning("catalog", vec![hit("Coffee Mug", 1299)]),
                FakeProvider::returning("partner", vec![hit("Enamel Camping Mug", 2250)]),
            ],
            Duration::from_millis(100),
        );

        let resp = sg.search("mug").await;

        assert!(!resp.degraded);
        assert_eq!(resp.hits.len(), 2);
        assert_eq!(report(&resp, "catalog").status, ProviderStatus::Ok);
        assert_eq!(report(&resp, "partner").status, ProviderStatus::Ok);
    }

    /// The headline behaviour: a hung provider costs the request its results,
    /// not its life.
    #[tokio::test]
    async fn a_stalled_provider_is_dropped_and_the_rest_still_answer() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::returning("catalog", vec![hit("Coffee Mug", 1299)]),
                FakeProvider::stalling("partner", Duration::from_millis(400)),
            ],
            Duration::from_millis(50),
        );

        let resp = sg.search("mug").await;

        assert!(resp.degraded);
        assert_eq!(resp.hits.len(), 1, "catalog's hit must survive");
        assert_eq!(resp.hits[0].name, "Coffee Mug");
        assert_eq!(report(&resp, "partner").status, ProviderStatus::TimedOut);
        assert!(report(&resp, "partner").error.is_some());
    }

    #[tokio::test]
    async fn a_failing_provider_is_reported_rather_than_propagated() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::returning("catalog", vec![hit("Coffee Mug", 1299)]),
                FakeProvider::failing("archive", "the archive index is rebuilding"),
            ],
            Duration::from_millis(100),
        );

        let resp = sg.search("mug").await;

        assert!(resp.degraded);
        assert_eq!(resp.hits.len(), 1);
        let archive = report(&resp, "archive");
        assert_eq!(archive.status, ProviderStatus::Failed);
        assert_eq!(archive.hits, 0);
        assert!(archive
            .error
            .as_deref()
            .is_some_and(|e| e.contains("rebuilding")));
    }

    /// Nothing survives, and it is *still* a successful, honest response.
    #[tokio::test]
    async fn losing_every_provider_yields_an_empty_but_truthful_answer() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::failing("catalog", "down"),
                FakeProvider::stalling("partner", Duration::from_millis(400)),
            ],
            Duration::from_millis(50),
        );

        let resp = sg.search("mug").await;

        assert!(resp.degraded);
        assert!(resp.hits.is_empty());
        assert_eq!(resp.providers.len(), 2, "silent providers still get a report");
    }

    #[tokio::test]
    async fn the_same_product_from_two_providers_merges_keeping_the_cheaper_price() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::returning("catalog", vec![hit("Coffee Mug", 1299)]),
                FakeProvider::returning("archive", vec![hit("coffee mug", 999)]),
            ],
            Duration::from_millis(100),
        );

        let resp = sg.search("mug").await;

        assert_eq!(resp.hits.len(), 1, "the two mugs are one product");
        assert_eq!(resp.hits[0].price_cents, 999);
        assert_eq!(resp.hits[0].sources, vec!["catalog", "archive"]);
        // The per-provider counts are pre-merge, so they still say one hit
        // arrived from each — the report describes the fan-out, not the merged
        // answer.
        assert_eq!(report(&resp, "catalog").hits, 1);
        assert_eq!(report(&resp, "archive").hits, 1);
    }

    #[tokio::test]
    async fn results_are_ranked_across_providers_not_within_them() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::returning("catalog", vec![hit("Travel Mug", 1899)]),
                FakeProvider::returning("archive", vec![hit("Stoneware Mug", 1100)]),
            ],
            Duration::from_millis(100),
        );

        let resp = sg.search("mug").await;

        let names: Vec<_> = resp.hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["Stoneware Mug", "Travel Mug"]);
    }

    /// The budget is spent concurrently, not per branch: three 80ms providers
    /// behind a 300ms budget must not take 240ms.
    #[tokio::test]
    async fn branches_run_concurrently_rather_than_one_after_another() {
        let sg = ScatterGather::new(
            vec![
                FakeProvider::stalling("a", Duration::from_millis(80)),
                FakeProvider::stalling("b", Duration::from_millis(80)),
                FakeProvider::stalling("c", Duration::from_millis(80)),
            ],
            Duration::from_millis(300),
        );

        let started = Instant::now();
        let resp = sg.search("mug").await;
        let elapsed = started.elapsed();

        assert!(!resp.degraded, "all three fit inside the budget");
        assert!(
            elapsed < Duration::from_millis(200),
            "three concurrent 80ms branches took {elapsed:?} — that looks sequential"
        );
    }
}
