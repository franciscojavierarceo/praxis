// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Unit tests for the `anthropic_messages_format` filter.

use bytes::Bytes;

use super::*;

// -----------------------------------------------------------------------------
// Config Parsing
// -----------------------------------------------------------------------------

#[test]
fn default_config_parses() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("{}").unwrap();
    let filter = AnthropicMessagesFormatFilter::from_config(&yaml).unwrap();
    assert_eq!(
        filter.name(),
        "anthropic_messages_format",
        "filter name should be anthropic_messages_format"
    );
}

#[test]
fn full_config_parses() {
    let yaml: serde_yaml::Value = serde_yaml::from_str(
        r#"
on_invalid: reject
max_body_bytes: 65536
headers:
  format: x-custom-format
  model: x-custom-model
  stream: x-custom-stream
"#,
    )
    .unwrap();
    let filter = AnthropicMessagesFormatFilter::from_config(&yaml).unwrap();
    assert_eq!(
        filter.name(),
        "anthropic_messages_format",
        "filter should parse full config"
    );
}

#[test]
fn zero_max_body_bytes_rejected() {
    let yaml: serde_yaml::Value = serde_yaml::from_str("max_body_bytes: 0").unwrap();
    let result = AnthropicMessagesFormatFilter::from_config(&yaml);
    assert!(result.is_err(), "zero max_body_bytes should be rejected");
}

// -----------------------------------------------------------------------------
// Handle Invalid Format
// -----------------------------------------------------------------------------

#[test]
fn anthropic_messages_not_rejected() {
    let cfg: AnthropicMessagesFormatConfig = serde_yaml::from_str("on_invalid: reject").unwrap();
    let result = handle_invalid_format(AiRequestFormat::AnthropicMessages, &cfg);
    assert!(result.is_none(), "anthropic_messages format should not be rejected");
}

#[test]
fn responses_not_rejected() {
    let cfg: AnthropicMessagesFormatConfig = serde_yaml::from_str("on_invalid: reject").unwrap();
    let result = handle_invalid_format(AiRequestFormat::Responses, &cfg);
    assert!(result.is_none(), "responses format should not be rejected");
}

#[test]
fn invalid_json_rejected_in_reject_mode() {
    let cfg: AnthropicMessagesFormatConfig = serde_yaml::from_str("on_invalid: reject").unwrap();
    let result = handle_invalid_format(AiRequestFormat::InvalidJson, &cfg);
    assert!(result.is_some(), "invalid JSON should be rejected in reject mode");
}

// -----------------------------------------------------------------------------
// Promotion Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn promotes_anthropic_messages_format() {
    let ctx = run_filter(
        "{}",
        r#"{"model":"claude-opus-4-8","max_tokens":1024,"system":"You are helpful.","messages":[{"role":"user","content":"Hi"}]}"#,
    )
    .await;
    let headers = collect_headers(&ctx);

    assert_eq!(
        headers.get("x-praxis-ai-format"),
        Some(&"anthropic_messages"),
        "format header should be anthropic_messages"
    );
    assert_eq!(
        headers.get("x-praxis-ai-model"),
        Some(&"claude-opus-4-8"),
        "model header"
    );
}

#[tokio::test]
async fn promotes_metadata_for_anthropic_request() {
    let ctx = run_filter(
        "{}",
        r#"{"model":"claude-opus-4-8","max_tokens":512,"system":"Be helpful.","messages":[{"role":"user","content":"Hi"}],"stream":true}"#,
    )
    .await;

    assert_eq!(
        ctx.filter_metadata.get("anthropic_format.format").map(String::as_str),
        Some("anthropic_messages"),
        "format metadata"
    );
    assert_eq!(
        ctx.filter_metadata.get("anthropic_format.model").map(String::as_str),
        Some("claude-opus-4-8"),
        "model metadata"
    );
    assert_eq!(
        ctx.filter_metadata.get("anthropic_format.stream").map(String::as_str),
        Some("true"),
        "stream metadata"
    );
    assert_eq!(
        ctx.filter_metadata
            .get("anthropic_format.max_tokens")
            .map(String::as_str),
        Some("512"),
        "max_tokens metadata"
    );
}

#[tokio::test]
async fn chat_completions_without_max_tokens_on_non_messages_path() {
    let filter = make_filter("{}");
    let req = crate::test_utils::make_request(http::Method::POST, "/v1/chat/completions");

    let req: &'static crate::context::Request = Box::leak(Box::new(req));
    let mut ctx = crate::test_utils::make_filter_context(req);
    let mut body = Some(Bytes::from(
        r#"{"model":"gpt-4","messages":[{"role":"user","content":"Hi"}]}"#,
    ));

    let action = filter.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(matches!(action, FilterAction::Release), "filter should release");

    let headers = collect_headers(&ctx);
    assert_eq!(
        headers.get("x-praxis-ai-format"),
        Some(&"openai_chat_completions"),
        "messages without max_tokens on /v1/chat/completions should be chat_completions"
    );
}

#[tokio::test]
async fn anthropic_version_header_overrides_body_heuristic() {
    let filter = make_filter("{}");
    let mut req = crate::test_utils::make_request(http::Method::POST, "/v1/messages");
    req.headers.insert("anthropic-version", "2023-06-01".parse().unwrap());

    let req: &'static crate::context::Request = Box::leak(Box::new(req));
    let mut ctx = crate::test_utils::make_filter_context(req);
    let mut body = Some(Bytes::from(
        r#"{"model":"claude-opus-4-8","messages":[{"role":"user","content":"Hi"}]}"#,
    ));

    let action = filter.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(matches!(action, FilterAction::Release), "filter should release");

    let headers = collect_headers(&ctx);
    assert_eq!(
        headers.get("x-praxis-ai-format"),
        Some(&"anthropic_messages"),
        "anthropic-version header should override to anthropic_messages"
    );
}

#[tokio::test]
async fn minimal_messages_path_overrides_to_anthropic() {
    let ctx = run_filter(
        "{}",
        r#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}"#,
    )
    .await;
    let headers = collect_headers(&ctx);

    assert_eq!(
        headers.get("x-praxis-ai-format"),
        Some(&"anthropic_messages"),
        "minimal body on /v1/messages path should classify as anthropic_messages"
    );
}

// -----------------------------------------------------------------------------
// Test Utilities
// -----------------------------------------------------------------------------

/// Run the filter and return the resulting context.
async fn run_filter(config_yaml: &str, body_str: &str) -> HttpFilterContext<'static> {
    let filter = make_filter(config_yaml);
    let req = crate::test_utils::make_request(http::Method::POST, "/v1/messages");

    let req: &'static crate::context::Request = Box::leak(Box::new(req));
    let mut ctx = crate::test_utils::make_filter_context(req);
    let mut body = Some(Bytes::from(body_str.to_owned()));

    let action = filter.on_request_body(&mut ctx, &mut body, true).await.unwrap();
    assert!(matches!(action, FilterAction::Release), "filter should release");
    ctx
}

/// Collect extra request headers into a map.
fn collect_headers<'a>(ctx: &'a HttpFilterContext<'_>) -> std::collections::HashMap<&'a str, &'a str> {
    ctx.extra_request_headers
        .iter()
        .map(|(k, v)| (k.as_ref(), v.as_str()))
        .collect()
}

/// Build a filter from a YAML snippet.
fn make_filter(yaml_str: &str) -> Box<dyn HttpFilter> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
    AnthropicMessagesFormatFilter::from_config(&yaml).unwrap()
}
