use crate::app_config::AppType;
use crate::provider::Provider;
use crate::proxy::route_attempt::ForwardAttempt;
use crate::proxy_core::api::auth::channel_auth_profile_missing_key_error;
use crate::proxy_core::api::domain::{
    channel_auth_profile_provider_application, ChannelAuthProfileProviderApplication,
};
use crate::proxy_core::api::errors::ProxyCoreResult;
use crate::proxy_core::api::ports::ChannelKeyRuntimeSource;
use crate::proxy_core_adapter::{
    provider_with_channel_auth_key, route_plan_no_matching_host_providers_error,
    route_plan_provider_match, route_plan_providers_unconfigured_error, RoutePlan,
};
use indexmap::IndexMap;

pub(crate) fn host_providers_for_plan(
    providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<Provider>> {
    let provider_match = route_plan_provider_match(plan, providers.keys().map(String::as_str));
    let has_matches = provider_match.has_matches();
    let matching: Vec<_> = provider_match
        .matched_provider_ids
        .iter()
        .filter_map(|provider_id| providers.get(provider_id.as_str()).cloned())
        .collect();
    if !has_matches {
        return Err(route_plan_providers_unconfigured_error());
    }
    Ok(matching)
}

pub(crate) fn forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> Vec<ForwardAttempt> {
    crate::proxy::route_attempt::forward_attempts_from_route_plan(app_type, providers, plan)
}

pub(crate) fn required_forward_attempts_from_plan(
    app_type: &AppType,
    providers: &[Provider],
    plan: &RoutePlan,
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let attempts = forward_attempts_from_plan(app_type, providers, plan);
    if attempts.is_empty() {
        return Err(route_plan_no_matching_host_providers_error());
    }
    Ok(attempts)
}

pub(crate) fn apply_channel_auth_profile_providers_from_source(
    app_type: &AppType,
    providers: &IndexMap<String, Provider>,
    attempts: &mut [ForwardAttempt],
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
) -> ProxyCoreResult<()> {
    for attempt in attempts {
        let auth_profile_ref = attempt
            .channel()
            .and_then(|channel| channel.auth_profile_ref.as_ref())
            .map(String::as_str);
        let channel_id = attempt.channel().map(|channel| channel.channel_id.as_str());
        match channel_auth_profile_provider_application(
            app_type.as_str(),
            auth_profile_ref,
            channel_id,
            |provider_id| providers.contains_key(provider_id),
        ) {
            ChannelAuthProfileProviderApplication::UseProvider { provider_id } => {
                let provider = providers
                    .get(&provider_id)
                    .expect("provider availability was checked by proxy-core")
                    .clone();
                attempt.set_auth_provider(provider);
            }
            ChannelAuthProfileProviderApplication::MissingProvider { warning } => {
                log::warn!("{warning}");
                continue;
            }
            ChannelAuthProfileProviderApplication::UseChannelKey {
                channel_id,
                key_ref,
            } => {
                let Some(key_candidate) =
                    channel_key_runtime_source.load_channel_key_candidate(&channel_id, &key_ref)?
                else {
                    return Err(channel_auth_profile_missing_key_error(
                        &channel_id,
                        &key_ref,
                    ));
                };
                attempt.set_auth_provider(provider_with_channel_auth_key(
                    app_type,
                    attempt.provider(),
                    &key_candidate.key_value,
                ));
            }
            ChannelAuthProfileProviderApplication::Ignore => {
                continue;
            }
        }
    }
    Ok(())
}

pub(crate) fn required_forward_attempts_from_sources(
    app_type: &AppType,
    all_providers: &IndexMap<String, Provider>,
    plan: &RoutePlan,
    channel_key_runtime_source: &(dyn ChannelKeyRuntimeSource + Send + Sync),
) -> ProxyCoreResult<Vec<ForwardAttempt>> {
    let providers = host_providers_for_plan(all_providers, plan)?;
    let mut attempts = required_forward_attempts_from_plan(app_type, &providers, plan)?;
    apply_channel_auth_profile_providers_from_source(
        app_type,
        all_providers,
        &mut attempts,
        channel_key_runtime_source,
    )?;
    Ok(attempts)
}
