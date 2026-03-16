//! Connector proxy policy evaluation.
//!
//! This module implements the deny-first, allow-second, deny-by-default
//! policy engine described in `docs/permissions.md`.
//!
//! It does **not** handle HTTP forwarding or credential injection itself —
//! that remains in `AuthenticatedHttpTool`. This module is invoked *before*
//! the HTTP request is made to decide whether it should be allowed.

use clawkson_core::models::{ConnectorPolicy, HttpMethod, ProxyRule};
use glob_match::glob_match;

/// The result of evaluating a request against a `ConnectorPolicy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyVerdict {
    /// The request is allowed by the policy.
    Allowed,
    /// The request is denied, with a human-readable reason.
    Denied(String),
}

/// Parse an HTTP method string (e.g. "GET") into our `HttpMethod` enum.
pub fn parse_http_method(method: &str) -> Option<HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "PATCH" => Some(HttpMethod::Patch),
        "DELETE" => Some(HttpMethod::Delete),
        "HEAD" => Some(HttpMethod::Head),
        "OPTIONS" => Some(HttpMethod::Options),
        _ => None,
    }
}

/// Check if a single `ProxyRule` matches the given method + path.
fn rule_matches(rule: &ProxyRule, method: &HttpMethod, url_path: &str) -> bool {
    // Method must be in the rule's method list
    if !rule.methods.contains(method) {
        return false;
    }
    // Path must match the glob pattern
    glob_match(&rule.path_pattern, url_path)
}

/// Evaluate a request against a `ConnectorPolicy`.
///
/// Order:
///   1. If any **deny** rule matches → Denied.
///   2. If at least one **allow** rule matches → Allowed.
///   3. Otherwise → Denied (deny-by-default).
pub fn evaluate_policy(
    policy: &ConnectorPolicy,
    method: &HttpMethod,
    url_path: &str,
) -> PolicyVerdict {
    // 1. Check deny rules first
    for rule in &policy.deny {
        if rule_matches(rule, method, url_path) {
            return PolicyVerdict::Denied(format!(
                "Blocked by deny rule: {} (pattern: {} {})",
                rule.description,
                rule.methods
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                rule.path_pattern,
            ));
        }
    }

    // 2. Check allow rules
    for rule in &policy.allow {
        if rule_matches(rule, method, url_path) {
            return PolicyVerdict::Allowed;
        }
    }

    // 3. Deny by default
    PolicyVerdict::Denied(format!("No allow rule matched for {} {}", method, url_path,))
}

/// Extract the path portion of a URL (without query string or fragment).
/// Returns `None` if the URL is not parseable.
pub fn extract_url_path(url: &str) -> Option<String> {
    // Try to parse as a full URL first
    if let Ok(parsed) = url::Url::parse(url) {
        return Some(parsed.path().to_string());
    }
    // Fallback: treat as a path directly
    if url.starts_with('/') {
        // Strip query string
        let path = url.split('?').next().unwrap_or(url);
        let path = path.split('#').next().unwrap_or(path);
        return Some(path.to_string());
    }
    None
}

/// Evaluate a request against a list of connector policies for a specific connector.
/// Returns the verdict from the first matching policy (by connector_id).
/// If no policy exists for the connector, returns Denied (deny-by-default).
pub fn evaluate_request(
    policies: &[ConnectorPolicy],
    connector_id: &uuid::Uuid,
    method: &HttpMethod,
    url_path: &str,
) -> PolicyVerdict {
    match policies.iter().find(|p| &p.connector_id == connector_id) {
        Some(policy) => evaluate_policy(policy, method, url_path),
        None => PolicyVerdict::Denied(format!(
            "No connector policy defined for connector {}",
            connector_id,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_policy(
        allow_methods: Vec<HttpMethod>,
        allow_pattern: &str,
        deny_methods: Vec<HttpMethod>,
        deny_pattern: &str,
    ) -> ConnectorPolicy {
        ConnectorPolicy {
            connector_id: Uuid::nil(),
            allow: vec![ProxyRule {
                methods: allow_methods,
                path_pattern: allow_pattern.to_string(),
                description: "test allow".to_string(),
            }],
            deny: if deny_methods.is_empty() {
                vec![]
            } else {
                vec![ProxyRule {
                    methods: deny_methods,
                    path_pattern: deny_pattern.to_string(),
                    description: "test deny".to_string(),
                }]
            },
            rate_limit_rpm: None,
        }
    }

    #[test]
    fn test_allow_get_only() {
        let policy = make_policy(
            vec![HttpMethod::Get],
            "/gmail/v1/users/me/messages/**",
            vec![],
            "",
        );
        assert_eq!(
            evaluate_policy(&policy, &HttpMethod::Get, "/gmail/v1/users/me/messages/123"),
            PolicyVerdict::Allowed,
        );
        // POST should be denied
        assert!(matches!(
            evaluate_policy(&policy, &HttpMethod::Post, "/gmail/v1/users/me/messages"),
            PolicyVerdict::Denied(_),
        ));
    }

    #[test]
    fn test_deny_overrides_allow() {
        let policy = make_policy(
            vec![HttpMethod::Get, HttpMethod::Delete],
            "/gmail/v1/**",
            vec![HttpMethod::Delete],
            "/gmail/v1/**",
        );
        // GET is allowed
        assert_eq!(
            evaluate_policy(&policy, &HttpMethod::Get, "/gmail/v1/users/me/messages"),
            PolicyVerdict::Allowed,
        );
        // DELETE is denied despite being in allow
        assert!(matches!(
            evaluate_policy(
                &policy,
                &HttpMethod::Delete,
                "/gmail/v1/users/me/messages/123"
            ),
            PolicyVerdict::Denied(_),
        ));
    }

    #[test]
    fn test_deny_by_default() {
        let policy = make_policy(vec![HttpMethod::Get], "/api/v1/read/**", vec![], "");
        // Path doesn't match any allow rule
        assert!(matches!(
            evaluate_policy(&policy, &HttpMethod::Get, "/api/v2/something"),
            PolicyVerdict::Denied(_),
        ));
    }

    #[test]
    fn test_no_policy_for_connector() {
        let policies = vec![make_policy(vec![HttpMethod::Get], "/**", vec![], "")];
        let unknown_id = Uuid::new_v4();
        assert!(matches!(
            evaluate_request(&policies, &unknown_id, &HttpMethod::Get, "/anything"),
            PolicyVerdict::Denied(_),
        ));
    }

    #[test]
    fn test_extract_url_path() {
        assert_eq!(
            extract_url_path("https://gmail.googleapis.com/gmail/v1/users/me/messages?q=is:unread"),
            Some("/gmail/v1/users/me/messages".to_string()),
        );
        assert_eq!(
            extract_url_path("/api/v1/items"),
            Some("/api/v1/items".to_string()),
        );
    }
}
