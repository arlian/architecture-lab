# architecture-lab — Fan-out / fan-in (scatter-gather) in Rust

Branches off [bff](../bff), which was the first lab where one inbound request
became several outbound ones. `web-bff` fans out to three backend services and
waits for all of them. This lab asks the question that fan-out leaves behind:

> **when one request becomes N, what do you return if only some of them come
> back?**

Every other lab in this repo has an answer that amounts to "don't be in that
situation". This one is built entirely around being in it.

## The core idea

A gateway takes `GET /search?q=mug` and scatters it to every registered search
provider at once, then gathers whatever arrived before the deadline into one
ranked answer.

- **The fan-out is homogeneous, so it's a list, not a wiring diagram.** Every
  provider answers the same question with the same wire shape, so the gateway
  holds one trait and a `Vec` of it (`provider.rs`). Which providers exist is
  the `PROVIDERS` environment variable — `name=url,name=url` — read at startup.
  Adding a fourth is a config change. Compare `bff/services/web-bff/src/clients.rs`:
  three hand-written traits, three fields, fixed at compile time, because those
  three services answer three *different* questions.
- **Every branch is isolated: its own timeout, its own `Result`.** No branch can
  fail, hang, or slow another. The join is `join_all`, deliberately not the
  `try_join_all` that `web-bff` uses — that one aborts on the first `Err` and
  fails the client's request. One word, opposite failure semantics.
- **There is a deadline, and it is spent concurrently.** `BUDGET_MS` (default
  500) bounds the *whole* scatter, not each branch, so the request costs roughly
  the budget rather than `n × budget`. A provider that misses it is dropped from
  this answer and its in-flight request abandoned.
- **Completeness is part of the payload.** The response carries a
  `providers: [...]` report — one entry per registered provider, in registry
  order, always — saying `ok` / `timed_out` / `failed`, how long it took, and
  how many hits it contributed. Plus a `degraded` flag summarising it. A caller
  can tell a thin answer from a complete one.
- **The tax: the answer is no longer deterministic, and someone has to rank
  it.** Two runs of the same query legitimately return different result sets.
  And because no provider can see any other's results, cross-provider ranking,
  dedupe, and identity all become the gateway's problem — problems that
  simply do not exist when one service owns the whole index.

## Layout

```
fan-out/
└── services/
    ├── search-gateway/       # the scatter-gather                     :3010
    ├── catalog-provider/     # in-house range — fast, well-behaved     :3011
    ├── partner-provider/     # third-party — always 800ms (> budget)   :3012
    └── archive-provider/     # discontinued lines — 503s half the time :3013
```

The three providers are near-identical small services (`index` / `http` /
`error` / `main`), each with its own private corpus. Their differences are
deliberately not in their domain logic:

| Provider | What makes it interesting | Knob |
| --- | --- | --- |
| catalog | nothing — it just works, so something always comes back | — |
| partner | stalls before answering, so it loses the race | `LATENCY_MS` (800) |
| archive | returns 503 on a random fraction of searches | `FAILURE_RATE` (0.5) |

Both misbehaviours are *real*: a real socket left open past the deadline, a real
5xx from a real handler. Nothing is mocked, which is why partner-provider's log
shows it finishing work for a request the gateway stopped listening to.

The gateway is four files, and only two of them matter:

```
search-gateway/src/
├── provider.rs   # the one port: SearchProvider + its HTTP adapter + the registry
├── scatter.rs    # THE FILE — the fan-out, the deadline, the merge, the report
├── http.rs       # GET /search?q=
└── main.rs       # reads PROVIDERS and BUDGET_MS, wires the fan-out, serves
```

## What changed vs. BFF

| Concern | BFF (`web-bff`) | Fan-out (`search-gateway`) |
| --- | --- | --- |
| Shape of the fan-out | heterogeneous — 3 different services, 3 traits | homogeneous — N interchangeable providers, 1 trait |
| How wide is it? | fixed at compile time | `PROVIDERS` env var, read at startup |
| Adding an upstream | new trait + new field + edit the aggregation | add one entry to a string |
| The join | `try_join_all` — first `Err` aborts the rest | `join_all` — every branch settles independently |
| One upstream is down | whole request 500s | that branch reports `failed`; request is 200 |
| One upstream is slow | whole request waits for it | that branch reports `timed_out` at the budget |
| Deadline | none | `BUDGET_MS`, spent concurrently across all branches |
| Does the client know what it got? | no — success or nothing | yes — `degraded` + a per-provider report |
| Merging results | typed composition; each upstream fills a different field | ranking, dedupe and identity across peers |

