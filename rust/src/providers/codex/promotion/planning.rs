//! Pure planner. No filesystem I/O, no clock, no RNG. Implements the
//! decision matrix from spec 41 §6.6: missing live -> None, unreadable
//! / API-key-only / identity-less live -> Reject, target identity
//! already matching live -> None, otherwise search readable managed
//! homes (provider id, then email) for a destination, then fall through
//! to persisted-but-not-readable homes (repair), then importNew.

use super::types::{
    ImportReason, LiveHomeState, NoneReason, PreparedIdentity, PreparedPromotionContext,
    PreparedStoredManagedAccount, PromotionPlan, RefreshReason, RejectReason, RepairReason,
    StoredHomeState,
};

pub fn plan(context: &PreparedPromotionContext) -> PromotionPlan {
    let target_identity = context.target.identity.as_ref();

    let displaced = match &context.live.home_state {
        LiveHomeState::Missing => {
            return PromotionPlan::None {
                reason: NoneReason::LiveMissing,
            };
        }
        LiveHomeState::Unreadable => {
            return PromotionPlan::Reject {
                reason: RejectReason::LiveUnreadable,
            };
        }
        LiveHomeState::ApiKeyOnly(_) => {
            return PromotionPlan::Reject {
                reason: RejectReason::LiveAPIKeyOnlyUnsupported,
            };
        }
        LiveHomeState::Readable(_) => match context.live.auth_identity.as_ref() {
            Some(id) => id,
            None => {
                return PromotionPlan::Reject {
                    reason: RejectReason::LiveIdentityMissingForPreservation,
                };
            }
        },
    };

    if let Some(target) = target_identity {
        if displaced.matches(target) {
            return PromotionPlan::None {
                reason: NoneReason::TargetMatchesLiveAuthIdentity,
            };
        }
    }

    // Search readable managed homes for a match against `displaced`.
    let provider_matches: Vec<&PreparedStoredManagedAccount> = context
        .other_managed
        .iter()
        .filter(|m| matches!(m.home_state, StoredHomeState::Readable(_)))
        .filter(|m| {
            m.identity
                .as_ref()
                .map(
                    |id| match (id.provider_id.as_deref(), displaced.provider_id.as_deref()) {
                        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a == b,
                        _ => false,
                    },
                )
                .unwrap_or(false)
        })
        .collect();
    if provider_matches.len() > 1 {
        return PromotionPlan::Reject {
            reason: RejectReason::ConflictingReadableManagedHome,
        };
    }
    if let Some(only) = provider_matches.into_iter().next() {
        return PromotionPlan::RefreshExisting {
            destination_id: only.account.id.clone(),
            reason: RefreshReason::ReadableHomeIdentityMatch,
        };
    }

    let email_matches: Vec<&PreparedStoredManagedAccount> = context
        .other_managed
        .iter()
        .filter(|m| matches!(m.home_state, StoredHomeState::Readable(_)))
        .filter(|m| {
            m.identity
                .as_ref()
                .map(
                    |id| match (id.email.as_deref(), displaced.email.as_deref()) {
                        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => {
                            a.eq_ignore_ascii_case(b)
                        }
                        _ => false,
                    },
                )
                .unwrap_or(false)
        })
        .collect();
    if email_matches.len() > 1 {
        return PromotionPlan::Reject {
            reason: RejectReason::ConflictingReadableManagedHome,
        };
    }
    if let Some(only) = email_matches.into_iter().next() {
        return PromotionPlan::RefreshExisting {
            destination_id: only.account.id.clone(),
            reason: RefreshReason::ReadableHomeIdentityMatchUsingPersistedEmailFallback,
        };
    }

    // Search persisted (non-readable) homes for a match.
    if let Some(repair) = find_persisted_match(context, displaced) {
        return repair;
    }

    PromotionPlan::ImportNew {
        reason: ImportReason::NoExistingManagedDestination,
    }
}

