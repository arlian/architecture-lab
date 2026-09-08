//! archive-provider's private corpus: discontinued lines, kept only here.
//!
//! It overlaps with catalog-provider on purpose — "Coffee Mug" exists in both,
//! at a different price *and a different id*, because these two services have
//! never agreed on an identifier for anything. The gateway has to decide what
//! to do about that; see `search-gateway/src/scatter.rs`.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: Uuid,
    pub name: String,
    pub price_cents: u64,
}

pub struct Index {
    items: Vec<Hit>,
}

impl Index {
    pub fn seeded() -> Self {
        let items = [
            // Same product name as catalog-provider's, cheaper, different id.
            ("Coffee Mug", 999u64),
            ("Stoneware Mug (2019)", 1100),
            ("Pocket Notebook", 500),
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
        let names: Vec<_> = index.search("mug").into_iter().map(|h| h.name).collect();
        assert_eq!(names.len(), 2, "expected both archived mugs, got {names:?}");
    }

    #[test]
    fn carries_the_same_product_name_as_the_live_catalog_more_cheaply() {
        let index = Index::seeded();
        let mug = index
            .search("coffee mug")
            .into_iter()
            .next()
            .expect("archive should still list the Coffee Mug");
        assert!(
            mug.price_cents < 1299,
            "the archived mug is meant to undercut catalog-provider's 1299"
        );
    }
}
