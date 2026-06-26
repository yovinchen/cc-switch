//! CC Switch ProviderRouter source assembly.

use crate::database::Database;
use crate::proxy::engine::routing::{ProviderRouter, ProviderRouterSources};
use crate::proxy::host::cc_switch::config_source::CcSwitchConfigSource;
use crate::proxy::host::cc_switch::database_channel_source::CcSwitchChannelSource;
use crate::proxy::host::cc_switch::provider_router_channel_source::CcSwitchProviderRouterChannelSource;
use crate::proxy::host::cc_switch::provider_router_config_source::CcSwitchProviderRouterConfigSource;
use crate::proxy::host::cc_switch::provider_router_health_store::CcSwitchProviderRouterHealthStore;
use crate::proxy::host::cc_switch::provider_router_provider_source::CcSwitchProviderRouterProviderSource;
use crate::proxy::host::cc_switch::route_policy_source::CcSwitchRoutePolicySource;
use std::sync::Arc;

pub(crate) struct CcSwitchProviderRouterSources;

impl CcSwitchProviderRouterSources {
    pub(crate) fn from_database(db: Arc<Database>) -> ProviderRouterSources {
        ProviderRouterSources::new(
            Arc::new(CcSwitchProviderRouterConfigSource::new(
                CcSwitchConfigSource::new(db.clone()),
            )),
            Arc::new(CcSwitchProviderRouterProviderSource::new(
                db.clone(),
                CcSwitchRoutePolicySource::new(db.clone()),
            )),
            Arc::new(CcSwitchProviderRouterChannelSource::new(
                CcSwitchChannelSource::new(db.clone()),
            )),
            Arc::new(CcSwitchProviderRouterHealthStore::new(db)),
        )
    }
}

pub(crate) fn provider_router_from_database(db: Arc<Database>) -> ProviderRouter {
    ProviderRouter::with_sources(CcSwitchProviderRouterSources::from_database(db))
}
