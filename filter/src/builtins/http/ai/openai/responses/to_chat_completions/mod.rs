// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Responses API to Chat Completions-compatible transformation filter.
//!
//! Rewrites non-streaming `OpenAI` Responses API request bodies to the
//! Chat Completions wire shape and transforms compatible non-streaming
//! responses back into Responses resources.

mod config;

use std::borrow::Cow;

use async_trait::async_trait;
use bytes::Bytes;
use tracing::{debug, warn};

use self::config::{OpenaiResponsesToChatCompletionsConfig, build_config};
use crate::{
    FilterAction, FilterError, Rejection,
    body::{BodyAccess, BodyMode},
    builtins::http::ai::translation::chat_completions::{
        ResponseContext, chat_response_to_response_resource, responses_request_to_chat_request,
    },
    factory::parse_filter_config,
    filter::{HttpFilter, HttpFilterContext},
};

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Filter metadata key used by the classifier.
const RESPONSES_FORMAT_KEY: &str = "openai_responses_format.format";

/// Classifier value for Responses API requests.
const RESPONSES_FORMAT_VALUE: &str = "openai_responses";

// -----------------------------------------------------------------------------
// OpenaiResponsesToChatCompletionsFilter
// -----------------------------------------------------------------------------

/// Transforms non-streaming Responses API requests for Chat Completions backends.
///
/// # YAML
///
/// ```yaml
/// filter: openai_responses_to_chat_completions
/// max_body_bytes: 67108864
/// ```
pub struct OpenaiResponsesToChatCompletionsFilter {
    /// Parsed and validated configuration.
    config: OpenaiResponsesToChatCompletionsConfig,
}

impl OpenaiResponsesToChatCompletionsFilter {
    /// Create a filter from parsed YAML config.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] if the YAML config is invalid.
    pub fn from_config(config: &serde_yaml::Value) -> Result<Box<dyn HttpFilter>, FilterError> {
        let cfg: OpenaiResponsesToChatCompletionsConfig =
            parse_filter_config("openai_responses_to_chat_completions", config)?;
        let validated = build_config(cfg)?;
        Ok(Box::new(Self { config: validated }))
    }
}

#[async_trait]
impl HttpFilter for OpenaiResponsesToChatCompletionsFilter {
    fn name(&self) -> &'static str {
        "openai_responses_to_chat_completions"
    }

    fn request_body_access(&self) -> BodyAccess {
        BodyAccess::ReadWrite
    }

    fn request_body_mode(&self) -> BodyMode {
        BodyMode::StreamBuffer {
            max_bytes: Some(self.config.max_body_bytes),
        }
    }

    fn response_body_access(&self) -> BodyAccess {
        BodyAccess::ReadWrite
    }

    fn response_body_mode(&self) -> BodyMode {
        BodyMode::Stream
    }

    async fn on_request(&self, _ctx: &mut HttpFilterContext<'_>) -> Result<FilterAction, FilterError> {
        Ok(FilterAction::Continue)
    }

    async fn on_response(&self, ctx: &mut HttpFilterContext<'_>) -> Result<FilterAction, FilterError> {
        if should_transform_response(ctx) {
            ctx.set_response_body_mode(BodyMode::StreamBuffer {
                max_bytes: Some(self.config.max_body_bytes),
            });
            if let Some(resp) = &mut ctx.response_header {
                resp.headers.remove(http::header::CONTENT_LENGTH);
                ctx.response_headers_modified = true;
            }
        }

        Ok(FilterAction::Continue)
    }

    async fn on_request_body(
        &self,
        ctx: &mut HttpFilterContext<'_>,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
    ) -> Result<FilterAction, FilterError> {
        if !end_of_stream || should_skip_request(ctx) {
            return Ok(FilterAction::Continue);
        }

        let Some(bytes) = request_body_bytes(body) else {
            return Ok(FilterAction::Continue);
        };

        let transformed = match transform_request_body(ctx, bytes) {
            Ok(transformed) => transformed,
            Err(action) => return Ok(action),
        };

        debug!(
            original_len = bytes.len(),
            transformed_len = transformed.body.len(),
            "transformed Responses request to Chat Completions-compatible format"
        );

        ctx.insert_filter_state(ResponsesToChatState {
            context: transformed.context,
        });
        ctx.extra_request_headers
            .push((Cow::Borrowed("content-length"), transformed.body.len().to_string()));
        *body = Some(Bytes::from(transformed.body));

        Ok(FilterAction::Continue)
    }

    fn on_response_body(
        &self,
        ctx: &mut HttpFilterContext<'_>,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
    ) -> Result<FilterAction, FilterError> {
        if !end_of_stream || !should_transform_response(ctx) {
            return Ok(FilterAction::Continue);
        }

        let Some(bytes) = body.as_ref() else {
            return Ok(FilterAction::Continue);
        };
        if bytes.is_empty() {
            return Ok(FilterAction::Continue);
        }

        let transformed = transform_response_body(ctx, bytes);
        if let Some(transformed) = transformed {
            if let Some(resp) = &mut ctx.response_header {
                resp.headers
                    .insert(http::header::CONTENT_LENGTH, http::HeaderValue::from(transformed.len()));
                resp.headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
                ctx.response_headers_modified = true;
            }
            *body = Some(transformed);
        }

        Ok(FilterAction::Continue)
    }
}

