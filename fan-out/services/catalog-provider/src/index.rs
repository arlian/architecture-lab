//! catalog-provider's private corpus: the products the shop sells itself.
//!
//! Every provider in this lab owns its own index and never shares it. Note what
//! a `Hit` does *not* carry: the provider's own name. The gateway stamps that on
//! from its registry, so a provider needs no idea what it's registered as, or
//! that anything is fanning out to it at all.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: Uuid,
    pub name: String,
    /// Price in the smallest currency unit (e.g. cents), same convention as
    /// every other lab in this repo.
    pub price_cents: u64,
}

pub struct Index {
    items: Vec<Hit>,
}

impl Index {
    /// The in-house range. Deliberately overlaps with archive-provider's corpus
    /// ("Coffee Mug" appears in both, at different prices and with a different
    /// id) so the gateway's fan-in has something real to reconcile.
    pub fn seeded() -> Self {
        let items = [
            ("Coffee Mug", 1299u64),
            ("Travel Mug", 1899),
            ("Notebook", 850),
            ("Desk Lamp", 3400),
        ]
        .into_iter()
        .map(|(name, price_cents)| Hit {
            id: Uuid::new_v4(),
            name: name.to_string(),
            price_cents,
        })
        .collect();

        Self { items }
    }

    /// Deliberately the dumbest possible matcher: a case-insensitive substring
    /// scan. Relevance scoring is not what this lab is about — what happens to
    /// these hits *after* they leave the process is.
    pub fn search(&self, query: &str) -> Vec<Hit> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.items
            .iter()
            .filter(|hit| hit.name.to_lowercase().contains(&needle))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_case_insensitively_on_a_substring() {
        let index = Index::seeded();
        let names: Vec<_> = index.search("MUG").into_iter().map(|h| h.name).collect();
        assert_eq!(names.len(), 2, "expected both mugs, got {names:?}");
        assert!(names.contains(&"Coffee Mug".to_string()));
        assert!(names.contains(&"Travel Mug".to_string()));
    }

    #[test]
    fn misses_return_nothing_rather_than_erroring() {
        let index = Index::seeded();
        assert!(index.search("bicycle").is_empty());
    }

    #[test]
    fn a_blank_query_matches_nothing() {
        let index = Index::seeded();
        assert!(index.search("   ").is_empty());
    }
}
