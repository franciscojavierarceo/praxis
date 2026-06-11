// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Praxis Contributors

//! Configuration for the Anthropic stream events filter.

use serde::Deserialize;

// -----------------------------------------------------------------------------
// AnthropicStreamEventsConfig
// -----------------------------------------------------------------------------

/// YAML configuration for the [`AnthropicStreamEventsFilter`].
///
/// [`AnthropicStreamEventsFilter`]: super::AnthropicStreamEventsFilter
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnthropicStreamEventsConfig {}
