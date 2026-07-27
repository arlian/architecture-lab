# architecture-lab — Backend for Frontend (BFF) in Rust

Branches off [microservices](../microservices) rather than continuing the
saga chain: same three backend services (users, catalog, orders), same
"boundaries are physical, HTTP only" rules — but now there are two different
*clients* of that backend, a web app and a mobile app, and they don't want
the same shape of data. This lab adds one gateway per client instead of
making either the clients or the backend services compromise.

## The core idea

In microservices, a browser (or a mobile app) that wants to show an order
would either call orders-service, users-service, and catalog-service
directly and stitch the results together itself, or orders-service would
grow query parameters like `?include=user,products` to serve every client's
needs from one generic endpoint. Both get worse as clients diverge:

- **A Backend for Frontend is a gateway owned by (or for) one specific
  client**, not a generic public API. `web-bff` exists only to serve the web
  app's order-detail screen; `mobile-bff` exists only to serve the mobile
  app's order-status screen.
- **It aggregates, it doesn't own data.** Unlike users/catalog/orders, the
  BFFs have no repository and no domain model — no `domain.rs`,
  no `service.rs`, no persistence. Their entire job is calling other
  services and reshaping the result. `views.rs` in each is the whole
  use case.
- **Different clients, different call graphs — not just different JSON.**
  `web-bff` calls all three backend services for its order-detail screen.
  `mobile-bff` calls only two: its screen never shows a product name, so it
  never links against a catalog client at all (there isn't one in
  `mobile-bff/src/clients.rs`). This is the detail that's easy to miss if you
  think of a BFF as "the same data, formatted differently" — often it's
  legitimately less work, because the client needs less.
- **The tax: as many gateways as you have meaningfully different clients**,
  each with its own deploy, its own on-call surface, and its own copy of
  "how do I call orders-service" glue. Two BFFs here means two things to
  keep in sync with orders-service's contract instead of one.

## Layout

```
bff/
└── services/
    ├── users-service/      # unchanged from microservices/         :3001
    ├── catalog-service/    # unchanged from microservices/         :3002
    ├── orders-service/     # unchanged from microservices/         :3003
    ├── web-bff/             # gateway for the WEB client            :3004
    │                        #   -> calls orders, users, AND catalog
    └── mobile-bff/           # gateway for the MOBILE client         :3005
                             #   -> calls orders and users ONLY
```

`users-service`, `catalog-service`, and `orders-service` are byte-for-byte
the same as [microservices/services](../microservices/services) — this lab
isn't about changing the backend, it's about what sits in front of it.

Each BFF has the same small internal shape:

```
web-bff/src/
├── clients.rs   # narrow HTTP clients for the backend services THIS bff needs
├── views.rs     # the aggregation: fan out, reshape, return one response DTO
├── http.rs      # routes /checkout (proxy) and /orders/:id (aggregation)
└── main.rs      # wires client URLs from the environment, starts the server
```

## What changed vs. microservices

| Concern | Microservices | BFF |
| --- | --- | --- |
| Who calls orders/users/catalog | each other, only where the domain requires it | also a gateway per client, on the client's behalf |
| Where response shaping happens | inside each backend service's own JSON | in the gateway, per client |
| Number of "does this order look right for my screen" endpoints | one, shared by every caller | one per client, each free to differ |
| Backend service coupling to clients | none — services don't know who calls them | still none — the BFFs absorb that knowledge instead |
| New failure mode | a backend call fails | a BFF's *fan-out* can partially fail (e.g. order found, but a product lookup 404s) |
| Deploy units | 3 | 5 |

The backend services still don't know their callers exist, and still don't
know about each other beyond what microservices already established
(orders-service still calls users-service and catalog-service to place an
order). What's new is a second tier of gateways whose *entire* reason to
exist is "shape this for one client."

## Try it: same order, two different responses

Run the lab (see below), register a user, add a product, and place an order
directly against orders-service — that part is identical to microservices:

```bash
curl -X POST localhost:3001/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'
# note "id" -> USER_ID

curl localhost:3002/products
# note Coffee Mug's "id" -> PRODUCT_ID

curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":2}]}'
# note "id" -> ORDER_ID
```

Now fetch the *same* order through both gateways:

```bash
curl localhost:3004/orders/ORDER_ID   # web-bff
```
```json
{
  "order_id": "...",
  "customer": { "id": "...", "name": "Ada", "email": "ada@example.com" },
  "lines": [
    { "product_id": "...", "product_name": "Coffee Mug", "quantity": 2,
      "unit_price_cents": 1299, "line_total_cents": 2598 }
  ],
  "total_cents": 2598
}
```

```bash
curl localhost:3005/orders/ORDER_ID   # mobile-bff
```
```json
{
  "order_id": "...",
  "customer_name": "Ada",
  "item_count": 2,
  "total_cents": 2598
}
```

Same order, same underlying services, deliberately different payloads — and
if you watch each gateway's logs, deliberately different *numbers of
outbound calls*. You can also place an order through either gateway instead
of orders-service directly:

```bash
curl -X POST localhost:3004/checkout \
  -H 'content-type: application/json' \
  -d '{"user_id":"USER_ID","lines":[{"product_id":"PRODUCT_ID","quantity":1}]}'
# identical to POSTing straight to orders-service — checkout is a pure proxy,
# not every BFF endpoint needs aggregation
```

## Running it

From this directory:

```bash
cargo run -p users-service      # http://localhost:3001
cargo run -p catalog-service     # http://localhost:3002 — seeds two products
cargo run -p orders-service      # http://localhost:3003
cargo run -p web-bff              # http://localhost:3004
cargo run -p mobile-bff            # http://localhost:3005
```

On Windows, start all five at once:

```powershell
./run-all.ps1
```

Run the tests (every service independently — the BFFs' aggregation logic is
exercised through fakes of `OrdersClient`/`UsersClient`/`CatalogClient`, same
"depend on the trait, fake the network" pattern as orders-service in
microservices) and lint the lot:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build
```

## Where to take it next (learning exercises)

- **Add a third client.** A `graphql-bff` or `admin-bff` that composes the
  same three backend services differently again — the fastest way to feel
  whether "one gateway per client" scales or turns into "one gateway per
  screen" chaos.
- **Handle partial failure.** Right now if catalog-service is down,
  `web-bff`'s `GET /orders/:id` fails the whole request even though the
  order and customer were found fine. Try degrading gracefully instead —
  return the order with `product_name: null` and a warning, rather than a
  500.
- **Add per-client auth/session handling at the BFF layer** — e.g. web-bff
  terminates a cookie-based session, mobile-bff terminates a bearer token —
  and have neither backend service know or care which scheme the caller used.
  This is one of the most common real-world reasons BFFs exist.
- **Add response caching** to one BFF (e.g. cache `GET /orders/:id` for a few
  seconds) and observe that the choice is local to that gateway — the other
  BFF and the backend services are unaffected.
- **Give web-bff a second aggregation**, e.g. a user dashboard that lists
  all of a customer's orders. orders-service's `list()` has no per-user
  filter today, so watch what a BFF is tempted to do when the backend's API
  doesn't quite fit: filter client-side in the gateway, or push a new query
  parameter down into orders-service?
- **Harden the containers**, same exercise as the other labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
