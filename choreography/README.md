# architecture-lab — Choreography in Rust

The same little e-commerce domain as the other labs, and the same
distributed transaction as [saga](../saga) — reserve stock, charge a wallet,
undo the reservation if the charge fails. One thing is different: **the
orchestrator is gone.**

This lab is the answer to the exercise the saga README set for itself:

> **Rewrite this as a choreographed saga.** Instead of orders-service telling
> inventory-service/payments-service what to do, have each participant react
> directly to the previous participant's event. Compare: is the workflow
> still as easy to find? What replaces the orchestrator's guarded state
> machine?

Short answers: no, and nothing. The long answers are below.

## The core idea

In `saga/`, orders-service knew the whole workflow and drove it, one directed
command at a time. Here nobody does. Every service publishes facts about its
own domain, subscribes to whatever facts it has decided are its business, and
acts on its own authority:

- **orders-service announces and then stops.** `POST /orders` validates,
  persists, publishes `orders.placed`, and returns. It does not ask for a
  stock reservation, because there is nobody it would be asking. Whether
  placing an order reserves stock is not its business any more.
- **inventory-service reserves stock because it decided to.** It subscribes
  to `orders.placed` and concludes on its own that a placed order warrants a
  reservation.
- **payments-service charges because it decided to.** It subscribes to
  `inventory.stock_reserved` and concludes on its own that reserved stock
  means it's time to debit a wallet.
- **inventory-service compensates itself.** It subscribes to
  `payments.declined` and puts the stock back. Note who is *not* involved:
  nobody asked it to roll back, and payments-service does not know it
  reserved anything in the first place.
- **Nobody declares the order finished.** There is no `orders.confirmed` and
  no `orders.failed` in this lab, because publishing either would require a
  service willing to speak for the whole workflow — which is exactly the
  thing we deleted. Success and failure are *inferences*, and each interested
  service draws its own.

## Layout

```
choreography/
└── services/
    ├── users-service/          # owns users, publishes users.registered        :3001
    ├── catalog-service/        # owns products & prices, publishes
    │                           #   catalog.product_created / .product_price_changed  :3002
    ├── orders-service/         # owns orders; publishes orders.placed and then
    │                           #   watches, tracker.rs                          :3003
    ├── inventory-service/      # owns stock; reserves and compensates on its
    │                           #   own initiative                               :3004
    ├── payments-service/       # owns wallets; joins two fact streams to know
    │                           #   when and what to charge                      :3005
    └── notifications-service/  # decides for itself what "done" means
```

`users-service` and `catalog-service` are byte-for-byte the saga lab's — they
were never part of the workflow, so removing the coordinator didn't touch
them. Every other service changed.

### The one file to read first

`orders-service/src/tracker.rs` is what `saga/services/orders-service/src/saga.rs`
became. Open them side by side. They subscribe to almost the same subjects
and run the same five-state machine; the difference is that every handler in
`saga.rs` ends by publishing the next command, and every handler in
`tracker.rs` just... ends. That diff is the whole lab.

## What changed vs. saga

| Concern | Saga (orchestrated) | Choreography |
| --- | --- | --- |
| What `POST /orders` publishes | `inventory.reserve.requested` — a command, addressed to one service | `orders.placed` — a fact, addressed to nobody |
| Who knows the workflow | orders-service, entirely, in one readable file | nobody; it's an emergent property of six subscriptions in three services |
| Who triggers step 2 (charge) | the orchestrator, after seeing step 1 succeed | payments-service, on its own, off inventory's fact |
| Who triggers compensation | the orchestrator, by command | inventory-service, on its own, off payments' fact |
| Who declares the order done | orders-service, via `orders.confirmed` / `orders.failed` | nobody — each service infers it separately, and they don't fully agree |
| `OrderStatus` | control state; reaching a state *causes* the next step | a report; reaching a state causes nothing |
| If orders-service dies mid-flow | workflow halts at that step, forever | workflow completes normally; only the *view* is lost |
| If a participant dies mid-flow | orchestrator waits forever (no timeout — a known gap there too) | same, but now there is nobody even in a position to notice |
| Distinct message types | 10 (request/reply pairs + 2 terminal events) | 6 |
| Copies of order data in the system | 1 (orders-service) | 3 (orders-service, payments-service, notifications-service) |

## How it actually flows

