//! The one port this gateway has, and the registry that fills it at startup.
//!
//! ## Why there is only one client here
//!
//! Compare with [`bff/services/web-bff/src/clients.rs`](../../../../bff/services/web-bff/src/clients.rs):
//! that gateway hand-writes three *different* traits — `OrdersClient`,
//! `UsersClient`, `CatalogClient` — because it aggregates three services that
//! answer three different questions. Its fan-out is **heterogeneous**, and its
//! shape is fixed at compile time: adding a fourth backend means a fourth
//! trait, a fourth field, and an edit to the aggregation.
//!
//! This gateway's fan-out is **homogeneous**. Every provider answers the same
//! question, so there is one trait and a `Vec` of it. Adding a provider is a
//! change to the `PROVIDERS` environment variable, not to any code — see
//! `parse_spec` below. That is the trade this lab is making visible: give up
//! bespoke per-branch contracts, get an open-ended set of branches.

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// One result from one provider, exactly as it arrives on the wire.
///
/// Note the absence of a `source` field: a provider never says who it is. The
/// gateway stamps that on from the registry name it dialled, which is the only
/// place that mapping is known.
#[derive(Debug, Clone, Deserialize)]
pub struct Hit {
    pub id: Uuid,
    pub name: String,
    pub price_cents: u64,
}

#[derive(Debug, Deserialize)]
struct ProviderResponse {
    hits: Vec<Hit>,
}

/// A branch failure. Deliberately *not* `AppError`: nothing in `scatter.rs` can
/// `?` one of these into a failed request, because a single dead provider must
/// never be able to fail the whole search. The type system enforces the policy.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ProviderError(pub String);

#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// The registry name — what this provider is called in the response's
    /// `providers` report. It is config, not something the provider tells us.
    fn name(&self) -> &str;

    async fn search(&self, query: &str) -> Result<Vec<Hit>, ProviderError>;
}

pub struct HttpSearchProvider {
    name: String,
    base_url: String,
    http: reqwest::Client,
}

impl HttpSearchProvider {
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SearchProvider for HttpSearchProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn search(&self, query: &str) -> Result<Vec<Hit>, ProviderError> {
        let url = format!("{}/search", self.base_url);
        let resp = self
            .http
            .get(&url)
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| ProviderError(format!("unreachable: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError(format!(
                "returned {status}: {}",
                body.trim()
            )));
        }

        let payload: ProviderResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError(format!("sent bad JSON: {e}")))?;
        Ok(payload.hits)
    }
}

/// Parse the `PROVIDERS` registry spec: `name=url,name=url,...`.
///
/// Kept as a pure function over strings so the interesting half — "what is the
/// fan-out actually made of?" — is unit-testable without opening a socket, the
/// same discipline the other labs apply to their ports.
pub fn parse_spec(spec: &str) -> Result<Vec<(String, String)>, String> {
    let mut providers = Vec::new();

    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (name, url) = entry
            .split_once('=')
            .ok_or_else(|| format!("expected `name=url`, got `{entry}`"))?;
        let (name, url) = (name.trim(), url.trim());

        if name.is_empty() || url.is_empty() {
            return Err(format!("expected `name=url`, got `{entry}`"));
        }
        if providers
            .iter()
            .any(|(existing, _): &(String, String)| existing.as_str() == name)
        {
            return Err(format!("provider `{name}` is registered twice"));
        }

        providers.push((name.to_string(), url.to_string()));
    }

    if providers.is_empty() {
        return Err("no providers registered".into());
    }

    Ok(providers)
}

/// Build the live fan-out from a registry spec. This is the only place in the
/// process that decides how wide the scatter is — `scatter.rs` just takes the
/// list it's given and never counts on its length.
pub fn from_spec(spec: &str) -> Result<Vec<Arc<dyn SearchProvider>>, String> {
    Ok(parse_spec(spec)?
        .into_iter()
        .map(|(name, url)| Arc::new(HttpSearchProvider::new(name, url)) as Arc<dyn SearchProvider>)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_multi_provider_registry() {
        let parsed = parse_spec("catalog=http://localhost:3011, partner=http://localhost:3012")
            .expect("valid spec");
        assert_eq!(
            parsed,
            vec![
                ("catalog".to_string(), "http://localhost:3011".to_string()),
                ("partner".to_string(), "http://localhost:3012".to_string()),
            ]
        );
    }

    #[test]
    fn a_single_provider_is_a_legitimate_fan_out_of_one() {
        let parsed = parse_spec("catalog=http://localhost:3011").expect("valid spec");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn rejects_entries_that_are_not_name_equals_url() {
        assert!(parse_spec("catalog").is_err());
        assert!(parse_spec("=http://localhost:3011").is_err());
        assert!(parse_spec("catalog=").is_err());
    }

    #[test]
    fn rejects_a_duplicate_name_rather_than_silently_scattering_twice() {
        let err = parse_spec("catalog=http://a,catalog=http://b").unwrap_err();
        assert!(err.contains("twice"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_an_empty_registry() {
        assert!(parse_spec("").is_err());
        assert!(parse_spec("  , ").is_err());
    }
}
