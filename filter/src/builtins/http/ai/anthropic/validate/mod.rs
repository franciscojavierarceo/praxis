// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Anthropic Messages request validation filter.
//!
//! Validates proxy-needed fields before forwarding. Rejects
//! malformed requests with consistent 400 error responses.
//! Does not create shared state or initialize persistence —
//! Anthropic Messages has no stateful orchestration.

mod config;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::needless_raw_strings,
    clippy::needless_raw_string_hashes,
    reason = "tests"
)]
mod tests;

use async_trait::async_trait;
use bytes::Bytes;
use tracing::debug;

use self::config::{AnthropicValidateConfig, build_config};
use crate::{
    FilterAction, FilterError, Rejection,
    body::{BodyAccess, BodyMode},
    factory::parse_filter_config,
    filter::{HttpFilter, HttpFilterContext},
};

// -----------------------------------------------------------------------------
// AnthropicValidateFilter
// -----------------------------------------------------------------------------

/// Validates Anthropic Messages request bodies for proxy-needed
/// fields. Rejects malformed requests before they reach the backend.
///
/// # YAML
///
/// ```yaml
/// filter: anthropic_validate
/// ```
pub struct AnthropicValidateFilter {
    /// Parsed and validated configuration.
    config: AnthropicValidateConfig,
}

impl AnthropicValidateFilter {
    /// Create a filter from parsed YAML config.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] if the YAML config is invalid.
    pub fn from_config(config: &serde_yaml::Value) -> Result<Box<dyn HttpFilter>, FilterError> {
        let cfg: AnthropicValidateConfig = parse_filter_config("anthropic_validate", config)?;
        let validated = build_config(cfg)?;
        Ok(Box::new(Self { config: validated }))
    }
}

#[async_trait]
impl HttpFilter for AnthropicValidateFilter {
    fn name(&self) -> &'static str {
        "anthropic_validate"
    }

    fn request_body_access(&self) -> BodyAccess {
        BodyAccess::ReadOnly
    }

    fn request_body_mode(&self) -> BodyMode {
        BodyMode::StreamBuffer {
            max_bytes: Some(self.config.max_body_bytes),
        }
    }

    async fn on_request(&self, _ctx: &mut HttpFilterContext<'_>) -> Result<FilterAction, FilterError> {
        Ok(FilterAction::Continue)
    }

    async fn on_request_body(
        &self,
        _ctx: &mut HttpFilterContext<'_>,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
    ) -> Result<FilterAction, FilterError> {
        if !end_of_stream {
            return Ok(FilterAction::Continue);
        }

        let Some(bytes) = body.as_deref().filter(|b| !b.is_empty()) else {
            return Ok(FilterAction::Reject(reject("request body is required")));
        };

        if let Some(rejection) = validate_request(bytes) {
            return Ok(FilterAction::Reject(rejection));
        }

        debug!("anthropic request validation passed");
        Ok(FilterAction::Continue)
    }
}

// -----------------------------------------------------------------------------
// Validation
// -----------------------------------------------------------------------------

/// Validate proxy-needed fields in the request body.
fn validate_request(body: &[u8]) -> Option<Rejection> {
    let value: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return Some(reject("request body is not valid JSON")),
    };

    let Some(obj) = value.as_object() else {
        return Some(reject("request body is not a JSON object"));
    };

    if let Err(msg) = check_model(obj) {
        return Some(reject(&msg));
    }

    if let Err(msg) = check_max_tokens(obj) {
        return Some(reject(&msg));
    }

    if let Err(msg) = check_messages(obj) {
        return Some(reject(&msg));
    }

    None
}

/// Check that `model` is present and non-empty.
fn check_model(obj: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    match obj.get("model") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Ok(()),
        Some(serde_json::Value::String(_)) => Err("'model' must not be empty".to_owned()),
        Some(_) => Err("'model' must be a string".to_owned()),
        None => Err("'model' is required".to_owned()),
    }
}

/// Check that `max_tokens` is present and > 0.
fn check_max_tokens(obj: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    match obj.get("max_tokens") {
        Some(serde_json::Value::Number(n)) => {
            if n.as_u64().is_some_and(|v| v > 0) {
                Ok(())
            } else {
                Err("'max_tokens' must be a positive integer".to_owned())
            }
        },
        Some(_) => Err("'max_tokens' must be a number".to_owned()),
        None => Err("'max_tokens' is required".to_owned()),
    }
}

/// Check that `messages` is a non-empty array.
///
/// Role ordering (e.g. first message must be `role: user`) is
/// deferred to the backend, consistent with validating only what
/// the proxy needs for its own operation.
fn check_messages(obj: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let Some(serde_json::Value::Array(messages)) = obj.get("messages") else {
        return Err("'messages' must be a non-empty array".to_owned());
    };

    if messages.is_empty() {
        return Err("'messages' must not be empty".to_owned());
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Build a 400 rejection with a JSON error body.
fn reject(message: &str) -> Rejection {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error"
        }
    })
    .to_string();

    Rejection::status(400)
        .with_header("content-type", "application/json")
        .with_body(Bytes::from(body))
}
