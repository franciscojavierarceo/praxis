// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Unit tests for the `anthropic_validate` filter.

use super::*;

// -----------------------------------------------------------------------------
// Validation Logic
// -----------------------------------------------------------------------------

#[test]
fn valid_request_passes() {
    let body = br#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}"#;
    assert!(validate_request(body).is_none(), "valid request should pass");
}

#[test]
fn missing_model_rejected() {
    let body = br#"{"max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "missing model should be rejected");
}

#[test]
fn empty_model_rejected() {
    let body = br#"{"model":"","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "empty model should be rejected");
}

#[test]
fn missing_max_tokens_rejected() {
    let body = br#"{"model":"claude-opus-4-8","messages":[{"role":"user","content":"Hi"}]}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "missing max_tokens should be rejected");
}

#[test]
fn zero_max_tokens_rejected() {
    let body = br#"{"model":"claude-opus-4-8","max_tokens":0,"messages":[{"role":"user","content":"Hi"}]}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "zero max_tokens should be rejected");
}

#[test]
fn missing_messages_rejected() {
    let body = br#"{"model":"claude-opus-4-8","max_tokens":1024}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "missing messages should be rejected");
}

#[test]
fn empty_messages_rejected() {
    let body = br#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[]}"#;
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "empty messages should be rejected");
}

#[test]
fn first_message_not_user_passes() {
    let body = br#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"assistant","content":"Hi"}]}"#;
    assert!(validate_request(body).is_none(), "role ordering deferred to backend");
}

#[test]
fn invalid_json_rejected() {
    let body = b"not json {{{";
    let rejection = validate_request(body);
    assert!(rejection.is_some(), "invalid JSON should be rejected");
}

#[tokio::test]
async fn empty_body_rejected_by_filter() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("{}").unwrap();
    let filter = AnthropicValidateFilter::from_config(&yaml).unwrap();
    let req = Box::leak(Box::new(crate::test_utils::make_request(
        http::Method::POST,
        "/v1/messages",
    )));
    let mut ctx = crate::test_utils::make_filter_context(req);
    let mut body = Some(Bytes::new());

    let action = filter.on_request_body(&mut ctx, &mut body, true).await.unwrap();

    assert!(
        matches!(action, FilterAction::Reject(_)),
        "empty body should be rejected"
    );
}

// -----------------------------------------------------------------------------
// Config
// -----------------------------------------------------------------------------

#[test]
fn default_config_parses() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("{}").unwrap();
    let filter = AnthropicValidateFilter::from_config(&yaml).unwrap();
    assert_eq!(
        filter.name(),
        "anthropic_validate",
        "filter name should be anthropic_validate"
    );
}

#[test]
fn zero_max_body_bytes_rejected() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("max_body_bytes: 0").unwrap();
    let result = AnthropicValidateFilter::from_config(&yaml);
    assert!(result.is_err(), "zero max_body_bytes should be rejected");
}

#[test]
fn rejects_max_body_bytes_above_ceiling() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("max_body_bytes: 67108865").unwrap();
    let result = AnthropicValidateFilter::from_config(&yaml);

    assert!(
        result.is_err(),
        "max_body_bytes above 64 MiB ceiling should be rejected"
    );
}
