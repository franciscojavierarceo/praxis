// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Anthropic passthrough filter for native `/v1/messages` backends.
//!
//! Manages `anthropic-version` header injection and forwards
//! Anthropic rate-limit response headers to the client. Does not
//! touch the request or response body.

mod config;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests"
)]
mod tests;

use std::borrow::Cow;

use async_trait::async_trait;
use tracing::debug;

use self::config::{AnthropicPassthroughConfig, build_config};
use crate::{
    FilterAction, FilterError,
    body::{BodyAccess, BodyMode},
    factory::parse_filter_config,
    filter::{HttpFilter, HttpFilterContext},
};

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Anthropic API version header name.
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";

// -----------------------------------------------------------------------------
// AnthropicPassthroughFilter
// -----------------------------------------------------------------------------

/// Passes Anthropic Messages requests through to native backends
/// with header management and rate-limit forwarding.
///
/// # YAML
///
/// ```yaml
/// filter: anthropic_passthrough
/// ```
///
/// # Full YAML
///
/// ```yaml
/// filter: anthropic_passthrough
/// default_version: "2023-06-01"
/// forward_rate_limits: true
/// ```
pub struct AnthropicPassthroughFilter {
    /// Parsed and validated configuration.
    config: AnthropicPassthroughConfig,
}

impl AnthropicPassthroughFilter {
    /// Create a filter from parsed YAML config.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] if the YAML config is invalid.
    pub fn from_config(config: &serde_yaml::Value) -> Result<Box<dyn HttpFilter>, FilterError> {
        let cfg: AnthropicPassthroughConfig = parse_filter_config("anthropic_passthrough", config)?;
        let validated = build_config(cfg)?;
        Ok(Box::new(Self { config: validated }))
    }
}

#[async_trait]
impl HttpFilter for AnthropicPassthroughFilter {
    fn name(&self) -> &'static str {
        "anthropic_passthrough"
    }

    fn request_body_access(&self) -> BodyAccess {
        BodyAccess::None
    }

    fn request_body_mode(&self) -> BodyMode {
        BodyMode::Stream
    }

    async fn on_request(&self, ctx: &mut HttpFilterContext<'_>) -> Result<FilterAction, FilterError> {
        let has_version = ctx.request.headers.get(ANTHROPIC_VERSION_HEADER).is_some();

        if !has_version {
            debug!(
                version = self.config.default_version.as_str(),
                "injecting default anthropic-version header"
            );
            ctx.extra_request_headers.push((
                Cow::Borrowed(ANTHROPIC_VERSION_HEADER),
                self.config.default_version.clone(),
            ));
        }

        Ok(FilterAction::Continue)
    }

    async fn on_response(&self, _ctx: &mut HttpFilterContext<'_>) -> Result<FilterAction, FilterError> {
        Ok(FilterAction::Continue)
    }
}
