//! Consolidate the Cursor strategies into the ordered list the
//! framework runtime walks. Phase 6.5 only ships the web strategy; the
//! cursor CLI integration lands in a follow-up because the existing
//! Codex/Claude PTY wiring is the next chunk of work to factor out.

use std::sync::Arc;

use crate::providers::claude::web::strategy::{CookieResolver, WebClient};
use crate::providers::cursor::web::strategy::CursorWebStrategy;
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct CursorWiring {
    pub web_client: Arc<dyn WebClient>,
    pub web_cookies: Arc<dyn CookieResolver>,
}

impl CursorWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(CursorWebStrategy::new(
            self.web_client,
            self.web_cookies,
        )) as Arc<dyn Strategy>]
    }
}
