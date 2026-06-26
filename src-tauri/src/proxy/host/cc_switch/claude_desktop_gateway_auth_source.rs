//! CC Switch Claude Desktop gateway auth source.

use crate::database::Database;
use crate::proxy_core_adapter::{
    claude_desktop_gateway_token_error, get_or_create_claude_desktop_gateway_token_from_db_source,
    ClaudeDesktopGatewayAuthSource, ProxyCoreResult,
};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchClaudeDesktopGatewayAuthSource {
    db: Arc<Database>,
}

impl CcSwitchClaudeDesktopGatewayAuthSource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl ClaudeDesktopGatewayAuthSource for CcSwitchClaudeDesktopGatewayAuthSource {
    fn load_gateway_token<'a>(&'a self) -> BoxFuture<'a, ProxyCoreResult<String>> {
        Box::pin(async move {
            get_or_create_claude_desktop_gateway_token_from_db_source(self.db.as_ref())
                .map_err(claude_desktop_gateway_token_error)
        })
    }
}
