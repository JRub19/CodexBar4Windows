//! Storage footprint per spec 30 section 12.7. Some providers track
//! storage allotment alongside compute (Cursor, Factory). The icon does
//! not surface this directly; the popup chart card consumes it.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderStorageFootprint {
    pub used_bytes: u64,
    pub allotted_bytes: Option<u64>,
}

impl ProviderStorageFootprint {
    pub fn remaining_percent(&self) -> f32 {
        let Some(total) = self.allotted_bytes else {
            return 100.0;
        };
        if total == 0 {
            return 100.0;
        }
        let remaining = total.saturating_sub(self.used_bytes) as f64;
        let pct = (remaining / total as f64) * 100.0;
        pct.clamp(0.0, 100.0) as f32
    }
}
