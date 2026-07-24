# architecture-lab — CQRS + Event Sourcing in Rust

The same little e-commerce domain as the other three labs, taken one step
further than [event-driven](../event-driven): Orders is now split into two
independent deployables — one that only accepts commands and appends events,
one that only answers queries from a projection. Users, Catalog, and
Notifications are unchanged from the event-driven lab; this lab is entirely
about what happens when you take Orders' "local read model built from events"
idea and push it all the way, on both sides.

## The core idea

Event-driven asked "how do services find out about each other?" and answered
with a broker. CQRS + event sourcing asks a narrower, sharper question about a
**single** bounded context: "does the model you write to have to be the model
you read from?" Here, no:

- **The write side has no queries.** `orders-command-service` cannot answer
  "show me all orders" — it doesn't even have the concept of a full order
  record, only an append-only log per order id (`event_store.rs`) and a fold
  function that replays it into current state on demand (`aggregate.rs`).
- **The read side has no writes.** `orders-query-service` cannot place, pay,
  ship, or cancel anything. It has no business rules, no validation, no
  concept of what transitions are legal — it just folds the events the
  command side publishes into a `HashMap<OrderId, OrderView>` and serves reads
  off it.
- **State is a replay, not a record.** In every other lab, "what's this
  order's status?" is a field read. Here it's the result of folding history:
  `OrderState::fold(&events)`. There is no other way to ask.
- **The write side gets an audit trail for free.** `GET
  /orders/:id/history` on the command service returns the literal sequence of
  events that produced the current state — something no other lab in this
  repo can offer, because none of them keep the "diff" around after applying it.
- **The tax: two things to keep in sync, and no shared timeline.** The query
  side's projection is now a second place order data lives, built
  asynchronously from what the command side publishes. If it's down, restarts,
  or a message is dropped, it silently drifts from what the command side's
  event store actually contains. Nothing in this lab reconciles the two
  automatically — see "a known gap" below.

## Layout

```
cqrs-es/
└── services/
    ├── users-service/            # unchanged from event-driven                :3001
    ├── catalog-service/          # unchanged from event-driven                :3002
    ├── orders-command-service/   # writes: event-sourced aggregate            :3003
    ├── orders-query-service/     # reads: projection of the same events       :3004
    └── notifications-service/    # unchanged from event-driven, no HTTP at all
```

`orders-command-service` layering:

| File | Role |
| --- | --- |
| `aggregate.rs` | `OrderEvent`, `OrderStatus`, `OrderState::fold` — pure, no I/O |
| `event_store.rs` | the append log: `load(id)` replays, `append(id, event)` extends |
| `events.rs` | the four events published outward (`OrderPlaced`/`Paid`/`Shipped`/`Cancelled`) |
| `read_model.rs` + `projection.rs` | unchanged from the event-driven lab: local knowledge of which users/products are valid, used only to validate `place` |
| `service.rs` | command handlers: load → fold → validate → append → publish |
| `http.rs` | `POST /orders`, `POST /orders/:id/{pay,ship,cancel}`, `GET /orders/:id/history` |

`orders-query-service` layering — deliberately much smaller:

| File | Role |
| --- | --- |
| `events.rs` | its own copy of the four inbound event shapes |
| `view.rs` | `OrderView` + the in-memory projection (`SharedOrderViews`) |
| `projection.rs` | subscribes to the four subjects, updates the projection |
| `http.rs` | `GET /orders` (with an optional `?user_id=` filter), `GET /orders/:id` — no writes |

## What changed vs. event-driven

| Concern | Event-driven | CQRS + event sourcing |
| --- | --- | --- |
| Orders' own state | a `HashMap<OrderId, Order>`, mutated in place | an append-only `Vec<OrderEvent>` per id, replayed to get state |
| "What's this order's status?" | a field read | `OrderState::fold(&history)` |
| Read and write | same service, same process | two separate deployables, no shared code or process |
| Query flexibility | whatever the one service's repository supports | the read side can add query shapes (like `?user_id=`) the write side never needed |
| Audit trail | none — history isn't kept once applied | free — the event store IS the history |
| New failure mode | staleness in the read model used for *validation* | staleness in the read model used for *every query the outside world sees*, plus a whole second store to keep in sync |

