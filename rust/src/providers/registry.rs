//! `inventory!`-backed provider catalog. Each provider declares itself with
//! `inventory::submit!` (phase 4 onward), and the catalog collects them at
//! startup. Duplicate ids panic with a clear message.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use super::descriptor::ProviderDescriptor;
use crate::core::ProviderId;

pub struct ProviderRegistration {
    pub descriptor: fn() -> ProviderDescriptor,
}

inventory::collect!(ProviderRegistration);

/// In memory addressable catalog of registered providers.
#[derive(Debug, Default)]
pub struct ProviderCatalog {
    by_id: HashMap<&'static str, ProviderDescriptor>,
    order: Vec<&'static str>,
}

impl ProviderCatalog {
    /// Build the catalog from any iterator of `ProviderRegistration`. Panics
    /// if two registrations share the same `ProviderId`. Used at boot when
    /// a duplicate id is a programming error worth halting on.
    pub fn build<'a>(registrations: impl IntoIterator<Item = &'a ProviderRegistration>) -> Self {
        match Self::build_validated(registrations) {
            Ok(c) => c,
            Err(e) => panic!("{e}"),
        }
    }

    /// Same as `build`, but returns the duplicate-id error instead of
    /// panicking. Phase 4 P4-08 uses this from tests so it can assert on
    /// the error message without unwinding.
    pub fn build_validated<'a>(
        registrations: impl IntoIterator<Item = &'a ProviderRegistration>,
    ) -> Result<Self, super::errors::ProviderError> {
        let mut by_id = HashMap::new();
        let mut order = Vec::new();
        for reg in registrations {
            let descriptor = (reg.descriptor)();
            let key = descriptor.id.as_str();
            if by_id.contains_key(key) {
                return Err(super::errors::ProviderError::DuplicateId(key));
            }
            order.push(key);
            by_id.insert(key, descriptor);
        }
        Ok(Self { by_id, order })
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.order.iter().filter_map(|key| self.by_id.get(*key))
    }

    pub fn get(&self, id: ProviderId) -> Option<&ProviderDescriptor> {
        self.by_id.get(id.as_str())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Global catalog populated from every `inventory::submit!(ProviderRegistration { .. })`.
pub static REGISTRY: Lazy<ProviderCatalog> =
    Lazy::new(|| ProviderCatalog::build(inventory::iter::<ProviderRegistration>));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::branding::ProviderBranding;
    use crate::providers::descriptor::{ProviderFetchPlan, ProviderMetadata};

    fn fake_descriptor(id: &'static str) -> ProviderDescriptor {
        ProviderDescriptor {
            id: ProviderId(id),
            metadata: ProviderMetadata::minimal(id, "https://example.com"),
            branding: ProviderBranding::solid("#000000", id),
            cli: None,
            fetch_plan: ProviderFetchPlan::default(),
        }
    }

    fn fake_registration(id: &'static str) -> ProviderRegistration {
        // Hand build a registration that returns the descriptor for the id.
        // We cannot capture `id` in the `fn`, so each test uses a fixed id.
        match id {
            "claude" => ProviderRegistration {
                descriptor: || fake_descriptor("claude"),
            },
            "codex" => ProviderRegistration {
                descriptor: || fake_descriptor("codex"),
            },
            _ => panic!("test uses a fixed pair of ids"),
        }
    }

    #[test]
    fn empty_catalog_is_addressable() {
        let cat = ProviderCatalog::build(std::iter::empty());
        assert!(cat.is_empty());
        assert_eq!(cat.descriptors().count(), 0);
        assert!(cat.get(ProviderId("claude")).is_none());
    }

    #[test]
    fn catalog_preserves_registration_order() {
        let regs = [fake_registration("claude"), fake_registration("codex")];
        let cat = ProviderCatalog::build(regs.iter());
        let ids: Vec<&str> = cat.descriptors().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "codex"]);
    }

    #[test]
    #[should_panic(expected = "duplicate provider id")]
    fn duplicate_id_panics_with_clear_message() {
        let regs = [fake_registration("claude"), fake_registration("claude")];
        let _ = ProviderCatalog::build(regs.iter());
    }

    #[test]
    fn global_registry_includes_claude_after_phase_4() {
        // Phase 4 P4-10 registers Claude via `inventory::submit!`. Any
        // future provider lands as one more entry in this catalog.
        assert!(REGISTRY.get(ProviderId("claude")).is_some());
    }
}
