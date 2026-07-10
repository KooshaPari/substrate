# Service Level Objectives

> **Audience:** Operators, on-call engineers, and stakeholders.
> **Scope:** `psub-gateway` (OpenAI-compatible surface) and `driver-http` (dispatch surface).

---

## 1. Service Level Indicators (SLIs)

| Indicator | Definition | Collection |
|---|---|---|
| **Request latency (p50)** | Median HTTP request duration | `psub_gateway_request_duration_ms_bucket` histogram |
| **Request latency (p99)** | 99th percentile HTTP request duration | Same histogram |
| **Error rate** | `(5xx responses) / (total responses)` | `psub_gateway_errors_total{status="5xx"}` / `psub_gateway_requests_total` |
| **Throughput** | Requests per second | `rate(psub_gateway_requests_total[1m])` |
| **Upstream availability** | Fraction of upstream LLM providers reachable | `/health/providers` → per-provider `enabled` flag |
| **Budget exhaustion** | Sessions hitting token/cost cap | `budget::SessionBudgetSnapshot` per session |

---

## 2. Service Level Objectives (SLOs)

| SLO | Target | Measurement window | Severity |
|---|---|---|---|
| **Latency (p50)** | ≤1 000 ms | 28 days rolling | Warning |
| **Latency (p99)** | ≤10 000 ms | 28 days rolling | Critical |
| **Error rate** | ≤1% (5xx / total) | 28 days rolling | Critical |
| **Availability** | ≥99.5% (Uptime) | 28 days rolling | Critical |
| **Throughput** | ≥100 req/s sustained | 5 min window | Warning |
| **Upstream coverage** | ≥2 providers reachable at all times | 1 min window | Critical |

---

## 3. Error Budget

| SLO | Budget per 28 days | Burn rate before alert |
|---|---|---|
| Availability (99.5%) | ~3.4 hours of downtime | 10× (20 min before alert) |
| Error rate (1%) | ~6 h 43 min of bad requests | 10× (40 min before alert) |

**Burn rate alert rules:**

- **Page (P0):** Burn rate > 10× for ≥ 5 min
- **Warning (P1):** Burn rate > 5× for ≥ 30 min
- **Info:** Burn rate > 2× for ≥ 2 hours

---

## 4. Monitoring

### Prometheus metrics (from `/metrics/prometheus`)

| Metric | Type | Description |
|---|---|---|
| `psub_gateway_requests_total` | Counter | Total requests by provider, model, status |
| `psub_gateway_request_duration_ms_bucket` | Histogram | Latency distribution (p50/p90/p99) |
| `psub_gateway_errors_total` | Counter | Error count by status code |
| `psub_gateway_rate_limit_hits_total` | Counter | Rate-limit rejections by provider |
| `psub_gateway_budget_exhausted_total` | Counter | Budget-cap exceedances |

### Recommended alerts (PromQL)

```promql
# P0: High error rate
sum(rate(psub_gateway_errors_total{status=~"5.."}[5m])) / sum(rate(psub_gateway_requests_total[5m])) > 0.01

# P1: High p99 latency
histogram_quantile(0.99, rate(psub_gateway_request_duration_ms_bucket[5m])) > 10000

# P1: Budget exhaustion spike
rate(psub_gateway_budget_exhausted_total[5m]) > 10

# Info: Provider unavailable
psub_gateway_provider_up{provider="openai"} == 0
```

---

## 5. Review cadence

| Artifact | Frequency | Owner |
|---|---|---|
| SLO attainment report | Monthly | On-call lead |
| Error budget review | Weekly (standup) | Team |
| Burn rate incident review | Per incident | On-call rotation |
| SLO target adjustment | Quarterly | Engineering manager |

---

## 6. Exceptions

Any SLO can be temporarily relaxed by filing an exception in the team's
tracking system. Exceptions expire after 30 days unless renewed. All
exceptions are reviewed during the quarterly SLO adjustment.