## A known gap: no cold-start replay

If `orders-query-service` restarts, its projection is gone, and it can only
rebuild from events published *after* it reconnects — plain NATS core has no
history to replay. Meanwhile `orders-command-service` is sitting on the full,
authoritative event history for every order and has no way to hand it over.
This is the sharpest version of a gap the event-driven lab's README already
flagged for its simpler read model. Two ways to close it (good exercises,
see below): give the command service a `GET /internal/replay` that dumps
every event in id-then-sequence order for the query side to call once at
startup before subscribing live, or swap NATS core for **JetStream**, which
keeps history and lets a fresh subscriber ask for it.

## Running it

Requires the Rust toolchain and a NATS server:

```bash
docker run --rm -p 4222:4222 nats:2-alpine
```

Then, from this directory, open five terminals:

```bash
cargo run -p users-service               # http://localhost:3001
cargo run -p catalog-service              # http://localhost:3002 (seeds two products)
cargo run -p notifications-service        # no HTTP; watch its log lines
cargo run -p orders-query-service         # http://localhost:3004
cargo run -p orders-command-service       # http://localhost:3003
```

On Windows, start NATS plus all five at once:

```powershell
./run-all.ps1
```

Tests need no broker — every service's fakes stay socket-free, same as the
other labs:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build
```

### Try the API

```bash
# create a user and grab a product id, same as the other labs
curl -X POST localhost:3001/users -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'
curl localhost:3002/products

# place an order — on the COMMAND service
curl -X POST localhost:3003/orders -H 'content-type: application/json' \
  -d '{"user_id":"<USER_ID>","lines":[{"product_id":"<PRODUCT_ID>","quantity":2}]}'
# → note the returned "id" and "status": "placed"

# walk it through its lifecycle — still the command service
curl -X POST localhost:3003/orders/<ORDER_ID>/pay
curl -X POST localhost:3003/orders/<ORDER_ID>/ship

# try an illegal transition — this should 400
curl -X POST localhost:3003/orders/<ORDER_ID>/cancel

# see the raw event log for that order — only the command service can show you this
curl localhost:3003/orders/<ORDER_ID>/history

# now read it back — on the QUERY service, a different port, a different process
curl localhost:3004/orders/<ORDER_ID>
curl localhost:3004/orders                       # every order
curl "localhost:3004/orders?user_id=<USER_ID>"    # a query the command side never offered
```

**See the trade-off in action:** place an order, then immediately `curl
localhost:3004/orders/<ORDER_ID>` — there's a small window where the query
service hasn't projected the `OrderPlaced` event yet and you'll get a `404`
even though the command service already returned `200` with the order. Retry
and it appears. That gap is structural, not a bug: two processes, connected
only by an event stream, will never be perfectly synchronized.

## Where to take it next (learning exercises)

- **Close the cold-start gap.** Add a replay endpoint or switch to NATS
  JetStream, as described above — then kill and restart
  `orders-query-service` and watch it recover instead of starting empty.
- **Add optimistic concurrency to the event store.** `EventStore::append` in
  this lab just pushes — two concurrent commands against the same order could
  both read the same history and both decide their transition is legal,
  silently losing one. Add an `expected_version` parameter and reject stale
  appends.
- **Add a snapshot.** Folding a short-lived order's history is cheap; folding
  one with thousands of events on every single command isn't. Store a
  periodic `OrderState` snapshot plus "events since," and fold only the tail.
- **Give the query side more read models.** Add a second projection over the
  same four events — e.g. a running per-product sales count — to feel how
  cheap it is to add a new query shape when you're not also carrying the
  write model's constraints.
- **Make the event store durable.** Right now a process restart of
  `orders-command-service` loses every order ever placed — there's no
  persistence at all. `sqlx` + an append-only Postgres table (or a real event
  store) is the natural next step.
- **Harden the containers**, same exercise as every other lab here:
  healthchecks, `condition: service_healthy`, non-root users, pinned digests.
