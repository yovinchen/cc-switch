pub fn split_endpoint_and_query(endpoint: &str) -> (&str, Option<&str>) {
    endpoint
        .split_once('?')
        .map_or((endpoint, None), |(path, query)| (path, Some(query)))
}

pub fn strip_beta_query(query: Option<&str>) -> Option<String> {
    let filtered = query.map(|query| {
        query
            .split('&')
            .filter(|pair| !pair.is_empty() && !pair.starts_with("beta="))
            .collect::<Vec<_>>()
            .join("&")
    });

    match filtered.as_deref() {
        Some("") | None => None,
        Some(_) => filtered,
    }
}

pub fn merge_query_params(base_query: Option<&str>, extra_param: Option<&str>) -> Option<String> {
    let mut params: Vec<String> = base_query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .filter(|pair| !pair.starts_with("alt="))
        .map(ToString::to_string)
        .collect();

    if let Some(extra_param) = extra_param {
        params.push(extra_param.to_string());
    }

    if params.is_empty() {
        None
    } else {
        Some(params.join("&"))
    }
}

pub fn append_query_to_full_url(base_url: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => {
            if base_url.contains('?') {
                format!("{base_url}&{query}")
            } else {
                format!("{base_url}?{query}")
            }
        }
        _ => base_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_query_to_full_url, merge_query_params, split_endpoint_and_query, strip_beta_query,
    };

    #[test]
    fn split_endpoint_and_query_separates_first_query_marker() {
        assert_eq!(
            split_endpoint_and_query("/v1/messages?beta=true&x-id=1"),
            ("/v1/messages", Some("beta=true&x-id=1"))
        );
        assert_eq!(split_endpoint_and_query("/v1/messages"), ("/v1/messages", None));
    }

    #[test]
    fn strip_beta_query_removes_beta_pairs_and_empty_segments() {
        assert_eq!(
            strip_beta_query(Some("beta=true&x-id=1&&foo=bar")),
            Some("x-id=1&foo=bar".to_string())
        );
        assert_eq!(strip_beta_query(Some("beta=true&&")), None);
        assert_eq!(strip_beta_query(None), None);
    }

    #[test]
    fn merge_query_params_replaces_alt_param_with_extra_param() {
        assert_eq!(
            merge_query_params(Some("x-id=1&alt=json&foo=bar"), Some("alt=sse")),
            Some("x-id=1&foo=bar&alt=sse".to_string())
        );
        assert_eq!(merge_query_params(Some("alt=json"), None), None);
        assert_eq!(merge_query_params(None, Some("alt=sse")), Some("alt=sse".to_string()));
    }

    #[test]
    fn append_query_to_full_url_preserves_existing_query_string() {
        assert_eq!(
            append_query_to_full_url("https://relay.example/api?foo=bar", Some("x-id=1")),
            "https://relay.example/api?foo=bar&x-id=1"
        );
        assert_eq!(
            append_query_to_full_url("https://relay.example/api", Some("x-id=1")),
            "https://relay.example/api?x-id=1"
        );
        assert_eq!(
            append_query_to_full_url("https://relay.example/api", None),
            "https://relay.example/api"
        );
    }
}
