// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Configuration for the Anthropic stream events filter.

use serde::Deserialize;

use crate::FilterError;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Default maximum response body size (1 MiB).
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576; // 1 MiB

// -----------------------------------------------------------------------------
// AnthropicStreamEventsConfig
// -----------------------------------------------------------------------------

/// YAML configuration for the [`AnthropicStreamEventsFilter`].
///
/// [`AnthropicStreamEventsFilter`]: super::AnthropicStreamEventsFilter
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnthropicStreamEventsConfig {
    /// Maximum body size in bytes for `StreamBuffer` mode.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
}

/// Default max body bytes.
fn default_max_body_bytes() -> usize {
    DEFAULT_MAX_BODY_BYTES
}

// -----------------------------------------------------------------------------
// Config Validation
// -----------------------------------------------------------------------------

/// Validate the parsed configuration.
pub(crate) fn build_config(cfg: AnthropicStreamEventsConfig) -> Result<AnthropicStreamEventsConfig, FilterError> {
    if cfg.max_body_bytes == 0 {
        return Err("anthropic_stream_events: 'max_body_bytes' must be greater than 0".into());
    }
    Ok(cfg)
}
