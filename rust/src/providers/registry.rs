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
    /// if two registrations share the same `ProviderId`.
    pub fn build<'a>(registrations: impl IntoIterator<Item = &'a ProviderRegistration>) -> Self {
        let mut by_id = HashMap::new();
        let mut order = Vec::new();
        for reg in registrations {
            let descriptor = (reg.descriptor)();
            let key = descriptor.id.as_str();
            if by_id.contains_key(key) {
                panic!("duplicate provider id registered: {key}");
            }
            order.push(key);
            by_id.insert(key, descriptor);
        }
        Self { by_id, order }
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
    fn global_registry_is_empty_in_phase_1() {
        // Phase 1 has zero `inventory::submit!` calls. This invariant moves
        // when phase 4 lands Claude. Update the constant then.
        assert_eq!(REGISTRY.len(), 0);
    }
}