Everything the earlier labs established still holds: providers own their data,
the gateway owns none, and the gateway depends on a narrow trait rather than on
`reqwest` — its tests build fake providers that return, fail, and stall on
command, and never open a socket.

## How it actually flows

```
                        ┌──> catalog-provider   3ms   ok         2 hits
GET /search?q=mug ──────┼──> partner-provider   500ms timed_out   (dropped)
   (budget 500ms)       └──> archive-provider   4ms   failed      (503)
                                     │
                                     ▼
                            merge · dedupe · rank
                                     │
                                     ▼
                    200 { degraded: true, hits: [...], providers: [...] }
```

The scatter is unremarkable. The gather is where every decision lives, and
`scatter.rs` makes three of them out loud:

```rust
// 1. isolation — a timeout per branch, and no `?` anywhere near a branch error
let outcome = timeout(budget, provider.search(&query)).await;

// 2. join_all, NOT try_join_all: let every branch settle however it likes
let settled = join_all(branches).await;

// 3. the failure becomes a field, not a return
Ok(Err(err)) => (ProviderReport { status: Failed, error: Some(..), .. }, Vec::new()),
```

There's a fourth decision hiding in the merge, and it's the one worth staring
at. catalog-provider and archive-provider both list a "Coffee Mug" — at
different prices, under **different ids**, because no shared identity for a
product exists anywhere in this system. So the gateway dedupes on lower-cased
name and keeps the cheaper price. That is a bad rule, chosen on purpose: fan-in
*always* needs an identity story, and if the providers don't give you one, the
gateway invents one whether you thought about it or not.

The type system is doing some of the work here too. A branch failure is
`ProviderError`, a type that is deliberately *not* the gateway's `AppError` and
has no `From` impl to it — so no `?` can ever quietly promote one dead provider
into a failed request. The policy is enforced at compile time rather than by
remembering.

## The argument to have

**This gateway returns 200 when every single provider failed.** The body is
honest — `degraded: true`, `hits: []`, three reports explaining exactly what
happened — but the status line says success.

That is defensible: the gateway did its job, and "no results" is a real answer
to a search. It's also how most production search fan-outs behave, because the
alternative is that one flaky dependency takes the feature down.

It's also arguably wrong, and worth changing on purpose to feel the difference:

- **A quorum.** Succeed if ≥ half the providers answered, 503 otherwise.
- **Named essential providers.** catalog failing is a 503; archive failing is a
  footnote. This is usually the honest model, and it needs one more piece of
  config.
- **`206 Partial Content`**, semantically nice and widely misunderstood by
  clients and proxies alike.
- **Always 200, but with a `Warning` header** so caches and middleboxes see the
  degradation without parsing the body.

Pick one and implement it — it's a ten-line change in `scatter.rs` and `http.rs`,
and the argument is the actual lesson.

## A known gap: nothing backs off

Every request scatters to every provider, every time, no matter how the last
hundred requests went. archive-provider can be failing 100% of requests and the
gateway will keep dialling it, paying a connection and a timeout for nothing —
and worse, keeping load on a service that may be failing *because* it's
overloaded.

The standard fix is a **circuit breaker**: after N consecutive failures, stop
calling that provider for a cooling-off period and report it as `open` without
dialling, then let a trial request through to see if it recovered. Related, the
timed-out branch here is genuinely abandoned mid-flight — the gateway stops
waiting but the provider keeps working, so a slow provider under load gets
*more* load, not less. This lab leaves both gaps wide open so they stay visible;
see below.

## Running it

Requires only the Rust toolchain — no broker, no database, no Docker.

From this directory, open four terminals:

```bash
# terminal 1 — the fast one
cargo run -p catalog-provider     # http://localhost:3011

# terminal 2 — the slow one (800ms, above the gateway's 500ms budget)
cargo run -p partner-provider     # http://localhost:3012

# terminal 3 — the flaky one (503s half the time)
cargo run -p archive-provider     # http://localhost:3013

# terminal 4 — the gateway
cargo run -p search-gateway       # http://localhost:3010
```

