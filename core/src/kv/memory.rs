// SPDX-License-Identifier: MIT
// Copyright (c) 2024 Shane Utt

//! In-memory key-value store backend using [`DashMap`].
//!
//! Optimized for concurrent reads with lock-free lookups.
//! Writes are sharded across map segments to minimize
//! contention.
//!
//! [`DashMap`]: dashmap::DashMap

use std::sync::Arc;

use dashmap::DashMap;

use super::{KvBackend, MatchType};

// ---------------------------------------------------------------------------
// InMemoryKvBackend
// ---------------------------------------------------------------------------

/// Thread-safe in-memory key-value store.
///
/// Uses [`DashMap`] for concurrent access. Reads are
/// lock-free; writes shard across map segments.
///
/// ```
/// use std::sync::Arc;
///
/// use praxis_core::kv::{KvBackend, MatchType, memory::InMemoryKvBackend};
///
/// let store = InMemoryKvBackend::new();
/// store.set("color", Arc::from("blue"));
/// assert_eq!(store.get("color").as_deref(), Some("blue"));
/// ```
///
/// [`DashMap`]: dashmap::DashMap
#[derive(Debug)]
pub struct InMemoryKvBackend {
    /// Sharded concurrent hash map.
    data: DashMap<Arc<str>, Arc<str>>,
}

impl InMemoryKvBackend {
    /// Create an empty store.
    ///
    /// ```
    /// use praxis_core::kv::{KvBackend, memory::InMemoryKvBackend};
    ///
    /// let store = InMemoryKvBackend::new();
    /// assert!(store.is_empty());
    /// ```
    pub fn new() -> Self {
        Self { data: DashMap::new() }
    }

    /// Create a store pre-populated from key-value pairs.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use praxis_core::kv::{KvBackend, memory::InMemoryKvBackend};
    ///
    /// let store = InMemoryKvBackend::from_pairs(vec![("a".to_owned(), "1".to_owned())]);
    /// assert_eq!(store.len(), 1);
    /// ```
    pub fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        let data = DashMap::with_capacity(pairs.len());
        for (k, v) in pairs {
            data.insert(Arc::from(k.as_str()), Arc::from(v.as_str()));
        }
        Self { data }
    }
}

impl Default for InMemoryKvBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl KvBackend for InMemoryKvBackend {
    fn get(&self, key: &str) -> Option<Arc<str>> {
        self.data.get(key).map(|v| Arc::clone(v.value()))
    }

    fn set(&self, key: &str, value: Arc<str>) {
        self.data.insert(Arc::from(key), value);
    }

