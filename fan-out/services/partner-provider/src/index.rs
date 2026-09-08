//! partner-provider's private corpus: stock listed by a third-party
//! marketplace, none of which the shop holds itself.
//!
//! Byte-for-byte the same shape as catalog-provider's index — that sameness is
//! the contract. The interesting difference is not in this file at all; it's in
//! `http.rs`, where every response is deliberately slow.

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
            ("Enamel Camping Mug", 2250u64),
            ("Mug Warmer (USB)", 1650),
            ("Leather Notebook", 4200),
            ("Fountain Pen", 5900),
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
        assert_eq!(names.len(), 2, "expected both mug listings, got {names:?}");
    }

    #[test]
    fn misses_return_nothing_rather_than_erroring() {
        let index = Index::seeded();
        assert!(index.search("bicycle").is_empty());
    }
}