On Windows, start all four at once:

```powershell
./run-all.ps1
```

Run the tests (every service independently; the gateway's scatter-gather is
exercised through fake providers that return, fail, and stall on command, so no
test opens a socket) and lint the lot:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build
```

Notice that exactly one service's environment in `docker-compose.yml` names any
other service — the gateway's `PROVIDERS`. The providers are configured as if
they had no idea anything was fanning out to them, because they don't.

### Try the API

```bash
curl "localhost:3010/search?q=mug"
```

```json
{
  "query": "mug",
  "degraded": true,
  "hits": [
    { "id": "...", "name": "Coffee Mug", "price_cents": 1299,
      "sources": ["catalog"] },
    { "id": "...", "name": "Travel Mug", "price_cents": 1899,
      "sources": ["catalog"] }
  ],
  "providers": [
    { "name": "catalog", "status": "ok",        "took_ms": 2,   "hits": 2 },
    { "name": "partner", "status": "timed_out", "took_ms": 501, "hits": 0,
      "error": "did not answer within the 500ms budget" },
    { "name": "archive", "status": "failed",    "took_ms": 3,   "hits": 0,
      "error": "returned 503 Service Unavailable: ..." }
  ]
}
```

**Run it several times.** Nothing about the request changes, but the answer
does: archive is up on about half the runs, and when it is, the Coffee Mug drops
to `999` with `"sources": ["catalog", "archive"]` — the merge picking the cheaper
of two listings for the same product. The status code is `200` every time.

Now watch partner-provider's window: its `search (after stalling on purpose)`
line appears *after* your curl already returned. That's the abandoned request —
work the system paid for and threw away.

Then move the deadline and change nothing else:

```bash
# a budget generous enough for the slow provider
BUDGET_MS=1200 cargo run -p search-gateway
curl "localhost:3010/search?q=mug"     # partner is suddenly `ok`, with 2 hits
```

Or move the provider instead:

```bash
LATENCY_MS=100 cargo run -p partner-provider    # gateway untouched, not restarted
```

Either way the same code produces a different answer, which is the thing to
take away: in a fan-out, **who is in the response is a runtime property of the
deadline, not a static property of the system.**

Finally, shrink the fan-out to prove the registry is real:

```bash
PROVIDERS=catalog=http://localhost:3011 cargo run -p search-gateway
curl "localhost:3010/search?q=mug"     # one report, degraded: false
```

...and compare with hitting a provider directly, which knows nothing about any
of this:

```bash
curl "localhost:3011/search?q=mug"
```

## Where to take it next (learning exercises)

- **Add a circuit breaker.** Track consecutive failures per provider, skip a
  provider that's tripped, report it as `open` without dialling, and let a trial
  request through after a cooling-off period. This is the single biggest gap in
  the lab.
- **Change the completeness policy.** Implement one of the options from "the
  argument to have" above — a quorum, essential-vs-optional providers, or 206 —
  and notice how much of the decision is business policy rather than plumbing.
- **Return the first N answers instead of waiting for the budget.** Swap
  `join_all` for `FuturesUnordered` and stop early once you have enough hits, or
  once the fast providers have all answered. That's *hedging*, and it changes
  the latency profile completely — and makes the result even less deterministic.
- **Give the results a real identity.** Replace name-based dedupe with a shared
  product identifier (a GTIN-style key the providers agree on) and watch how
  much of the merge logic evaporates — and how much coordination between
  independent services that agreement actually costs.
- **Rank on relevance, not price.** Score hits in each provider, then reconcile
  scores across providers that have never agreed on what a score means. This is
  the hard, real problem behind every federated search system.
- **Add a fourth provider without recompiling the gateway.** Write it in any
  language, expose `GET /search?q=`, add it to `PROVIDERS`. If you have to touch
  the gateway's code, the homogeneous port has leaked.
- **Propagate the deadline downstream.** Send the remaining budget as a header
  and have providers give up on their own work when it's spent, instead of the
  gateway abandoning requests that keep running.
- **Harden the containers**, same exercise as the other labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
