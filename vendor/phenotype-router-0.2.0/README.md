# phenotype-router

[![Crates.io](https://img.shields.io/crates/v/phenotype-router.svg)](https://crates.io/crates/phenotype-router)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![71-pillar: L9](https://img.shields.io/badge/71--pillar-L9-blue.svg)](https://phenotype.dev/pillars)

Phenotype-owned router decision layer (ADR-050 / ADR-051, §8 router-architecture
ACCEPTED 2026-06-20). This crate owns the **decision layer** of the router architecture —
the boundary where a `Request` is mapped to a `Response`.

This crate is **library-only** — it does not expose HTTP endpoints directly. It is the
decision-layer substrate that HTTP services (the future `phenotype-router` HTTP wrapper,
Civis, etc.) build on. The curl blocks below illustrate the typical HTTP wrapper
integration following the [L9 REST API conventions](#conventions) (RFC 7807 error envelope).

## Quickstart (Rust library)

```rust
use phenotype_router::{DecisionLayer, Request, Response, BifrostAdapter};

let adapter = BifrostAdapter::new();
let req = Request {
    id: "route.chat.completion".to_string(),
    payload: r#"{"model":"claude-opus-4","messages":[]}"#.to_string(),
};
let resp: Response = adapter.decide(&req);
match resp.decision {
    phenotype_router::Decision::Allow => println!("allowed"),
    phenotype_router::Decision::Deny(reason) => println!("denied: {reason}"),
}
```

## HTTP wrapper curl examples (illustrative)

When `phenotype-router` is wired into an HTTP service, the following curl blocks
demonstrate the [L9 REST API conventions](#conventions) (RFC 7807 error envelope). The
`<host>` and `<port>` placeholders must be replaced with the deploying service's values.

### Successful decision (POST 200)

```bash
curl -sS -X POST \
  -H "Accept: application/json" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"id":"route.chat.completion","payload":{"model":"claude-opus-4","messages":[]}}' \
  "https://<host>/api/v1/phenotype-router/decisions"
```

Successful response (HTTP 200):

```json
{
  "data": {
    "decision": "Allow",
    "trace": [
      ["adapter", "bifrost"],
      ["latency_ms", "12"]
    ]
  },
  "page_info": {"has_more": false}
}
```

### Denied decision (POST 200 with Deny)

```bash
curl -sS -X POST \
  -H "Accept: application/json" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"id":"route.forbidden.action","payload":{}}' \
  "https://<host>/api/v1/phenotype-router/decisions"
```

Response (HTTP 200, body carries the deny reason):

```json
{
  "data": {
    "decision": "Deny",
    "reason": "policy:model-not-allowed",
    "trace": [
      ["adapter", "bifrost"],
      ["policy_id", "model-not-allowed"]
    ]
  },
  "page_info": {"has_more": false}
}
```

### Validation error — RFC 7807 Problem Details (HTTP 400)

```bash
curl -sS -X POST \
  -H "Accept: application/problem+json" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{}' \
  "https://<host>/api/v1/phenotype-router/decisions"
```

Error response (HTTP 400):

```json
{
  "type": "https://phenotype.dev/errors/validation-failed",
  "title": "Validation Failed",
  "status": 400,
  "detail": "Field 'id' is required",
  "instance": "/api/v1/phenotype-router/decisions",
  "code": "VALIDATION_FAILED",
  "errors": [
    {"field": "id", "code": "REQUIRED", "message": "Field is required"}
  ]
}
```

### Rate-limited (HTTP 429)

```bash
curl -sS -i -X POST \
  -H "Accept: application/json" \
  -H "Content-Type: application/json" \
  "https://<host>/api/v1/phenotype-router/decisions"
```

Response headers:

```
HTTP/1.1 429 Too Many Requests
Content-Type: application/problem+json
Retry-After: 60
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1718947200
```

## API surface

| Item | Kind | Description |
|------|------|-------------|
| `DecisionLayer` | trait | Port trait every router-decision adapter must implement |
| `Request` | struct | Decision input (id, payload) |
| `Decision` | enum | `Allow` or `Deny(String)` |
| `Response` | struct | Decision output (decision + trace annotations) |
| `DecisionError` | enum | Error envelope (`Adapter(String)`) |
| `BifrostAdapter` | struct | Stub adapter (future Bifrost FFI bridge) |
| `HelloWorldPort` | trait | Parity-test fixture port |

## Error envelope mapping to RFC 7807

When HTTP wrappers convert `DecisionError` into HTTP responses, the mapping is:

| `DecisionError` variant | HTTP status | `code` (RFC 7807) |
|-------------------------|------------:|-------------------|
| `Adapter(_)`            | 502 | `ADAPTER_ERROR` |

Validation failures (missing `id`, malformed payload) are surfaced as RFC 7807
`validation-failed` BEFORE the decision layer is invoked (HTTP 400).

## Architecture (ADR-050 / ADR-051)

```
              HTTP wrapper (axum/warp)
                      │
                      ▼
       ┌──────────────────────────┐
       │   phenotype-router       │  ← this crate (Rust)
       │   DecisionLayer port     │
       └──────────────────────────┘
                      │
           ┌──────────┴──────────┐
           ▼                     ▼
    BifrostAdapter         HelloWorldPort
    (Go FFI stub)         (parity-test fixture)
```

- **ADR-050** — Bifrost-as-library integration boundary (decision layer).
- **ADR-051** — Phenotype-owned decision layer (this crate).

## Conventions

This crate follows the
[Phenotype REST API conventions](https://github.com/KooshaPari/phenotype-apps/blob/apps-extract/docs/conventions/rest-api.md):

- L9 — RFC 7807 Problem Details error envelope.
- L9.5 — `/openapi.json` published by HTTP wrappers.
- L9.7 — `Idempotency-Key` header on POST.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## REST API examples

The router decision layer is exposed over HTTP at `/v1/`. All requests require
a bearer token; all list endpoints use cursor pagination; all mutating
endpoints accept `Idempotency-Key`; all errors follow [RFC 7807 problem+json]
(see [`docs/conventions/rest-api.md`][convention] for the full contract).

### List registered plugins (cursor pagination)

```bash
curl -sS https://router.phenotype.dev/v1/plugins?limit=20 \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "data": [
    {"id": "rate-limit", "version": "1.2.0", "tier": "standard"},
    {"id": "smart-fallback", "version": "0.4.1", "tier": "internal"}
  ],
  "next_cursor": "eyJpZCI6InNtYXJ0LWZhbGxiYWNrIn0",
  "has_more": true
}
```

### Submit a routing decision (idempotent)

```bash
curl -sS https://router.phenotype.dev/v1/decisions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Idempotency-Key: 8d2e3a40-1f23-4f10-9b1f-3a8b8c0e1d22" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{
    "principal": "user:42",
    "intent": "weather",
    "context": {"region": "us-west-2"}
  }'
```

```json
{
  "decision": "allow",
  "plugin": "smart-fallback",
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736"
}
```

### Handle an RFC 7807 error

```bash
curl -sS -i https://router.phenotype.dev/v1/decisions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"principal": "user:42"}'
```

```http
HTTP/2 422
Content-Type: application/problem+json
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1719005460
```

```json
{
  "type": "https://phenotype.dev/probs/validation-failed",
  "title": "Validation failed.",
  "detail": "The 'intent' field is required.",
  "instance": "/v1/decisions",
  "status": 422,
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "errors": [
    {"field": "intent", "code": "missing", "message": "field is required"}
  ]
}
```

[RFC 7807 problem+json]: https://www.rfc-editor.org/rfc/rfc7807
[convention]: https://github.com/KooshaPari/phenotype-apps/blob/main/docs/conventions/rest-api.md

## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
