//! Federation discovery: fetch a list of known federations from an external
//! index.
//!
//! The fetch itself is platform-specific:
//! - **Native** (iOS, Android, desktop): [`reqwest`], an async HTTP client
//!   already present in the workspace dependency tree.
//! - **WASM** (browser): [`gloo_net`], a thin wrapper around the browser's
//!   built-in `fetch()` API, with effectively zero binary-size cost.
//!
//! Both paths parse the same JSON schema into [`DiscoveredFederation`] via
//! the crate-private [`RawDiscoveredFederation`](crate::types::discovery::RawDiscoveredFederation)
//! intermediate.

use crate::types::discovery::RawDiscoveredFederation;
use crate::{DEFAULT_DISCOVERY_URL, DiscoveredFederation, Error, ErrorCode, Result};

/// Fetches a list of federations from the given URL, or from the default
/// observer API when `url` is `None`.
///
/// # Errors
///
/// [`ErrorCode::Internal`] when the HTTP request fails (DNS,
/// TLS, timeout, non-200 status).
/// [`ErrorCode::InvalidInput`] when the response body cannot be parsed as JSON.
pub async fn fetch_discovered_federations(
    url: Option<String>,
) -> Result<Vec<DiscoveredFederation>> {
    let url = url.as_deref().unwrap_or(DEFAULT_DISCOVERY_URL);
    validate_discovery_url(url)?;

    let body = fetch_body(url).await?;
    parse_discovery_response(&body)
}

fn validate_discovery_url(url: &str) -> Result<()> {
    let parsed_url = url::Url::parse(url).map_err(|e| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("Observer URL is structurally invalid: {e}"),
        )
    })?;

    if parsed_url.scheme() == "http" {
        let host = parsed_url.host_str().unwrap_or("");
        let is_local_or_onion = host == "127.0.0.1" || host == "localhost" || host.ends_with(".onion");

        if !is_local_or_onion {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "Discovery URLs must use HTTPS, unless connecting to localhost or a .onion address.",
            ));
        }
    } else if parsed_url.scheme() != "https" {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Discovery URLs must use the https:// or http:// scheme.",
        ));
    }

    Ok(())
}

fn parse_discovery_response(body: &str) -> Result<Vec<DiscoveredFederation>> {
    if body.len() > 1_000_000 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Observer parse error: API payload exceeded 1MB limit",
        ));
    }

    let raw_values: Vec<serde_json::Value> = serde_json::from_str(body).map_err(|e| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("Observer parse error: response is not a JSON array: {e}"),
        )
    })?;

    let federations: Vec<DiscoveredFederation> = raw_values
        .into_iter()
        .filter_map(|val| serde_json::from_value::<RawDiscoveredFederation>(val).ok())
        .filter_map(|r| r.try_into().ok())
        .collect();

    Ok(federations)
}

// ---------------------------------------------------------------------------
// Platform-specific fetch implementations
// ---------------------------------------------------------------------------

/// Native fetch: uses `reqwest` since it is already inside the dependency tree.
#[cfg(not(target_family = "wasm"))]
async fn fetch_body(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("fedimint-sdk/discovery")
        .build()
        .map_err(|e| {
            Error::new(
                ErrorCode::Internal,
                format!("Observer API error: failed to build HTTP client: {e}"),
            )
        })?;

    let response = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            Error::new(ErrorCode::Timeout, "Observer API error: request timed out after 30s")
        } else {
            Error::new(
                ErrorCode::Internal,
                format!("Observer API error: HTTP request failed: {e}"),
            )
        }
    })?;

    if !response.status().is_success() {
        return Err(Error::new(
            ErrorCode::Internal,
            format!("Observer API error: endpoint returned HTTP {}", response.status()),
        ));
    }

    response.text().await.map_err(|e| {
        Error::new(
            ErrorCode::Internal,
            format!("Observer API error: failed to read response body: {e}"),
        )
    })
}

/// WASM fetch: uses `gloo_net` which delegates to the browser's `fetch()` API.
#[cfg(target_family = "wasm")]
async fn fetch_body(url: &str) -> Result<String> {
    use gloo_net::http::Request;
    use web_sys::AbortController;

    let controller = AbortController::new().map_err(|_| {
        Error::new(ErrorCode::Internal, "failed to create AbortController")
    })?;
    let signal = controller.signal();

    let request = Request::get(url).abort_signal(Some(&signal));

    let fetch_future = async {
        let response = request.send().await.map_err(|e| {
            Error::new(
                ErrorCode::Internal,
                format!("Observer API error: HTTP request failed: {e}"),
            )
        })?;

        if !response.ok() {
            return Err(Error::new(
                ErrorCode::Internal,
                format!("Observer API error: endpoint returned HTTP {}", response.status()),
            ));
        }

        response.text().await.map_err(|e| {
            Error::new(
                ErrorCode::Internal,
                format!("Observer API error: failed to read response body: {e}"),
            )
        })
    };

    let (sender, receiver) = futures::channel::oneshot::channel();
    let timeout_id = gloo_timers::callback::Timeout::new(30_000, move || {
        let _ = sender.send(());
    });

    let result = tokio::select! {
        res = fetch_future => res,
        _ = receiver => {
            controller.abort();
            Err(Error::new(ErrorCode::Timeout, "Observer API error: request timed out after 30s"))
        }
    };

    timeout_id.cancel();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_discovery_response_valid() {
        let json = r#"[
            {
                "id": "fed11qgq",
                "name": "Test Fed",
                "deposits": 1000,
                "health": "online"
            }
        ]"#;
        let feds = parse_discovery_response(json).expect("should parse");
        assert_eq!(feds.len(), 1);
        assert_eq!(feds[0].id, "fed11qgq");
        assert_eq!(feds[0].deposits, Some(1000));
    }

    #[test]
    fn test_parse_discovery_response_filters_missing_id() {
        let json = r#"[ { "invalid": true } ]"#;
        let feds = parse_discovery_response(json).expect("should parse");
        assert_eq!(feds.len(), 0);
    }

    #[test]
    fn test_parse_discovery_payload_exceeds_1mb() {
        let large_body = " ".repeat(1_000_001);
        let res = parse_discovery_response(&large_body);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_discovery_url("https://observer.fedimint.org").is_ok());
        assert!(validate_discovery_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_discovery_url("http://localhost:3000").is_ok());
        assert!(validate_discovery_url("http://some.onion").is_ok());
        assert!(validate_discovery_url("http://evil.com").is_err());
        assert!(validate_discovery_url("ftp://observer.fedimint.org").is_err());
    }
}

