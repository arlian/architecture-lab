# architecture-lab — Saga in Rust

The same little e-commerce domain as the other three labs, taken one step
further than [event-driven](../event-driven): placing an order now involves
a real **distributed transaction**. Reserving stock and charging a wallet are
side effects owned by two different services, so an order can fail *after*
some steps have already happened — and undoing that requires an explicit
compensating action, because there is no two-phase commit across services
that only talk to each other through a broker.

## The core idea

Event-driven's own README flagged this as the natural next step: "if placing
an order needs to reserve stock in an inventory-service, you now have a
distributed transaction with no 2PC. Model it explicitly as a saga... with
compensating events for the failure path." This lab does exactly that, as an
**orchestrated** saga:

- **orders-service is the orchestrator.** It's the one place that knows the
  whole workflow: reserve stock, then charge the wallet, then confirm — or,
  if a step fails, run the compensating action and fail the order. Read
  `saga.rs` top to bottom and you can see the entire distributed transaction
  in one file.
- **inventory-service and payments-service are participants.** Each one only
  knows how to do its own step (and undo it, for inventory) when told to.
  Neither knows the other exists, and neither knows what happens next in the
  workflow — that knowledge lives entirely in the orchestrator.
- **Still no service calls another service's HTTP endpoint.** Saga commands
  and replies are still just NATS publish/subscribe, same rule as
  event-driven. The difference is that these are *directed* commands — the
  orchestrator tells a specific participant what to do next — not
  fire-and-forget facts that nobody in particular is expected to act on.
- **The tax: the orchestrator is now everyone's coordinator.** Event-driven's
  big win was "producers don't know their consumers." That's still true
  here in one direction (inventory-service and payments-service don't know
  each other), but orders-service now *does* know both of them by name and
  subject — the workflow's visibility comes at the cost of a new central
  point every participant is indirectly coupled to.

## Layout

```
saga/
└── services/
    ├── users-service/          # owns users, publishes users.registered        :3001
    ├── catalog-service/        # owns products & prices, publishes
    │                           #   catalog.product_created / .product_price_changed  :3002
    ├── orders-service/         # the saga ORCHESTRATOR                          :3003
    ├── inventory-service/      # saga participant: stock reservations          :3004
    ├── payments-service/       # saga participant: wallet charges              :3005
    └── notifications-service/  # consumes orders.confirmed / orders.failed
```

`users-service` and `catalog-service` are unchanged from event-driven — still
just owning their data and publishing facts about it.
`inventory-service`/`payments-service` are new. `orders-service` is
event-driven's Orders, reworked into the orchestrator. `notifications-service`
now reacts to the saga's two terminal events instead of the old
`orders.placed`.

## What changed vs. event-driven

| Concern | Event-driven | Saga |
| --- | --- | --- |
| What `POST /orders` does | validates, persists, publishes one fact, done | validates, persists as `Pending`, publishes the *first* saga command |
| What `POST /orders` returns | `200 OK` with a finished order | `202 Accepted` with a `Pending` order — the caller polls `GET /orders/:id` |
| Cross-service messages | facts nobody owns the reaction to | directed commands, each with exactly one intended recipient |
| Who drives the workflow | nobody — every service reacts independently | orders-service, explicitly, in `saga.rs` |
| Multi-step consistency | not modeled — one fact, one step | a real state machine (`OrderStatus`), with a compensating action on failure |
| Coupling | no service names another | orders-service names inventory-service and payments-service (by subject); they don't name each other |

## How it actually flows

```
                    ReserveStockRequested
Order: Pending  ───────────────────────────►  inventory-service
                                                     │
                       reserve.succeeded ◄───────────┤ reserve.failed
                              │                       │
                              ▼                       ▼
                     AwaitingPayment              Failed  (nothing to undo)
                              │
                     ChargeRequested
                              ▼
                        payments-service
                              │
                charge.succeeded ◄──┤ charge.failed
                       │             │
                       ▼             ▼
                  Confirmed    Compensating ──ReleaseStockRequested──► inventory-service
                                     │                                        │
                                     └──────────── release.succeeded ◄────────┘
                                                    │
                                                    ▼
                                                  Failed
```