    fn delete(&self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    fn entries(&self) -> Vec<(Arc<str>, Arc<str>)> {
        self.data
            .iter()
            .map(|e| (Arc::clone(e.key()), Arc::clone(e.value())))
            .collect()
    }

    fn lookup(&self, pattern: &str, match_type: MatchType) -> Option<(Arc<str>, Arc<str>)> {
        match match_type {
            MatchType::Exact => self
                .data
                .get(pattern)
                .map(|e| (Arc::clone(e.key()), Arc::clone(e.value()))),
            MatchType::Prefix => self.data.iter().find_map(|e| {
                e.key()
                    .starts_with(pattern)
                    .then(|| (Arc::clone(e.key()), Arc::clone(e.value())))
            }),
            MatchType::Suffix => self.data.iter().find_map(|e| {
                e.key()
                    .ends_with(pattern)
                    .then(|| (Arc::clone(e.key()), Arc::clone(e.value())))
            }),
            MatchType::Regex => {
                let re = regex::Regex::new(pattern).ok()?;
                self.data.iter().find_map(|e| {
                    re.is_match(e.key())
                        .then(|| (Arc::clone(e.key()), Arc::clone(e.value())))
                })
            },
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn get_returns_none_for_missing_key() {
        let store = InMemoryKvBackend::new();
        assert!(store.get("missing").is_none(), "missing key should return None");
    }

    #[test]
    fn set_then_get_returns_value() {
        let store = InMemoryKvBackend::new();
        store.set("key", Arc::from("value"));
        assert_eq!(
            store.get("key").as_deref(),
            Some("value"),
            "set value should be retrievable"
        );
    }

    #[test]
    fn set_overwrites_existing() {
        let store = InMemoryKvBackend::new();
        store.set("key", Arc::from("v1"));
        store.set("key", Arc::from("v2"));
        assert_eq!(store.get("key").as_deref(), Some("v2"), "second set should overwrite");
    }

    #[test]
    fn delete_existing_returns_true() {
        let store = InMemoryKvBackend::new();
        store.set("key", Arc::from("val"));
        assert!(store.delete("key"), "deleting existing key should return true");
        assert!(store.get("key").is_none(), "deleted key should be gone");
    }

    #[test]
    fn delete_missing_returns_false() {
        let store = InMemoryKvBackend::new();
        assert!(!store.delete("missing"), "deleting missing key should return false");
    }

    #[test]
    fn len_and_is_empty() {
        let store = InMemoryKvBackend::new();
        assert!(store.is_empty(), "new store should be empty");
        assert_eq!(store.len(), 0, "new store length should be 0");

        store.set("a", Arc::from("1"));
        assert_eq!(store.len(), 1, "store should have 1 entry");
        assert!(!store.is_empty(), "store with entries should not be empty");
    }

    #[test]
    fn entries_returns_all_pairs() {
        let store = InMemoryKvBackend::new();
        store.set("a", Arc::from("1"));
        store.set("b", Arc::from("2"));

        let mut entries = store.entries();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(entries.len(), 2, "should have 2 entries");
        assert_eq!(entries[0].0.as_ref(), "a");
        assert_eq!(entries[0].1.as_ref(), "1");
        assert_eq!(entries[1].0.as_ref(), "b");
        assert_eq!(entries[1].1.as_ref(), "2");
    }

    #[test]
    fn from_pairs_populates_store() {
        let store = InMemoryKvBackend::from_pairs(vec![
            ("x".to_owned(), "10".to_owned()),
            ("y".to_owned(), "20".to_owned()),
        ]);
        assert_eq!(store.len(), 2, "from_pairs should populate 2 entries");
        assert_eq!(store.get("x").as_deref(), Some("10"));
        assert_eq!(store.get("y").as_deref(), Some("20"));
    }

    #[test]
    fn lookup_exact() {
        let store = InMemoryKvBackend::new();
        store.set("route.api", Arc::from("cluster_a"));

        let result = store.lookup("route.api", MatchType::Exact);
        assert_eq!(
            result.as_ref().map(|(_, v)| v.as_ref()),
            Some("cluster_a"),
            "exact lookup should find the key"
        );

        assert!(
            store.lookup("route.ap", MatchType::Exact).is_none(),
            "partial key should not match exact"
        );
    }

    #[test]
    fn lookup_prefix() {
        let store = InMemoryKvBackend::new();
        store.set("route.api.users", Arc::from("users_cluster"));
        store.set("route.web.home", Arc::from("web_cluster"));

        let result = store.lookup("route.api", MatchType::Prefix);
        assert!(result.is_some(), "prefix lookup should find matching key");
        assert_eq!(result.unwrap().1.as_ref(), "users_cluster");
    }

    #[test]
    fn lookup_suffix() {
        let store = InMemoryKvBackend::new();
        store.set("us-east.backend", Arc::from("east"));
        store.set("us-west.frontend", Arc::from("west"));

        let result = store.lookup(".backend", MatchType::Suffix);
        assert!(result.is_some(), "suffix lookup should find matching key");
        assert_eq!(result.unwrap().1.as_ref(), "east");
    }

    #[test]
    fn lookup_regex() {
        let store = InMemoryKvBackend::new();
        store.set("model-gpt4", Arc::from("openai"));
        store.set("model-claude", Arc::from("anthropic"));

        let result = store.lookup("model-gpt\\d", MatchType::Regex);
        assert!(result.is_some(), "regex lookup should find matching key");
        assert_eq!(result.unwrap().1.as_ref(), "openai");
    }

    #[test]
    fn lookup_regex_invalid_pattern_returns_none() {
        let store = InMemoryKvBackend::new();
        store.set("key", Arc::from("val"));
        assert!(
            store.lookup("[invalid", MatchType::Regex).is_none(),
            "invalid regex should return None"
        );
    }

    #[test]
    fn lookup_no_match_returns_none() {
        let store = InMemoryKvBackend::new();
        store.set("key", Arc::from("val"));
        assert!(store.lookup("other", MatchType::Prefix).is_none());
        assert!(store.lookup("other", MatchType::Suffix).is_none());
        assert!(store.lookup("other", MatchType::Regex).is_none());
    }

    #[test]
    fn concurrent_reads_and_writes() {
        let store = Arc::new(InMemoryKvBackend::new());
        let handles: Vec<_> = (0..100)
            .map(|i| {
                let s = Arc::clone(&store);
                std::thread::spawn(move || {
                    let key = format!("k{i}");
                    let val = Arc::from(format!("v{i}").as_str());
                    s.set(&key, val);
                    s.get(&key)
                })
            })
            .collect();

        for h in handles {
            assert!(h.join().unwrap().is_some(), "concurrent set+get should succeed");
        }
        assert_eq!(store.len(), 100, "all 100 entries should be present");
    }

    #[test]
    fn default_creates_empty_store() {
        let store = InMemoryKvBackend::default();
        assert!(store.is_empty(), "default store should be empty");
    }

    #[test]
    fn empty_string_key_and_value() {
        let store = InMemoryKvBackend::new();
        store.set("", Arc::from(""));
        assert_eq!(store.get("").as_deref(), Some(""), "empty key/value should work");
        assert_eq!(store.len(), 1, "empty key counts as an entry");
    }

    #[test]
    fn unicode_keys_and_values() {
        let store = InMemoryKvBackend::new();
        store.set("\u{6a21}\u{578b}", Arc::from("\u{30af}\u{30e9}\u{30b9}\u{30bf}"));
        assert_eq!(
            store.get("\u{6a21}\u{578b}").as_deref(),
            Some("\u{30af}\u{30e9}\u{30b9}\u{30bf}"),
            "unicode should roundtrip"
        );
    }

    #[test]
    fn lookup_exact_returns_none_on_empty_store() {
        let store = InMemoryKvBackend::new();
        assert!(store.lookup("any", MatchType::Exact).is_none());
        assert!(store.lookup("any", MatchType::Prefix).is_none());
        assert!(store.lookup("any", MatchType::Suffix).is_none());
        assert!(store.lookup("any", MatchType::Regex).is_none());
    }

    #[test]
    fn lookup_prefix_does_not_match_substring() {
        let store = InMemoryKvBackend::new();
        store.set("api.users", Arc::from("v1"));
        assert!(
            store.lookup("users", MatchType::Prefix).is_none(),
            "prefix should match start, not substring"
        );
    }

    #[test]
    fn lookup_suffix_does_not_match_substring() {
        let store = InMemoryKvBackend::new();
        store.set("api.users.list", Arc::from("v1"));
        assert!(
            store.lookup("users", MatchType::Suffix).is_none(),
            "suffix should match end, not substring"
        );
    }

    #[test]
    fn lookup_regex_anchored() {
        let store = InMemoryKvBackend::new();
        store.set("model-gpt4", Arc::from("openai"));
        let result = store.lookup("^model-", MatchType::Regex);
        assert!(result.is_some(), "anchored regex should match");
    }

    #[test]
    fn delete_then_lookup_returns_none() {
        let store = InMemoryKvBackend::new();
        store.set("temp", Arc::from("val"));
        store.delete("temp");
        assert!(
            store.lookup("temp", MatchType::Exact).is_none(),
            "deleted key should not match"
        );
    }

    #[test]
    fn from_pairs_empty_vec() {
        let store = InMemoryKvBackend::from_pairs(vec![]);
        assert!(store.is_empty(), "from_pairs with empty vec should be empty");
    }

    #[test]
    fn concurrent_deletes_are_safe() {
        let store = Arc::new(InMemoryKvBackend::new());
        for i in 0..50 {
            store.set(&format!("k{i}"), Arc::from("v"));
        }
        let handles: Vec<_> = (0..50)
            .map(|i| {
                let s = Arc::clone(&store);
                std::thread::spawn(move || s.delete(&format!("k{i}")))
            })
            .collect();
        let deleted: u32 = handles.into_iter().map(|h| u32::from(h.join().unwrap())).sum();
        assert_eq!(deleted, 50, "all 50 deletes should succeed");
        assert!(store.is_empty(), "store should be empty after all deletes");
    }

    #[test]
    fn concurrent_lookups_during_writes() {
        let store = Arc::new(InMemoryKvBackend::new());
        for i in 0..50 {
            store.set(&format!("route.{i}"), Arc::from(format!("cluster-{i}").as_str()));
        }
        let handles: Vec<_> = (0..100)
            .map(|i| {
                let s = Arc::clone(&store);
                std::thread::spawn(move || {
                    if i % 2 == 0 {
                        s.set(&format!("route.new.{i}"), Arc::from("new"));
                    }
                    s.lookup("route.", MatchType::Prefix)
                })
            })
            .collect();
        for h in handles {
            assert!(
                h.join().unwrap().is_some(),
                "lookup should find at least one prefix match"
            );
        }
    }
}
