//! CC Switch route policy source.

use crate::database::Database;
use crate::proxy_core::api::domain::AppKind;
use crate::proxy_core::api::errors::{config_error_with_context, ProxyCoreResult};
use crate::proxy_core::api::ports::RoutePolicySource;
use crate::proxy_core::api::routing::{route_policy_from_failover_provider_ids, RoutePolicy};
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchRoutePolicySource {
    db: Arc<Database>,
}

impl CcSwitchRoutePolicySource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl RoutePolicySource for CcSwitchRoutePolicySource {
    fn load_policy<'a>(
        &'a self,
        app: &'a AppKind,
    ) -> BoxFuture<'a, ProxyCoreResult<Option<RoutePolicy>>> {
        Box::pin(async move {
            let queue = self
                .db
                .get_failover_queue(app.as_str())
                .map_err(|error| config_error_with_context("load route policy", error))?;
            Ok(Some(route_policy_from_failover_provider_ids(
                app.clone(),
                queue.into_iter().map(|item| item.provider_id),
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::proxy_core::api::routing::DEFAULT_ROUTE_GROUP;
    use serde_json::json;

    fn save_claude_provider(db: &Database) {
        let provider = Provider::with_id(
            "anthropic-main".to_string(),
            "Anthropic Main".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://relay-a.example.com/v1/",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }
            }),
            None,
        );
        db.save_provider("claude", &provider)
            .expect("save provider");
    }

    #[tokio::test]
    async fn route_policy_source_projects_failover_queue_through_core() {
        let db = Arc::new(Database::memory().expect("memory db"));
        save_claude_provider(&db);
        db.add_to_failover_queue("claude", "anthropic-main")
            .expect("add failover provider");
        let source = CcSwitchRoutePolicySource::new(db);

        let policy = source
            .load_policy(&AppKind::Claude)
            .await
            .expect("load policy")
            .expect("policy");

        assert_eq!(policy.app, AppKind::Claude);
        assert!(policy.groups.is_empty());
        assert_eq!(policy.raw["defaultGroup"], json!(DEFAULT_ROUTE_GROUP));
        assert_eq!(policy.raw["failoverProviderIds"], json!(["anthropic-main"]));
    }
}