Every arrow is a broadcast, not a delivery to a recipient. Read the fan-outs
as "everyone who cared was listening", not "these three were told":

```
POST /orders
     │
     ▼
orders-service ──── orders.placed ────┬──────────────┬────────────────┐
  (done; it now         (a fact)      │              │                │
   only watches)                      ▼              ▼                ▼
                              inventory-svc    payments-svc    notifications
                               RESERVES        files the       files the
                                   │           amount away     customer away
                                   │           (not a trigger) (not a trigger)
                 ┌─────────────────┴─────────────────┐
                 ▼                                   ▼
      inventory.stock_rejected            inventory.stock_reserved
                 │                                   │
        ┌────────┴────────┐              ┌───────────┴───────────┐
        ▼                 ▼              ▼                       ▼
  orders: failed    notif: "sorry"  orders:              payments-svc
                                    awaiting_payment     JOINS + CHARGES
                                                                 │
                                          ┌──────────────────────┴───────┐
                                          ▼                              ▼
                                   payments.charged              payments.declined
                                          │                              │
                                ┌─────────┴────────┐      ┌──────────────┼──────────────┐
                                ▼                  ▼      ▼              ▼              ▼
                        orders: confirmed   notif:      orders:     inventory-svc   notif:
                                            "thanks"  compensating   RELEASES       "sorry"
                                                                          │
                                                            inventory.stock_released
                                                                          │
                                                                          ▼
                                                                  orders: failed
```

Three things worth staring at in that diagram.

**The workflow's causality is the fan-out, not the boxes.** In the saga
diagram every arrow had a sender and a named recipient. Here an arrow means
"this was announced"; who acts on it is decided at the other end, by code in
a different repository, that the publisher has never heard of.

**`payments.declined` fans out to three services that each do something
different with it.** inventory-service treats it as an instruction to roll
back. orders-service treats it as "compensation is starting" and keeps
waiting. notifications-service treats it as the end of the story and emails
the customer immediately. Nobody coordinated that, and nobody could.

**The bottom-left path finishes in two hops; the bottom-right takes four.**
In `saga/` the orchestrator made that asymmetry explicit in one `match`. Here
you can only discover it by reading three services.

## The trade this lab exists to show

It is tempting to summarize choreography as "less coupling." That is
backwards. Count the foreign namespaces each service must know about to do
its job (ignoring the `users.*` / `catalog.*` seed subscriptions, which are
identical in both labs):

| Service | Saga | Choreography |
| --- | --- | --- |
| orders-service | inventory, payments (**2**) | inventory, payments (**2**) |
| inventory-service | — (**0**) | orders, payments (**2**) |
| payments-service | — (**0**) | orders, inventory (**2**) |
| notifications-service | orders (**1**) | orders, inventory, payments (**3**) |
| **total** | **3** | **9** |

Choreography did not remove coupling. It removed the **hub** and
redistributed the hub's knowledge into every spoke. The star became a mesh.

The subtlety in row one: orders-service's number didn't move. Its coupling
got *weaker in kind* — it only reads those namespaces now instead of
publishing into them — but not narrower in extent, and only because this lab
keeps a `status` field on `GET /orders/:id`. Delete `tracker.rs` and that row
drops to **0**: orders-service would publish one fact and know nothing about
anything. That is the honest best case for choreography, and the price is
that your orders API can no longer tell a customer what happened to their
order. Which is usually not a price anyone wants to pay — see the exercises.

### Where the orchestrator's work actually went

Deleting a coordinator does not delete the work it was doing. Three jobs
moved, and none of them are in the domain:

1. **Sequencing** → into each participant's choice of subscription.
   (`inventory-service/src/reactor.rs`, `payments-service/src/reactor.rs`.)
2. **Data assembly** → into a streaming join in every participant that needs
   data it doesn't own. `payments.charged` and `inventory.stock_reserved`
   carry an `order_id` and nothing else, because their publishers don't know
   who the customer is or what the total was. So payments-service and
   notifications-service each keep their own copy of every order, forever,
   and each had to write a two-input join to correlate it — see
   `payments-service/src/repository.rs` and
   `notifications-service/src/registry.rs`. The orchestrator used to do this
   join for everybody, once, by holding the order and putting the right
   fields in each command.
3. **Deciding what "done" means** → into every consumer, independently. See
   below.

### Two services, two definitions of done

