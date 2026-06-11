# Anthropic Messages API

Praxis supports the Anthropic Messages API
(`/v1/messages`) through five composable filters.
Operators can route, validate, and transform
Anthropic requests to reach any backend.

## Filters

| Filter | Purpose |
| ------ | ------- |
| `anthropic_messages_format` | Classify requests and promote routing facts to headers |
| `anthropic_validate` | Validate proxy-needed fields before forwarding |
| `anthropic_passthrough` | Header management for native `/v1/messages` backends |
| `anthropic_to_openai` | Bidirectional body transformation to `OpenAI` Chat Completions |
| `anthropic_stream_events` | SSE event transformation (per-chunk streaming, conformant with Inference Proxy Conformance Guidelines) |

## Passthrough to vLLM

Route Anthropic requests directly to a backend that
supports `/v1/messages` natively (e.g. vLLM with
Anthropic endpoint enabled).

```yaml
listeners:
  - name: gateway
    address: "0.0.0.0:8080"
    filter_chains: [anthropic]

filter_chains:
  - name: anthropic
    filters:
      - filter: anthropic_messages_format
        on_invalid: continue

      - filter: anthropic_validate

      - filter: anthropic_passthrough
        default_version: "2023-06-01"

      - filter: router
        routes:
          - path_prefix: "/"
            cluster: vllm

      - filter: load_balancer
        clusters:
          - name: vllm
            endpoints:
              - "127.0.0.1:8000"
```

Test:

```console
curl http://localhost:8080/v1/messages \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "openai/gpt-oss-20b",
    "max_tokens": 100,
    "system": "Reply concisely.",
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

## Passthrough to Anthropic API

Route to `api.anthropic.com` with credential
injection for the `x-api-key` header.

```yaml
listeners:
  - name: gateway
    address: "0.0.0.0:8080"
    filter_chains: [anthropic]

filter_chains:
  - name: anthropic
    filters:
      - filter: anthropic_messages_format
        on_invalid: continue

      - filter: anthropic_validate

      - filter: anthropic_passthrough
        default_version: "2023-06-01"

      - filter: headers
        request_set:
          - name: Host
            value: "api.anthropic.com"

      - filter: router
        routes:
          - path_prefix: "/"
            cluster: anthropic

      - filter: credential_injection
        clusters:
          - name: anthropic
            header: x-api-key
            env_var: ANTHROPIC_API_KEY
            strip_client_credential: true

      - filter: load_balancer
        clusters:
          - name: anthropic
            tls:
              sni: "api.anthropic.com"
            endpoints:
              - "api.anthropic.com:443"
```

Test:

```console
curl http://localhost:8080/v1/messages \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-haiku-4-5",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

## Transformation to OpenAI Backend

Transform Anthropic requests to OpenAI Chat
Completions format for backends that only speak
OpenAI (e.g. llm-d with disaggregation, KServe
without Anthropic support).

```yaml
listeners:
  - name: gateway
    address: "0.0.0.0:8080"
    filter_chains: [transform]

filter_chains:
  - name: transform
    filters:
      - filter: anthropic_messages_format
        on_invalid: continue

      - filter: anthropic_validate

      - filter: anthropic_to_openai
        max_body_bytes: 1048576

      - filter: path_rewrite
        replace:
          pattern: "^/v1/messages$"
          replacement: "/v1/chat/completions"
        conditions:
          - when:
              path_prefix: "/v1/messages"

      - filter: router
        routes:
          - path_prefix: "/"
            cluster: vllm

      - filter: load_balancer
        clusters:
          - name: vllm
            endpoints:
              - "127.0.0.1:8000"
```

The `anthropic_to_openai` filter:
- Hoists `system` to an OpenAI system message
- Flattens content blocks (text, image, tool_use,
  tool_result)
- Maps `stop_sequences` to `stop`,
  `tool_choice` semantics, tool definitions
- Preserves `top_k` as an extra body parameter
- Drops `thinking` blocks with a log warning
- Transforms the response back to Anthropic format
- Preserves original `finish_reason` in filter
  metadata as `openai.finish_reason`

## Filter Configuration Reference

### `anthropic_messages_format`

Classifies requests by body structure, path, and
`anthropic-version` header.

```yaml
filter: anthropic_messages_format
on_invalid: continue      # continue | reject
max_body_bytes: 1048576    # 1 MiB
headers:
  format: x-praxis-ai-format
  model: x-praxis-ai-model
  stream: x-praxis-ai-stream
```

Classification signals (in priority order):
1. `anthropic-version` request header
2. Request path is `/v1/messages`
3. Body has `messages` + `max_tokens` + `system`
4. Body has typed content blocks

### `anthropic_validate`

Validates proxy-needed fields. Role ordering
(e.g. first message must be `user`) is deferred
to the backend.

```yaml
filter: anthropic_validate
max_body_bytes: 1048576    # 1 MiB
```

Checks: `model` present and non-empty,
`max_tokens` present and > 0, `messages`
non-empty array.

### `anthropic_passthrough`

Injects `anthropic-version` header if absent.
No body transformation.

```yaml
filter: anthropic_passthrough
default_version: "2023-06-01"
```

### `anthropic_to_openai`

Bidirectional request/response transformation.
Non-streaming only; use `anthropic_stream_events`
for SSE responses.

```yaml
filter: anthropic_to_openai
max_body_bytes: 1048576    # 1 MiB
```

### `anthropic_stream_events`

Transforms OpenAI SSE chunks to Anthropic SSE
events. Processes SSE chunks incrementally as they arrive.

pending #143).

```yaml
filter: anthropic_stream_events
max_body_bytes: 1048576    # 1 MiB
```

## Running with Debug Logging

See filter activity in real time:

```console
RUST_LOG=debug cargo run -p praxis -- -c config.yaml
```

Filter-specific logging:

```console
RUST_LOG=praxis_filter::builtins::http::ai=debug cargo run -p praxis -- -c config.yaml
```
