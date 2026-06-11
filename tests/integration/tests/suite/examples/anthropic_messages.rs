// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Functional tests for Anthropic Messages example configs.

use std::collections::HashMap;

use praxis_test_utils::{
    free_port, http_send, json_post, load_example_config, parse_body, parse_status, start_backend_with_shutdown,
    start_proxy,
};

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn anthropic_passthrough_routes_to_backend() {
    let backend_guard = start_backend_with_shutdown("anthropic-backend");
    let proxy_port = free_port();

    let config = load_example_config(
        "ai/anthropic/messages-passthrough.yaml",
        proxy_port,
        HashMap::from([("127.0.0.1:3001", backend_guard.port())]),
    );
    let proxy = start_proxy(&config);

    let body = r#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}"#;
    let raw = http_send(proxy.addr(), &json_post("/v1/messages", body));

    assert_eq!(parse_status(&raw), 200, "passthrough should return 200");
    assert_eq!(
        parse_body(&raw),
        "anthropic-backend",
        "should route to anthropic-backend"
    );
}

#[test]
fn anthropic_to_openai_routes_to_backend() {
    let backend_guard = start_backend_with_shutdown("openai-backend");
    let proxy_port = free_port();

    let config = load_example_config(
        "ai/anthropic/messages-to-openai.yaml",
        proxy_port,
        HashMap::from([("127.0.0.1:3001", backend_guard.port())]),
    );
    let proxy = start_proxy(&config);

    let body = r#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}"#;
    let raw = http_send(proxy.addr(), &json_post("/v1/messages", body));

    assert_eq!(parse_status(&raw), 200, "transformation should return 200");
    assert_eq!(parse_body(&raw), "openai-backend", "should route to openai-backend");
}

#[test]
fn unified_gateway_routes_anthropic_to_correct_backend() {
    let anthropic_guard = start_backend_with_shutdown("anthropic-backend");
    let openai_guard = start_backend_with_shutdown("openai-backend");
    let responses_guard = start_backend_with_shutdown("responses-backend");
    let default_guard = start_backend_with_shutdown("default-backend");
    let proxy_port = free_port();

    let config = load_example_config(
        "ai/anthropic/unified-gateway.yaml",
        proxy_port,
        HashMap::from([
            ("127.0.0.1:3001", anthropic_guard.port()),
            ("127.0.0.1:3002", openai_guard.port()),
            ("127.0.0.1:3003", responses_guard.port()),
            ("127.0.0.1:3004", default_guard.port()),
        ]),
    );
    let proxy = start_proxy(&config);

    let anthropic_body = r#"{"model":"claude-opus-4-8","max_tokens":1024,"messages":[{"role":"user","content":"Hi"}]}"#;
    let raw = http_send(proxy.addr(), &json_post("/v1/messages", anthropic_body));
    assert_eq!(parse_status(&raw), 200, "anthropic request should return 200");
    assert_eq!(
        parse_body(&raw),
        "anthropic-backend",
        "anthropic should route to anthropic-backend"
    );
}

#[test]
fn unified_gateway_routes_openai_to_correct_backend() {
    let anthropic_guard = start_backend_with_shutdown("anthropic-backend");
    let openai_guard = start_backend_with_shutdown("openai-backend");
    let responses_guard = start_backend_with_shutdown("responses-backend");
    let default_guard = start_backend_with_shutdown("default-backend");
    let proxy_port = free_port();

    let config = load_example_config(
        "ai/anthropic/unified-gateway.yaml",
        proxy_port,
        HashMap::from([
            ("127.0.0.1:3001", anthropic_guard.port()),
            ("127.0.0.1:3002", openai_guard.port()),
            ("127.0.0.1:3003", responses_guard.port()),
            ("127.0.0.1:3004", default_guard.port()),
        ]),
    );
    let proxy = start_proxy(&config);

    let openai_body = r#"{"model":"gpt-4","messages":[{"role":"user","content":"Hi"}]}"#;
    let raw = http_send(proxy.addr(), &json_post("/v1/chat/completions", openai_body));
    assert_eq!(parse_status(&raw), 200, "openai request should return 200");
    assert_eq!(
        parse_body(&raw),
        "openai-backend",
        "openai should route to openai-backend"
    );
}