On the payment-failure path, `orders-service` moves the order to
`compensating` when it sees `payments.declined` and only calls it `failed`
once `inventory.stock_released` confirms the stock went back.
`notifications-service` treats `payments.declined` itself as terminal and
emails the customer right then.

So for a few milliseconds the customer has been told their order failed while
`GET /orders/:id` still says `compensating`.

Both readings are defensible. The point is that **nothing in the system is in
a position to say one of them is wrong.** In `saga/` this disagreement was
not expressible: there was one `orders.failed` event, published at one
moment, by the one service that got to decide. Choreography buys services
that evolve independently by giving up the guarantee that they agree.

### One thing choreography is genuinely better at

Kill orders-service immediately after `POST /orders` and the order still gets
reserved, charged, and (on failure) compensated. Inventory and payments never
needed it. In `saga/`, the same kill freezes the workflow at whatever step it
had reached, permanently, because the thing that decides what happens next is
on the floor.

Losing the orchestrator loses the workflow. Losing the tracker loses only the
view of it. There's a walkthrough for this below — it's the most interesting
thing to actually try.

## Known gaps

Everything the saga lab flagged is still true (no timeouts, no durable state,
plain NATS core with no redelivery or replay), and removing the coordinator
made two of them meaningfully worse:

- **A dropped `orders.placed` is undetectable.** In `saga/`, a lost first
  command left the order in `Pending` with the orchestrator aware that a step
  was outstanding — enough to build a timeout on. Here, if that one publish
  fails, no participant ever learns the order exists, and no participant is
  expecting it. There is nothing anywhere in the system holding the
  expectation that could be violated.
- **A stalled order has no owner.** If payments-service never charges,
  the stock stays reserved forever. In the orchestrated version there was at
  least an obvious place to put the fix (the orchestrator knew it was waiting
  on a reply). Here, which service should notice? inventory-service doesn't
  know a charge was supposed to happen. orders-service knows something is
  wrong but has no authority to do anything about it. There is no natural
  home for the timeout — you end up reintroducing a coordinator, or building
  a saga log that watches the bus, which is a coordinator wearing a hat.
- **The join buffers grow without bound.** payments-service and
  notifications-service must remember every order they've ever seen, because
  no fact ever says "this one is finished, you may forget it." Adding such a
  fact means someone deciding an order is finished — which brings us back to
  the missing orchestrator. It's the same hole in a different shape.
- **Redelivery guards are now the only guards.** The idempotency ledgers in
  `inventory-service`/`payments-service` used to be a second line of defence
  behind the orchestrator's own guarded transitions. They are now the *only*
  check that a step doesn't run twice, and nothing audits them. See
  `inventory-service/src/repository.rs`, which needs a strictly stronger
  ledger than the saga lab's for exactly this reason — the orchestrator used
  to quietly serialize reserve and release, and now nothing does.

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
cargo run -p orders-service          # http://localhost:3003
```

Unlike the saga lab, **the order doesn't matter** — there's no orchestrator
that has to come up last. It does still matter that everyone is running
before you place an order: plain NATS core has no replay, so a fact published
while a subscriber is down is gone for that subscriber forever.

On Windows, start NATS plus all six services at once:

```powershell
./run-all.ps1
```

Run the tests (every service independently, no broker required — the join
logic and the tracker transitions are exercised through repository/`EventBus`
fakes, same as every other service in this lab) and lint the lot:

```bash
cargo test
cargo clippy
```

The tests worth reading are the ones that only exist because the coordinator
is gone:

- `payments-service`: `charges_when_the_reservation_is_observed_before_the_order`
- `inventory-service`: `a_redelivered_placement_after_compensation_does_not_re_reserve`
- `notifications-service`: `sends_when_the_outcome_is_observed_before_the_order`

### …or with Docker

```bash
docker compose up --build
```

### Try the API

Seed numbers are the same as the saga lab: Catalog seeds **Coffee Mug at
1299¢**, Inventory seeds **5 units** per product, Payments opens a **2000¢**
wallet per user.

First, register a user and grab a product id:

```bash
curl -X POST localhost:3001/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'
# note the returned "id" — this is USER_ID below

