// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Configuration for the Anthropic passthrough filter.

use serde::Deserialize;

use crate::FilterError;

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

/// Default Anthropic API version header value.
const DEFAULT_VERSION: &str = "2023-06-01";

// -----------------------------------------------------------------------------
// AnthropicPassthroughConfig
// -----------------------------------------------------------------------------

/// YAML configuration for the [`AnthropicPassthroughFilter`].
///
/// [`AnthropicPassthroughFilter`]: super::AnthropicPassthroughFilter
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnthropicPassthroughConfig {
    /// Default `anthropic-version` header value when absent.
    #[serde(default = "default_version")]
    pub default_version: String,
}

/// Default version string.
fn default_version() -> String {
    DEFAULT_VERSION.to_owned()
}

// -----------------------------------------------------------------------------
// Config Validation
// -----------------------------------------------------------------------------

/// Validate the parsed configuration.
pub(crate) fn build_config(cfg: AnthropicPassthroughConfig) -> Result<AnthropicPassthroughConfig, FilterError> {
    if cfg.default_version.is_empty() {
        return Err("anthropic_passthrough: 'default_version' must not be empty".into());
    }
    Ok(cfg)
}