Every arrow above is a NATS publish; every box transition happens in
`orders-service/src/saga.rs`. Correlation uses the order's own id as the
**saga id** — there's no separate saga-id type anywhere in this lab. Both
participants keep a tiny idempotency ledger keyed by that saga id
(`inventory-service`/`payments-service`'s `repository.rs`), so a redelivered
command — there's no at-most-once guarantee on plain NATS core — can't
double-reserve stock or double-charge a wallet. `saga.rs` guards its own
transitions the same way: every reply handler only applies if the order is
still in the status it expects, so a stale or duplicate reply is just
ignored instead of re-running a step.

Seed numbers are deliberately small and chosen to make all three demo paths
below reachable:

- Catalog seeds **Coffee Mug at 1299¢** (unchanged from the earlier labs).
- Inventory seeds **5 units** of stock per product the moment
  `catalog.product_created` arrives.
- Payments opens **a 2000¢ wallet** the moment `users.registered` arrives.

## A known gap: no saga timeout, no durable saga state

If a participant never replies — it crashed, or the message was dropped
(plain NATS core has no redelivery) — the order just sits in `Pending` or
`AwaitingPayment` forever. A real orchestrator needs a timeout that decides
"this step is never coming back, compensate anyway." And saga state here
lives only in `orders-service`'s in-memory repository: restart it mid-saga
and every in-flight order's progress is gone, even though inventory-service
and payments-service may have already applied their side. See "where to take
it next" below.

## Running it

Requires the Rust toolchain and a NATS server:

```bash
docker run --rm -p 4222:4222 nats:2-alpine
```

Then, from this directory, six terminals (or just use `run-all.ps1` /
`docker compose up --build`, see below):

```bash
cargo run -p users-service          # http://localhost:3001
cargo run -p catalog-service         # http://localhost:3002 — seeds two products
cargo run -p inventory-service       # http://localhost:3004 — seeds stock as products appear
cargo run -p payments-service        # http://localhost:3005 — opens wallets as users register
cargo run -p notifications-service   # no HTTP port; watch its log lines
cargo run -p orders-service          # http://localhost:3003 — start this one last
```

On Windows, start NATS plus all six services at once:

```powershell
./run-all.ps1
```

Run the tests (every service independently, no broker required — saga.rs's
transition logic is exercised through `OrderRepository`/`EventBus` fakes,
same as every other service in this lab) and lint the lot:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build
```

### Try the API

First, register a user and grab a product id:

```bash
curl -X POST localhost:3001/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'
# note the returned "id" — this is USER_ID below

curl localhost:3002/products
# note Coffee Mug's "id" — this is PRODUCT_ID below
```

Give the read models (orders-service, inventory-service, payments-service — all
built from the events above) a moment to catch up, then check the starting
state:

```bash
curl localhost:3004/stock/PRODUCT_ID     # {"available_units":5}
curl localhost:3005/wallets/USER_ID      # {"balance_cents":2000}
```

**1. Happy path** — order 1 mug (1299¢ ≤ 2000¢ wallet, 1 ≤ 5 stock):

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":1}]}'
# 202 Accepted, status: "pending"

curl localhost:3003/orders/ORDER_ID
# poll a couple of times — watch status go pending -> awaiting_payment -> confirmed
```

**2. Insufficient stock** — order 6 mugs (> 5 seeded): fails immediately,
payment is never attempted.

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":6}]}'

curl localhost:3003/orders/ORDER_ID       # status: "failed"
curl localhost:3004/stock/PRODUCT_ID      # still {"available_units":5} — untouched
```

**3. Insufficient funds + compensation** — order 2 mugs (2598¢ > 2000¢
wallet, but 2 ≤ 5 stock): stock *is* reserved, then payment fails, then the
compensating release runs.

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":2}]}'

curl localhost:3003/orders/ORDER_ID
# watch: pending -> awaiting_payment -> compensating -> failed

curl localhost:3004/stock/PRODUCT_ID
# back to {"available_units":5} — proof the compensating action actually ran,
# not just that the order failed
curl localhost:3005/wallets/USER_ID
# still {"balance_cents":2000} — never charged
```

And watch the notifications window: every confirmed *or* failed order
triggers a different log line there, with zero code in orders-service aware
that notifications-service exists.

## Where to take it next (learning exercises)

- **Add a saga timeout.** If a reply never arrives, the order is stuck.
  Add a deadline per step and a "compensate on timeout" path.
- **Persist saga state.** Move `orders-service`'s repository (and the
  participants' idempotency ledgers) to something durable, then kill
  orders-service mid-saga and make it resume correctly on restart.
- **Rewrite this as a choreographed saga.** Instead of orders-service telling
  inventory-service/payments-service what to do, have each participant react
  directly to the previous participant's event (inventory-service listens for
  `orders.placed` and reserves on its own; payments-service listens for a
  `stock.reserved` fact and charges on its own). Compare: is the workflow
  still as easy to find? What replaces the orchestrator's guarded state
  machine?
- **Move to JetStream** for durable, replayable saga messages with acks,
  instead of plain NATS core (same exercise the earlier labs suggest).
- **Add a second compensatable step**, e.g. a shipping reservation after
  payment, to see a two-step rollback chain instead of a one-step one.
- **Harden the containers**, same exercise as the other labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