curl localhost:3002/products
# note Coffee Mug's "id" — this is PRODUCT_ID below
```

Give the read models a moment to catch up, then check the starting state:

```bash
curl localhost:3004/stock/PRODUCT_ID     # {"available_units":5}
curl localhost:3005/wallets/USER_ID      # {"balance_cents":2000}
```

The three walkthroughs below run **in sequence** from that starting state, so
the expected numbers carry over from one to the next.

**1. Happy path** — order 1 mug (1299¢ ≤ 2000¢ wallet, 1 ≤ 5 stock):

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":1}]}'
# 202 Accepted, status: "pending"

curl localhost:3003/orders/ORDER_ID
# poll a couple of times — pending -> awaiting_payment -> confirmed

curl localhost:3004/stock/PRODUCT_ID     # {"available_units":4}
curl localhost:3005/wallets/USER_ID      # {"balance_cents":701}
```

Watch the three service windows as this happens. Nothing in any of them says
"next step" — each just reports what it decided to do about something it
overheard.

**2. Insufficient stock** — order 6 mugs (> the 4 now left): fails
immediately, payment is never attempted.

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":6}]}'

curl localhost:3003/orders/ORDER_ID       # status: "failed"
curl localhost:3004/stock/PRODUCT_ID      # still {"available_units":4} — untouched
curl localhost:3005/wallets/USER_ID       # still {"balance_cents":701} — never charged
```

payments-service filed this order's details away when `orders.placed` went
out and will now hold them forever, because no reservation is ever coming and
nothing will ever tell it to forget. That's the unbounded join buffer from
"known gaps", visible in one request.

**3. Insufficient funds + compensation** — order 2 mugs (2598¢ > the 701¢
left, but 2 ≤ 4 stock): stock *is* reserved, payment fails, and
inventory-service rolls itself back.

```bash
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":2}]}'

curl localhost:3003/orders/ORDER_ID
# pending -> awaiting_payment -> compensating -> failed

curl localhost:3004/stock/PRODUCT_ID
# back to {"available_units":4} — proof the compensation ran, and that
# nobody ordered it to
curl localhost:3005/wallets/USER_ID
# still {"balance_cents":701} — never charged
```

Watch the notifications window closely on this one: its "sorry" line appears
on `payments.declined`, one hop *before* orders-service reaches `failed`.
That's the disagreement from "two definitions of done", live.

**4. The interesting one: kill the tracker.**

```bash
# place an order that will succeed...
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":1}]}'

# ...and immediately Ctrl-C the orders-service window.
```

Now check the participants:

```bash
curl localhost:3004/stock/PRODUCT_ID     # decremented anyway
curl localhost:3005/wallets/USER_ID      # charged anyway
```

The order went through with its own service dead. Notifications even emailed
the customer. Try the identical experiment in `saga/` — kill orders-service
right after `POST /orders` and stock is never touched at all, because the
thing that decides what happens next is gone.

Then restart orders-service and ask it about that order: it's not there. The
work happened; the record of it didn't. That gap — real state changed,
nobody's view agrees — is the thing choreography makes cheap to create and
expensive to detect.

## Where to take it next (learning exercises)

- **Delete `tracker.rs` entirely** and see what `GET /orders/:id` can still
  honestly say. This is the version of choreography with the coupling table's
  best-case numbers. Decide whether you'd ship it.
- **Add a step in the middle** — say a shipping reservation between stock and
  payment. In `saga/` this is a one-file edit to the orchestrator. Here,
  count the files you have to touch and the services you have to redeploy in
  the right order to avoid a window where orders fall through the gap.
- **Find the workflow.** Hand this repo to someone and ask them to write down
  what happens when an order is placed, without running it. Time them. Do the
  same with `saga/`.
- **Add a saga log.** Build a seventh service that subscribes to everything
  and reconstructs each order's progress, purely to answer "where is order
  42?" Then notice what you've built: a read-only orchestrator. Ask whether
  the write-side version would have been cheaper.
- **Make the stalled-order problem concrete.** Kill payments-service, place an
  order, restart it. The stock is reserved and nothing will ever charge or
  release it. Now decide which service should have caught that, and see how
  quickly you end up reinventing a coordinator.
- **Move to JetStream** for durable, replayable facts with acks. This fixes
  the "published while you were down" hole and is a much bigger deal here
  than in the earlier labs, since there's no coordinator to re-drive anything.
- **Bound the join buffers.** Add whatever fact would let payments-service and
  notifications-service forget an order, then work out who is qualified to
  publish it.
- **Harden the containers**, same exercise as the other labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
