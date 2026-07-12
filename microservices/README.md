# architecture-lab — Microservices in Rust

The same little e-commerce domain as the [modular monolith](../modular-monolith),
rebuilt as **independently deployable services that talk over HTTP**. Reading the
two side by side is the whole point: the domain logic barely changes, but the way
the pieces connect — and the failure modes you sign up for — change a lot.

## The core idea

Where the modular monolith enforced boundaries with the *compiler* inside one
process, microservices enforce them with the *network* across many processes:

- **Many processes, many deployables** — each service is its own binary, its own
  port, its own release cadence, independently scalable.
- **No shared code, no shared database** — services agree only on a *wire
  contract* (HTTP + JSON). Each owns its data privately.
- **Boundaries are physical** — a service literally cannot reach into another's
  internals; it can only call its API over the network.
- **The tax: partial failure** — an in-process call can't fail; a network call
  can be slow, down, or wrong. That reality shows up in the code.

## Layout

```
microservices/
├── Cargo.toml                 # a workspace ONLY for local convenience (see note)
└── services/
    ├── users-service/         # owns users            :3001
    ├── catalog-service/       # owns products & prices :3002
    └── orders-service/        # owns orders; calls the other two :3003
```

> **Why one workspace?** Purely so `cargo build` is one command while you learn.
> Conceptually each service is its own repo, pipeline, and container. Nothing in
> the code couples them at build time — delete the workspace file and build each
> `services/*` folder separately and it still works.

Each service has the same internal layering as a monolith module —
`domain` / `repository` / `service` / `http` — because good internal structure is
orthogonal to how you deploy. What's new lives in **`orders-service`**.

## What changed vs. the modular monolith

| Concern | Modular monolith | Microservices |
| --- | --- | --- |
| Boundary enforced by | the Rust compiler (`pub(crate)`) | the network |
| Cross-context call | inject `Arc<dyn Trait>`, call a method | HTTP request to a URL |
| Can that call fail? | no | **yes** — every call returns `Result` |
| Shared error type | one `shared` crate | **copied** into each service |
| Shared id types | `use users::UserId` | each service defines its **own** ids |
| Wiring | one composition root (`app`) | each service wires itself + neighbour **URLs** |
| Deploy unit | one binary | three binaries |

The two design pillars survive the jump intact, which is the encouraging part:

1. **Data ownership.** Orders stores a `user_id`, never a copy of the user. To
   learn anything more it must ask the Users service.
2. **Depend on a narrow contract, not an implementation.** Orders still depends on
   the *ports* `UserDirectory` and `ProductCatalog` (see
   [`orders-service/src/clients.rs`](./services/orders-service/src/clients.rs)).
   In the monolith the real service was injected; here an **HTTP client** is. Same
   seam — which is exactly why the Orders unit tests still run with in-memory fakes
   and never open a socket.

## How Orders talks to its neighbours

Placing an order still needs two answers. Both are now network calls:

1. *Does this user exist?* → `GET {USERS_URL}/users/:id` → **200** vs **404**.
2. *What does this product cost?* → `GET {CATALOG_URL}/products/:id`, read
   `price_cents`.

```rust
// orders-service/src/service.rs — note the `?`: the network can fail now.
if !self.users.exists(cmd.user_id).await? {           // HTTP round-trip
    return Err(AppError::Validation("user ... does not exist".into()));
}
let unit_price_cents = self.catalog.price_of(id).await?  // HTTP round-trip
    .ok_or_else(|| AppError::Validation("product ... does not exist".into()))?;
```

Orders finds its collaborators by **URL from the environment** (`USERS_URL`,
`CATALOG_URL`) — the simplest form of service discovery. A real system would use
DNS, a registry, or a service mesh instead of hard-coded addresses.

## Running it

Requires the Rust toolchain (`rustup`, which installs `cargo`). The services must
run at the same time. Open **three terminals** from this directory:

```bash
# terminal 1 — users
cargo run -p users-service       # http://localhost:3001

# terminal 2 — catalog (seeds two products on startup)
cargo run -p catalog-service     # http://localhost:3002

# terminal 3 — orders (needs the other two up)
cargo run -p orders-service      # http://localhost:3003
```

On Windows you can start all three at once with the helper script:

```powershell
./run-all.ps1                    # opens each service in its own window
```

Run the tests (each service independently) and lint the lot:

```bash
cargo test                       # runs every service's unit tests
cargo clippy
```

### …or with Docker

No Rust toolchain needed — each service ships as its own image, and
`docker-compose.yml` wires them on a private network:

```bash
docker compose up --build        # builds 3 images, starts all 3 containers
```

This is where the microservice model gets concrete. Each service builds from the
same context (the workspace root) but a **different `Dockerfile`** — one image per
deployable. Service discovery is just Docker's DNS: Orders reaches its neighbours
by *service name* (`USERS_URL=http://users:3001`), never a hard-coded address.
The same three ports (3001/3002/3003) are published to your host, so the `curl`
commands below work unchanged.

### Try the API

```bash
# health of each service
curl localhost:3001/health
curl localhost:3002/health
curl localhost:3003/health

# create a user (on the users service)
curl -X POST localhost:3001/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'

# list the seeded products (on the catalog service) — grab a product id
curl localhost:3002/products

# place an order (on the orders service) — it will call users + catalog for you
curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"<USER_ID>","lines":[{"product_id":"<PRODUCT_ID>","quantity":2}]}'
```

**See the trade-off in action:** stop the catalog service (Ctrl-C in terminal 2)
and place another order. Instead of a clean answer you get a `500` —
`"catalog service unreachable"`. In the monolith this failure was impossible.
Handling it (retries, timeouts, circuit breakers, fallbacks) is the ongoing work
microservices ask of you.

## Where to take it next (learning exercises)

- **Add resilience.** Give the HTTP clients timeouts and a retry/circuit-breaker
  policy. Decide what Orders should do when Catalog is down.
- **Stop the synchronous chatter.** Have Orders publish an `OrderPlaced` event to
  a broker (NATS/Kafka/RabbitMQ) instead of calling neighbours inline — the move
  toward event-driven, eventually-consistent services.
- **Give each service a real database** (`sqlx` + Postgres), one schema per
  service, and keep the no-shared-DB rule.
- **Add an API gateway** in front so clients hit one origin instead of three
  ports, and centralise auth there.
- **Contract-test the boundaries.** Add consumer-driven contract tests so Orders
  and Catalog can evolve without breaking each other.
- **Harden the containers.** The `Dockerfile`s + `docker-compose.yml` are already
  here; next add real healthchecks and `depends_on: condition: service_healthy`,
  run as a non-root user, and pin base-image digests.
```
