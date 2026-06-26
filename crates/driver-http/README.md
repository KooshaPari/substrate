# driver-http

HTTP/REST inbound adapter for substrate. Exposes dispatch and planning APIs for non-Rust consumers (Go agentapi-plusplus, TS OmniRoute, Python driver-mcp).

Wires the same [`DispatchService`] and [`DispatchPlanner`] as [`driver-cli`], returning identical JSON shapes.

## Run

```bash
cargo run -p driver-http --bin substrate-http
```

| Variable | Default |
|----------|---------|
| `SUBSTRATE_HTTP_BIND` | `127.0.0.1:8080` |
| `SUBSTRATE_STATE_DIR` | `./.substrate` |
| `SUBSTRATE_HTTP_AUTH_TOKEN` | unset (no auth) |

## Endpoints

| Method | Path | Body | Response |
|--------|------|------|----------|
| GET | `/healthz` | — | `{ "status": "ok" }` |
| POST | `/v1/plan` | `{ "engine?", "cwd", "prompt", "mode?", "agent?", "resume?" }` | [`DispatchPlan`] JSON |
| POST | `/v1/dispatch` | same as plan | [`StructuredResult`] JSON (same as `substrate dispatch`) |
| POST | `/v1/route` | `{ "task": Task }` | [`RoutingDecision`] JSON |
| POST | `/v1/mailbox/send` | A2A [`Message`] | `201 Created` |
| GET | `/v1/mailbox/inbox?team=&to=` | — | `[Message]` |
| GET | `/v1/tasks?team=` | — | `[Task]` |

When `SUBSTRATE_HTTP_AUTH_TOKEN` is set, all routes except `/healthz` require `Authorization: Bearer <token>`.

## Example

```bash
curl -s localhost:8080/v1/plan \
  -H 'Content-Type: application/json' \
  -d '{"engine":"forge","cwd":"/tmp","prompt":"echo hi"}'
```

[`DispatchService`]: https://docs.rs/substrate-app/latest/substrate_app/struct.DispatchService.html
[`DispatchPlanner`]: https://docs.rs/substrate-app/latest/substrate_app/struct.DispatchPlanner.html
[`DispatchPlan`]: https://docs.rs/substrate-app/latest/substrate_app/struct.DispatchPlan.html
[`StructuredResult`]: https://docs.rs/substrate-core/latest/substrate_core/domain/struct.StructuredResult.html
[`RoutingDecision`]: https://docs.rs/substrate-core/latest/substrate_core/domain/struct.RoutingDecision.html
[`Message`]: https://docs.rs/a2a/latest/a2a/struct.Message.html
[`driver-cli`]: ../driver-cli/
