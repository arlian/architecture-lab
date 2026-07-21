# architecture-lab — Event-Driven in Rust

The same little e-commerce domain as the [modular monolith](../modular-monolith)
and [microservices](../microservices) labs, rebuilt a third way: services that
never call each other directly, only publish and subscribe to events through a
broker (NATS). Reading all three side by side is the point — same domain
logic, three very different answers to "how do the pieces find out about each
other?"

## The core idea

Where microservices enforced boundaries with the *network* via synchronous
request/response, event-driven enforces them the same way but flips the
direction of dependency:

- **No service calls another service.** Not even by URL. Every service only
  ever talks to the broker (`NATS_URL`). Grep this codebase for one service's
  name inside another's source — you won't find it, unlike microservices'
  `USERS_URL` / `CATALOG_URL`.
- **Producers don't know their consumers.** Users publishes `UserRegistered`
  whether zero, one, or ten services are listening. Adding
  `notifications-service` required editing exactly one new file — zero
  changes to `orders-service`, the producer of the event it consumes.
- **Consumers keep their own local copy of what they need.** Orders can't ask
  "does this user exist?" over the wire any more — there's no synchronous
  channel to ask it on. Instead it subscribes to `UserRegistered` /
  `ProductCreated` / `ProductPriceChanged` and maintains a tiny local
  **read model** (`orders-service/src/read_model.rs`), the same "narrow
  contract, not an implementation" seam as the other two labs, just built by
  projecting events instead of injecting a client.
- **The tax: eventual consistency.** An HTTP call is stale by however long the
  round-trip takes (milliseconds). An event-driven read model is stale by
  however long event delivery + processing takes — and right after a user
  registers, there is a real window where Orders would still reject an order
  for that user id. That window is the new failure mode this pattern asks you
  to accept, in exchange for producers and consumers never being coupled at
  request time.

## Layout

```
event-driven/
└── services/
    ├── users-service/          # owns users, publishes UserRegistered        :3001
    ├── catalog-service/        # owns products & prices, publishes
    │                           #   ProductCreated / ProductPriceChanged      :3002
    ├── orders-service/         # local read model + publishes OrderPlaced    :3003
    └── notifications-service/  # pure consumer of OrderPlaced, no HTTP at all
```

Each HTTP-facing service keeps the same internal layering as the other two
labs — `domain` / `repository` / `service` / `http` — plus two new files:
`events.rs` (the wire shapes it publishes and/or consumes) and `bus.rs` (the
`EventBus` port + its NATS adapter, mirroring the old `UserDirectory` /
`ProductCatalog` ports). Orders additionally has `read_model.rs` (its local
projection) and `projection.rs` (the subscriptions that keep it in sync).

## What changed vs. microservices

| Concern | Microservices | Event-driven |
| --- | --- | --- |
| Cross-service call | HTTP request to a neighbour's URL | publish an event; no direct call at all |
| Who initiates | the caller, synchronously | the producer, whenever something happened |
| Can it fail per-request? | yes — every call is a `Result` | no per-request network call to fail; staleness replaces it |
| How Orders validates a user/product | ask Users/Catalog live, every time | check its own local read model, built from past events |
| Coupling | Orders knows Users' and Catalog's URLs | no service names another service anywhere |
| Adding a new consumer | edit the producer to add a new outbound call | add a new subscriber; zero producer changes |
| Wire contract | request/response JSON per endpoint | one-way JSON events per subject, still copied per-consumer |

The two design pillars from the earlier labs still survive the jump:

1. **Data ownership.** Orders still stores only a `user_id`/`product_id`, never
   a copy of the user or product record. It just now learns *whether an id is
   any good* from a stream of events instead of a live query.
2. **Depend on a narrow contract, not an implementation.** `OrderService` in
   `service.rs` depends on `ReadModel` (a plain in-memory projection, no
   socket) and the `EventBus` trait — never on `async_nats::Client` directly.
   Its unit tests build a `ReadModel` and feed it facts by hand, and use a fake
   `EventBus` that just records what was published. No test ever touches a
   socket, same as the other two labs.

## How it actually flows

Placing an order needs the same two answers as always. Neither is a network
call any more:

```rust
// orders-service/src/service.rs — no `?` on these two: a local read can't
// fail the way a socket call could. It can only be stale.
if !self.read_model.user_exists(cmd.user_id.0).await {
    return Err(AppError::Validation("user ... does not exist".into()));
}
let unit_price_cents = self.read_model.price_of(id).await
    .ok_or_else(|| AppError::Validation("product ... does not exist".into()))?;
```

That local state is kept warm by `projection.rs`, which subscribes to three
subjects at startup and updates `read_model.rs` as events arrive:

```
users-service     --publishes-->  users.registered                --+
catalog-service   --publishes-->  catalog.product_created          --+--> orders-service's
catalog-service   --publishes-->  catalog.product_price_changed    --+     read model
```

And placing an order announces itself the same way, with nobody upstream
knowing who's downstream:

```
orders-service --publishes--> orders.placed --> notifications-service (logs a "sent" email)
                                            --> (add your own subscriber here)
```

## A known gap: dual writes

Each service does a plain in-memory insert, then a best-effort NATS publish
(logged and swallowed on failure — see the `tracing::warn!` calls in every
`service.rs`). Those two steps aren't atomic: if the process crashes between
them, or the publish itself fails, the write happened but the event never
went out, and every downstream read model silently drifts out of sync. A real
system closes this gap with a **transactional outbox** (write the event to the
same datastore/transaction as the entity, then a separate relay publishes it)
or a durable log the writer can safely retry against (NATS **JetStream**,
Kafka). This lab uses plain NATS core specifically so that gap stays visible —
see "where to take it next" below.

## Running it

Requires the Rust toolchain and a NATS server. The simplest way to get NATS
locally, even if you're running the Rust services natively:

```bash
docker run --rm -p 4222:4222 nats:2-alpine
```

Then, from this directory, open four terminals:

```bash
# terminal 1 — users
cargo run -p users-service              # http://localhost:3001

# terminal 2 — catalog (seeds two products on startup)
cargo run -p catalog-service             # http://localhost:3002

# terminal 3 — notifications (no HTTP port; watch its log lines)
cargo run -p notifications-service

# terminal 4 — orders (subscribes to users/catalog events on startup)
cargo run -p orders-service               # http://localhost:3003
```

On Windows, start NATS plus all four services at once:

```powershell
./run-all.ps1                    # opens each in its own window; NATS via Docker
```

Run the tests (each service independently, no broker required — the fakes
never open a socket) and lint the lot:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build        # NATS + 4 service images, one network
```

Notice `docker-compose.yml` never wires one service's URL into another's
environment the way the microservices compose file wires `USERS_URL` into
Orders — every service only gets `NATS_URL`.

### Try the API

```bash
# health of each HTTP-facing service (notifications has none)
curl localhost:3001/health
curl localhost:3002/health
curl localhost:3003/health

# create a user (on the users service) — this publishes UserRegistered
curl -X POST localhost:3001/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'

# list the seeded products (on the catalog service) — grab a product id
curl localhost:3002/products

# place an order (on the orders service) — validated against its own
# read model, built from the events above
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"<USER_ID>","lines":[{"product_id":"<PRODUCT_ID>","quantity":2}]}'
```

**See the trade-off in action:** place an order for a user *immediately* after
creating it — there's a small chance you'll hit the eventual-consistency
window and get a `400` saying the user doesn't exist, even though `POST
/users` already returned `200`. Wait a moment and retry; it'll succeed once
the event's been projected. Compare this to the microservices version, where
that same check was always instantly correct (or a clean `500` if Users was
down) — never almost-right-but-not-yet.

Then change a price and watch Orders notice without you telling it to:

```bash
curl -X PUT localhost:3002/products/<PRODUCT_ID>/price \
  -H 'content-type: application/json' \
  -d '{"price_cents": 1500}'

# place another order for the same product — the new price applies
```

And watch the notifications window: every successful order triggers a log
line there, with zero code in orders-service aware that notifications-service
exists.

## Where to take it next (learning exercises)

- **Close the dual-write gap.** Add a transactional outbox (write the event
  row in the same in-memory/DB transaction as the entity, relay it separately)
  or switch to NATS **JetStream** for persistent, replayable streams with
  acks — then kill orders-service mid-run and watch it catch up on restart
  instead of missing events forever.
- **Give the read model a cold-start story.** Right now a fresh orders-service
  only learns about users/products created *after* it started listening.
  Replay (JetStream) or a snapshot/backfill endpoint on Users/Catalog would
  fix this — try it.
- **Add a saga for multi-step consistency.** If placing an order needs to
  reserve stock in an `inventory-service`, you now have a distributed
  transaction with no 2PC. Model it explicitly as a saga (orchestrated or
  choreographed) with compensating events for the failure path.
- **Add a dead-letter path.** Right now a handler that can't deserialize a
  payload just logs a warning and drops the message (see the `Err` arms in
  `projection.rs`). Decide what should happen instead.
- **Give each service a real, persistent broker and database** (JetStream or
  Kafka; `sqlx` + Postgres per service) so restarts don't lose everything.
- **Harden the containers**, same exercise as the other two labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
