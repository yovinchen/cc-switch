//! CC Switch request-context provider hydration source.

use crate::app_config::AppType;
use crate::database::Database;
use crate::provider::Provider;
use crate::proxy::engine::context::RequestContextProviderSource;
use crate::proxy::error::ProxyError;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct CcSwitchRequestContextProviderSource {
    db: Arc<Database>,
}

impl CcSwitchRequestContextProviderSource {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl RequestContextProviderSource for CcSwitchRequestContextProviderSource {
    fn source_name(&self) -> &'static str {
        "host database"
    }

    fn load_provider(
        &self,
        app_type: &AppType,
        provider_id: &str,
    ) -> Result<Option<Provider>, ProxyError> {
        self.db
            .get_provider_by_id(provider_id, app_type.as_str())
            .map_err(|error| ProxyError::DatabaseError(error.to_string()))
    }
}