// -----------------------------------------------------------------------------
// State
// -----------------------------------------------------------------------------

/// Per-request state captured from the original Responses request.
#[derive(Debug)]
struct ResponsesToChatState {
    /// Response-context fields needed when transforming the backend response.
    context: ResponseContext,
}

/// Transformed request body and response context captured together.
#[derive(Debug)]
struct TransformedRequest {
    /// Request body to send upstream.
    body: Vec<u8>,
    /// Response transform context derived from the original request.
    context: ResponseContext,
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Borrow non-empty request body bytes.
fn request_body_bytes(body: &Option<Bytes>) -> Option<&[u8]> {
    match body.as_ref() {
        Some(bytes) if !bytes.is_empty() => Some(bytes.as_ref()),
        _ => None,
    }
}

/// Transform one complete Responses request body.
fn transform_request_body(ctx: &HttpFilterContext<'_>, bytes: &[u8]) -> Result<TransformedRequest, FilterAction> {
    let request = parse_request_body(bytes)?;
    reject_streaming_request(&request)?;

    let context = response_context(ctx, &request);
    let body = serialize_chat_request(&request)?;

    Ok(TransformedRequest { body, context })
}

/// Parse the original Responses request body.
fn parse_request_body(bytes: &[u8]) -> Result<serde_json::Value, FilterAction> {
    serde_json::from_slice(bytes).map_err(|e| reject_invalid(&format!("invalid request body: {e}")))
}

/// Reject streaming until a per-event Responses stream filter is available.
fn reject_streaming_request(request: &serde_json::Value) -> Result<(), FilterAction> {
    if request.get("stream").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(reject_invalid(
            "streaming Responses to Chat Completions translation requires the Responses stream_events filter",
        ));
    }

    Ok(())
}

/// Serialize the Chat Completions-compatible request body.
fn serialize_chat_request(request: &serde_json::Value) -> Result<Vec<u8>, FilterAction> {
    let transformed = responses_request_to_chat_request(request).map_err(|e| reject_invalid(&e.to_string()))?;

    serde_json::to_vec(&transformed).map_err(|e| reject_invalid(&format!("serialization failed: {e}")))
}

/// Return true when classifier metadata proves this is not a Responses request.
fn should_skip_request(ctx: &HttpFilterContext<'_>) -> bool {
    ctx.get_metadata(RESPONSES_FORMAT_KEY)
        .is_some_and(|format| format != RESPONSES_FORMAT_VALUE)
}

/// Return true when the non-streaming upstream response should be transformed.
fn should_transform_response(ctx: &HttpFilterContext<'_>) -> bool {
    if ctx.get_filter_state::<ResponsesToChatState>().is_none() {
        return false;
    }

    ctx.response_header
        .as_ref()
        .is_none_or(|response| response.status.is_success())
}

/// Build the response transform context from request and filter metadata.
fn response_context(ctx: &HttpFilterContext<'_>, request: &serde_json::Value) -> ResponseContext {
    let response_id = ctx.get_metadata("responses.response_id").map_or_else(
        || format!("resp_{}", ctx.id_generator.generate(ctx.time_source)),
        str::to_owned,
    );
    let created_at = ctx.time_source.now().as_secs();

    ResponseContext::from_responses_request(request, response_id, created_at)
}

/// Apply non-streaming Chat Completions response transformation.
fn transform_response_body(ctx: &HttpFilterContext<'_>, bytes: &Bytes) -> Option<Bytes> {
    let state = ctx.get_filter_state::<ResponsesToChatState>()?;
    let response = parse_chat_response(bytes)?;
    let transformed = response_resource_bytes(&response, &state.context)?;

    debug!(
        transformed_len = transformed.len(),
        "transformed Chat Completions response to Responses"
    );

    Some(Bytes::from(transformed))
}

/// Parse a Chat Completions response body.
fn parse_chat_response(bytes: &Bytes) -> Option<serde_json::Value> {
    match serde_json::from_slice(bytes) {
        Ok(value) => Some(value),
        Err(e) => {
            warn!(error = %e, "failed to parse Chat Completions response");
            None
        },
    }
}

/// Convert and serialize a Chat Completions response body.
fn response_resource_bytes(response: &serde_json::Value, context: &ResponseContext) -> Option<Vec<u8>> {
    let transformed = match chat_response_to_response_resource(response, context) {
        Ok(value) => value,
        Err(e) => {
            warn!(error = %e, "failed to transform Chat Completions response");
            return None;
        },
    };

    match serde_json::to_vec(&transformed) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            warn!(error = %e, "failed to serialize Responses resource");
            None
        },
    }
}

/// Build a 400 rejection with a JSON error body.
fn reject_invalid(message: &str) -> FilterAction {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error"
        }
    })
    .to_string();

    FilterAction::Reject(
        Rejection::status(400)
            .with_header("content-type", "application/json")
            .with_body(Bytes::from(body)),
    )
}
