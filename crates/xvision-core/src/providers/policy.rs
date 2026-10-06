//! Provider shutdown policy, shared by the engine and sidecar client.
//!
//! Groq is disabled to prevent saved configurations from resuming paid usage.

pub const GROQ_DISABLED_MESSAGE: &str =
    "Groq is disabled in xvision; remove the saved provider and revoke its API key";

/// Match the URL authority, rather than a substring in the path or query.
/// No network or secret access is performed here.
pub fn is_groq_endpoint(base_url: &str) -> bool {
    let value = base_url.trim();
    let authority = value.split_once("://").map_or(value, |(_, rest)| rest);
    let authority = authority.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = host_port
        .split(':')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    host == "groq.com" || host.ends_with(".groq.com")
}

pub fn is_groq_provider(name: &str, base_url: &str, api_key_env: &str) -> bool {
    name.trim().eq_ignore_ascii_case("groq")
        || api_key_env.trim().eq_ignore_ascii_case("GROQ_API_KEY")
        || is_groq_endpoint(base_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_saved_and_custom_groq_endpoints() {
        for url in [
            "https://api.groq.com/openai/v1",
            " HTTPS://API.GROQ.COM.:443/openai/v1/ ",
            "https://user@api.groq.com/openai/v1",
            "https://groq.com",
        ] {
            assert!(is_groq_endpoint(url), "{url}");
            assert!(is_groq_provider("custom", url, "CUSTOM_KEY"));
        }
        assert!(is_groq_provider("Groq", "", ""));
        assert!(is_groq_provider(
            "custom",
            "https://proxy.example/v1",
            "GROQ_API_KEY"
        ));
    }

    #[test]
    fn preserves_other_providers_and_local_models() {
        for url in [
            "https://api.groq.com.example/v1",
            "https://proxy.example/api.groq.com",
            "https://api.groq.com@proxy.example/v1",
            "https://openrouter.ai/api/v1",
            "http://localhost:11434/v1",
        ] {
            assert!(!is_groq_endpoint(url), "{url}");
        }
    }
}
