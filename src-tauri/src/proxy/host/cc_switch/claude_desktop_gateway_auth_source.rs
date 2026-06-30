//! CC Switch Claude Desktop gateway auth source.

use crate::database::Database;
use crate::error::AppError;
use crate::proxy_core::api::auth::claude_desktop_gateway_token_error;
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::ClaudeDesktopGatewayAuthSource;
use futures::future::BoxFuture;
use std::sync::Arc;

pub(crate) const CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY: &str = "claude_desktop_gateway_token";

pub(crate) fn claude_desktop_gateway_token_configured_from_db_source(db: &Database) -> bool {
    db.get_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY)
        .ok()
        .flatten()
        .is_some_and(|token| !token.trim().is_empty())
}

pub(crate) fn get_or_create_claude_desktop_gateway_token_from_db_source(
    db: &Database,
) -> Result<String, AppError> {
    if let Some(token) = db.get_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY)? {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let token = format!("ccs-{}", uuid::Uuid::new_v4().simple());
    db.set_setting(CLAUDE_DESKTOP_GATEWAY_TOKEN_SETTING_KEY, &token)?;
    Ok(token)
}

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
