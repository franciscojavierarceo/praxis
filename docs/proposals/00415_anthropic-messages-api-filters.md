---
issue: # to be created
discussion: # to be created
status: proposed
authors:
  - franciscojavierarceo
stakeholders:
  - leseb
  - shaneutt
  - nerdalert
---

# Anthropic Messages API Filters

## What?

Add Anthropic Messages API support to Praxis as
composable filters, mirroring the pattern established
by the OpenAI Responses API filters in #354. This
enables Praxis to classify, route, and transform
requests between the Anthropic Messages API
(`/v1/messages`) and OpenAI Chat Completions
(`/v1/chat/completions`).

The OpenAI Responses API (`/v1/responses`) is a
fundamentally different protocol with stateful
semantics and is out of scope for format
transformation. Responses API support is covered
separately.

The scope covers three capabilities:

1. **Classification and routing**: detect Anthropic
   Messages API requests by body structure and
   promote routing facts to headers for downstream
   filter chains and cluster selection. The
   classifier must distinguish Anthropic Messages
   from OpenAI Chat Completions even though both use
   a `messages` field, using discriminating signals:
   top-level `system` parameter, required
   `max_tokens`, `anthropic-version` header, and
   typed content blocks. Note: on Opus 4.7+ models,
   `"role": "system"` is now allowed inside the
   messages array as a mid-conversation system
   message, so role-based detection alone is not
   sufficient. The `anthropic-version` header is the
   strongest signal.

2. **Format transformation**: bidirectional conversion
   between Anthropic Messages and OpenAI Chat
   Completions so that clients speaking one dialect
   can reach backends speaking the other. This is
   validated by existing production implementations
   in OGX (the open-source agentic API server) which
   performs the same translation in Python. The known mapping
   rules are:

   **Request (Anthropic → OpenAI):**
   - `system` (top-level string or text block array)
     → prepended as OpenAI message with
     `role: "system"`
   - Content blocks → flattened:
     - `type: "text"` → string content
     - `type: "tool_use"` → OpenAI `tool_calls`
       with `function.arguments = JSON-serialized
       input`
     - `type: "tool_result"` → separate OpenAI
       message with `role: "tool"` (images in tool
       results promoted to follow-up user messages
       since OpenAI tool messages are text-only)
   - `max_tokens` → `max_tokens` (direct mapping)
   - `stop_sequences` → `stop`
   - `tool_choice`: `"any"` → `"required"`,
     `"none"` → `"none"`, default → `"auto"`,
     `{"type": "tool", "name": "X"}` →
     `{"type": "function", "function": {"name": "X"}}`
   - Tool definitions: custom tools convert
     (`input_schema` → `parameters`); server-side
     tools (web_search, bash, text_editor) are
     dropped with a log warning
   - `top_k` → no standard OpenAI equivalent;
     passed as extra body parameter for backends
     that support it (e.g. vLLM)
   - `temperature`, `top_p`, `top_k` → not
     supported on Opus 4.7+ models (returns 400);
     transformation must strip these when targeting
     newer Claude models
   - `thinking` blocks → dropped (no OpenAI
     equivalent)
   - Image blocks: Anthropic uses `type: "image"`
     with `source.type: "base64"|"url"|"file"`;
     OpenAI uses `type: "image_url"` with
     `image_url.url` — requires structural mapping

   **Response (OpenAI → Anthropic):**
   - `message.content` string → content block
     with `type: "text"`
   - `tool_calls` → content block per call with
     `type: "tool_use"` and `input = JSON-parsed
     arguments`
   - Finish reason: `"stop"` → `"end_turn"`,
     `"tool_calls"` → `"tool_use"`,
     `"length"` → `"max_tokens"`,
     `"content_filter"` → `"end_turn"`
   - `stop_reason: "refusal"` with `stop_details`
     (Opus 4.7+) → no direct OpenAI equivalent;
     map to `finish_reason: "stop"` with refusal
     metadata preserved in response headers or
     filter metadata
   - Usage: `prompt_tokens` → `input_tokens`,
     `completion_tokens` → `output_tokens`,
     `cached_tokens` → `cache_read_input_tokens`
   - Response ID generated as `msg_{uuid}`

   **Streaming (OpenAI chunks → Anthropic SSE):**
   1. Emit `MessageStartEvent` with empty content
   2. Per text delta: `ContentBlockStartEvent` +
      `ContentBlockDeltaEvent(text_delta)`
   3. Per tool call delta:
      `ContentBlockStartEvent` with empty
      `ToolUseBlock`, then
      `ContentBlockDeltaEvent(input_json_delta)`
   4. `ContentBlockStopEvent` to close each block
   5. `MessageDeltaEvent` with final `stop_reason`
      and usage
   6. `MessageStopEvent`

3. **Anthropic-native features**: proxy and preserve
   Anthropic-specific capabilities that have no
   OpenAI equivalent when routing to Anthropic
   backends in pass-through mode:
   - Prompt caching (`cache_control` blocks with
     `ephemeral` type and configurable TTL)
   - Extended thinking (`thinking` parameter with
     `budget_tokens`)
   - Citations in responses
   - Anthropic SSE streaming event protocol
   - `anthropic-version` header preservation
   - Rate-limit headers (`x-ratelimit-limit-tokens`,
     etc.)

Each capability is a separate filter implementing
`HttpFilter`, composable in YAML pipelines. Operators
deploy only what they need.

### Goals

- Classify Anthropic Messages API requests and
  promote `x-praxis-api-format: anthropic_messages`
  to headers for routing, extending the existing
  `AiRequestFormat` enum alongside `responses` and
  `chat_completions`.