fn find_persisted_match(
    context: &PreparedPromotionContext,
    displaced: &PreparedIdentity,
) -> Option<PromotionPlan> {
    for entry in &context.other_managed {
        if matches!(entry.home_state, StoredHomeState::Readable(_)) {
            continue;
        }
        // Persisted match by provider id wins first.
        if let (Some(persisted), Some(want)) = (
            entry
                .account
                .provider_id
                .as_deref()
                .filter(|s| !s.is_empty()),
            displaced.provider_id.as_deref().filter(|s| !s.is_empty()),
        ) {
            if persisted == want {
                let reason = match entry.home_state {
                    StoredHomeState::Missing(_) => {
                        RepairReason::PersistedProviderMatchWithMissingHome
                    }
                    StoredHomeState::Unreadable(_) => {
                        RepairReason::PersistedProviderMatchWithUnreadableHome
                    }
                    StoredHomeState::Readable(_) => unreachable!(),
                };
                return Some(PromotionPlan::RepairExisting {
                    destination_id: entry.account.id.clone(),
                    reason,
                });
            }
        }
    }
    // Email-only legacy match: only consider it if no provider id was set.
    for entry in &context.other_managed {
        if matches!(entry.home_state, StoredHomeState::Readable(_)) {
            continue;
        }
        if entry.account.provider_id.is_some() {
            continue;
        }
        if let (Some(persisted), Some(want)) = (
            entry.account.email.as_deref().filter(|s| !s.is_empty()),
            displaced.email.as_deref().filter(|s| !s.is_empty()),
        ) {
            if persisted.eq_ignore_ascii_case(want) {
                return Some(PromotionPlan::RepairExisting {
                    destination_id: entry.account.id.clone(),
                    reason: RepairReason::PersistedLegacyEmailMatch,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::codex::promotion::types::{AuthMaterial, ManagedCodexAccount};
    use std::path::PathBuf;

    fn identity(provider_id: &str, email: &str) -> PreparedIdentity {
        PreparedIdentity {
            provider_id: if provider_id.is_empty() {
                None
            } else {
                Some(provider_id.into())
            },
            email: if email.is_empty() {
                None
            } else {
                Some(email.into())
            },
        }
    }

    fn target_with(
        id: &PreparedIdentity,
        home_state: StoredHomeState,
    ) -> PreparedStoredManagedAccount {
        PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "target".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/target"),
                provider_id: id.provider_id.clone(),
                email: id.email.clone(),
            },
            home_state,
            identity: Some(id.clone()),
        }
    }

    fn material() -> AuthMaterial {
        AuthMaterial {
            bytes: vec![1, 2, 3],
        }
    }

    fn ctx(
        live: LiveHomeState,
        live_identity: Option<PreparedIdentity>,
    ) -> PreparedPromotionContext {
        PreparedPromotionContext {
            target: target_with(
                &identity("codex:target", "target@example.com"),
                StoredHomeState::Readable(material()),
            ),
            other_managed: Vec::new(),
            live: PreparedLiveAccount {
                home_state: live,
                auth_identity: live_identity,
                snapshot_account_identity: None,
            },
        }
    }

    use super::super::types::PreparedLiveAccount;

    #[test]
    fn live_missing_yields_none_live_missing() {
        let p = plan(&ctx(LiveHomeState::Missing, None));
        assert!(matches!(
            p,
            PromotionPlan::None {
                reason: NoneReason::LiveMissing
            }
        ));
    }

    #[test]
    fn live_unreadable_yields_reject_live_unreadable() {
        let p = plan(&ctx(LiveHomeState::Unreadable, None));
        assert!(matches!(
            p,
            PromotionPlan::Reject {
                reason: RejectReason::LiveUnreadable
            }
        ));
    }

    #[test]
    fn live_api_key_only_yields_reject() {
        let p = plan(&ctx(LiveHomeState::ApiKeyOnly(material()), None));
        assert!(matches!(
            p,
            PromotionPlan::Reject {
                reason: RejectReason::LiveAPIKeyOnlyUnsupported
            }
        ));
    }

    #[test]
    fn live_readable_without_identity_yields_reject() {
        let p = plan(&ctx(LiveHomeState::Readable(material()), None));
        assert!(matches!(
            p,
            PromotionPlan::Reject {
                reason: RejectReason::LiveIdentityMissingForPreservation
            }
        ));
    }

    #[test]
    fn live_matches_target_yields_none() {
        let live_id = identity("codex:target", "target@example.com");
        let p = plan(&ctx(LiveHomeState::Readable(material()), Some(live_id)));
        assert!(matches!(
            p,
            PromotionPlan::None {
                reason: NoneReason::TargetMatchesLiveAuthIdentity
            }
        ));
    }

    #[test]
    fn live_distinct_with_no_managed_destinations_imports_new() {
        let live_id = identity("codex:other", "other@example.com");
        let p = plan(&ctx(LiveHomeState::Readable(material()), Some(live_id)));
        assert!(matches!(
            p,
            PromotionPlan::ImportNew {
                reason: ImportReason::NoExistingManagedDestination
            }
        ));
    }

    #[test]
    fn live_distinct_matching_readable_managed_refreshes() {
        let live_id = identity("codex:displaced", "displaced@example.com");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id.clone()));
        context.other_managed.push(PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "managed-1".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/m1"),
                provider_id: Some("codex:displaced".into()),
                email: None,
            },
            home_state: StoredHomeState::Readable(material()),
            identity: Some(live_id),
        });
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::RefreshExisting {
                destination_id,
                reason: RefreshReason::ReadableHomeIdentityMatch,
            } if destination_id == "managed-1"
        ));
    }

    #[test]
    fn multiple_readable_managed_matches_yield_conflict_reject() {
        let live_id = identity("codex:displaced", "displaced@example.com");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id.clone()));
        for n in 1..=2 {
            context.other_managed.push(PreparedStoredManagedAccount {
                account: ManagedCodexAccount {
                    id: format!("managed-{n}"),
                    created_at_unix_secs: 0,
                    display_name: None,
                    home: PathBuf::from(format!("/managed/m{n}")),
                    provider_id: Some("codex:displaced".into()),
                    email: None,
                },
                home_state: StoredHomeState::Readable(material()),
                identity: Some(live_id.clone()),
            });
        }
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::Reject {
                reason: RejectReason::ConflictingReadableManagedHome
            }
        ));
    }

    #[test]
    fn email_fallback_match_uses_email_refresh_reason() {
        let live_id = identity("", "shared@example.com");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id.clone()));
        context.other_managed.push(PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "managed-1".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/m1"),
                provider_id: None,
                email: Some("shared@example.com".into()),
            },
            home_state: StoredHomeState::Readable(material()),
            identity: Some(live_id),
        });
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::RefreshExisting {
                destination_id,
                reason: RefreshReason::ReadableHomeIdentityMatchUsingPersistedEmailFallback,
            } if destination_id == "managed-1"
        ));
    }

    #[test]
    fn persisted_provider_match_with_missing_home_repairs() {
        let live_id = identity("codex:displaced", "");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id));
        context.other_managed.push(PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "managed-1".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/m1"),
                provider_id: Some("codex:displaced".into()),
                email: None,
            },
            home_state: StoredHomeState::Missing(PathBuf::from("/managed/m1")),
            identity: None,
        });
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::RepairExisting {
                destination_id,
                reason: RepairReason::PersistedProviderMatchWithMissingHome,
            } if destination_id == "managed-1"
        ));
    }

    #[test]
    fn persisted_provider_match_with_unreadable_home_repairs() {
        let live_id = identity("codex:displaced", "");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id));
        context.other_managed.push(PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "managed-1".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/m1"),
                provider_id: Some("codex:displaced".into()),
                email: None,
            },
            home_state: StoredHomeState::Unreadable(PathBuf::from("/managed/m1")),
            identity: None,
        });
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::RepairExisting {
                destination_id,
                reason: RepairReason::PersistedProviderMatchWithUnreadableHome,
            } if destination_id == "managed-1"
        ));
    }

    #[test]
    fn persisted_legacy_email_match_repairs() {
        let live_id = identity("", "legacy@example.com");
        let mut context = ctx(LiveHomeState::Readable(material()), Some(live_id));
        context.other_managed.push(PreparedStoredManagedAccount {
            account: ManagedCodexAccount {
                id: "legacy-1".into(),
                created_at_unix_secs: 0,
                display_name: None,
                home: PathBuf::from("/managed/legacy"),
                provider_id: None,
                email: Some("legacy@example.com".into()),
            },
            home_state: StoredHomeState::Missing(PathBuf::from("/managed/legacy")),
            identity: None,
        });
        let p = plan(&context);
        assert!(matches!(
            p,
            PromotionPlan::RepairExisting {
                destination_id,
                reason: RepairReason::PersistedLegacyEmailMatch,
            } if destination_id == "legacy-1"
        ));
    }
}