- Transform requests bidirectionally between
  Anthropic Messages and OpenAI Chat Completions
  using the mapping rules documented above,
  validated against OGX's production implementation.
- Transform streaming responses between Anthropic
  SSE events (`message_start`, `content_block_start`,
  `content_block_delta`, `content_block_stop`,
  `message_delta`, `message_stop`) and OpenAI SSE
  chunks (`chat.completion.chunk`).
- Gracefully degrade when transforming Anthropic
  requests for OpenAI backends: drop unsupported
  features (thinking, server-side tools,
  `cache_control`) with structured log warnings
  rather than rejecting the request.
- Preserve Anthropic-specific headers and request
  features end-to-end when routing to backends
  that natively support `/v1/messages` (e.g. vLLM,
  Anthropic API).
- Provide a pass-through fast path for backends
  that natively support `/v1/messages` with
  sub-millisecond proxy overhead.
- Support credential injection for Anthropic
  backends using the existing `credential_injection`
  filter (inject `x-api-key` and
  `anthropic-version` headers per cluster).
- Enable unified gateway configurations where a
  single Praxis instance routes to vLLM (OpenAI),
  llm-d (OpenAI via vLLM), KServe/MaaS backends,
  and Anthropic API simultaneously, with automatic
  format detection and transformation.

## Why?

### Motivation

Production AI platforms increasingly need to support
multiple inference backends and API formats
simultaneously. The Anthropic Messages API is a
first-class inference protocol alongside OpenAI's
Chat Completions and Responses APIs, with
significant adoption in enterprise deployments.

Today, Praxis classifies requests as either
`responses` (OpenAI Responses API) or
`chat_completions` (OpenAI Chat Completions) in the
`AiRequestFormat` enum. Anthropic Messages requests
arrive with `messages` (like Chat Completions) but
are structurally different: `system` is a top-level
parameter, `max_tokens` is required, content uses
typed blocks (`text`, `image`, `tool_use`,
`tool_result`), and streaming uses a distinct SSE
event protocol. The current classifier would
misidentify these as `chat_completions`, leading to
incorrect routing or transformation failures.

The format transformation filters are needed because
real deployments mix backends:

- **vLLM and llm-d** expose OpenAI-compatible
  endpoints (`/v1/chat/completions`) and also
  support `/v1/messages` natively, but not all
  deployments enable Anthropic compatibility. vLLM
  additionally supports `/v1/responses`,
  embeddings, audio, and scoring endpoints. llm-d
  is a Kubernetes-native orchestration layer that
  routes to vLLM workers using the Gateway API
  Inference Extension with prefix-cache-aware
  scheduling and prefill/decode disaggregation.
- **KServe and MaaS** (Models as a Service) provide
  model discovery and API key management. MaaS
  returns model URLs that clients call directly;
  the model endpoints may implement either format.
  MaaS uses OpenAI-compatible API keys (`sk-oai-*`)
  and `/v1/models` for discovery.
- **Anthropic API** is the canonical backend for
  Claude models and uses a distinct wire format
  with features that have no OpenAI equivalent:
  prompt caching with `cache_control` blocks (5m
  and 1h TTL), extended thinking with
  `budget_tokens`, typed content blocks, and a
  block-based SSE streaming protocol. Anthropic
  also provides an OpenAI compatibility endpoint
  at `/v1/chat/completions`, but it lacks prompt
  caching, extended thinking details, and strict
  tool use — making native `/v1/messages` routing
  necessary for full feature access.

The bidirectional format transformation is a
validated pattern. OGX (the open-source agentic API
server) implements the same Anthropic ↔ OpenAI
mapping in production, with a native-passthrough
fast path when backends support `/v1/messages`
directly. The mapping rules documented in this
proposal are derived from that implementation and
cover the known edge cases: tool result image
promotion, server-side tool filtering, thinking
block handling, and streaming event sequencing.

Without format transformation, operators must either
standardize all clients on one format (impractical)
or run separate gateway instances per format
(operationally expensive). Praxis should handle this
at the filter pipeline level.

Anthropic-native features (prompt caching, extended
thinking) represent capabilities that cannot be
expressed in OpenAI format. When routing to
Anthropic backends, these must be preserved
end-to-end. When routing Anthropic requests to
OpenAI-compatible backends, the filters must
gracefully degrade: strip unsupported fields, map
what can be mapped, and log what was dropped.

### User Stories

- As a platform engineer, I want to route
  `/v1/messages` requests to vLLM backends that
  only support `/v1/chat/completions` so that
  clients using the Anthropic SDK can reach any
  backend in my fleet.
- As an AI gateway operator, I want a single Praxis
  instance to serve clients speaking OpenAI Chat
  Completions and Anthropic Messages formats,
  routing each to the appropriate backend with
  automatic format detection.
- As a developer, I want to send Anthropic-format
  requests with prompt caching to a Claude backend
  through Praxis without losing the `cache_control`
  blocks or `anthropic-version` header.
- As an SRE, I want to use Praxis credential
  injection to manage `x-api-key` headers for
  Anthropic backends the same way I manage
  `Authorization: Bearer` headers for OpenAI
  backends.
- As a platform engineer running llm-d with vLLM
  workers, I want clients using the Anthropic SDK
  to transparently reach my vLLM fleet with
  automatic request/response format translation.
- As a security engineer, I want Anthropic-specific
  rate-limit headers (`x-ratelimit-limit-tokens`,
  etc.) to be forwarded to clients so that
  client-side backoff works correctly.
- As a platform engineer using MaaS for model
  discovery, I want Praxis to detect whether a
  discovered model endpoint speaks Anthropic or
  OpenAI format and apply the appropriate
  transformation filters automatically.
