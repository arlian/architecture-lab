# architecture-lab — Modular Monolith in Rust

A small, runnable boilerplate for learning the **modular monolith** architecture
in Rust. One deployable binary, but internally split into independent modules
(bounded contexts) whose boundaries are enforced *by the Rust compiler*.

## The core idea

A modular monolith sits between a big-ball-of-mud monolith and microservices:

- **One process, one deployable** — simple ops, no network between modules.
- **Strong internal boundaries** — each module is a separate crate. A module can
  only touch another module through its **published public API**, never its
  internals. If you try, it won't compile.
- **Easy to extract later** — because modules already talk through narrow
  contracts, promoting one to its own service is a mechanical change.

## Layout

```
architecture-lab/
├── Cargo.toml                 # workspace + shared dependency versions
└── crates/
    ├── shared/                # shared kernel: AppError only. Keep it tiny.
    ├── users/                 # bounded context: users
    ├── catalog/               # bounded context: products & prices
    ├── orders/                # bounded context: orders (uses users + catalog)
    └── app/                   # composition root: wires modules, runs HTTP server
```

## Anatomy of a module

Every module crate has the same internal layering, and exposes almost nothing:

```
users/src/
├── domain.rs        # entities & value objects (User, UserId) — no framework
├── repository.rs    # storage trait + in-memory impl   [pub(crate) — hidden]
├── service.rs       # use cases / business rules        [application layer]
├── api.rs           # PUBLIC cross-module contract (trait UserDirectory)
├── http.rs          # HTTP adapter (maps requests -> service)   [pub(crate)]
└── lib.rs           # exposes: api, the Module wiring type, boundary types
```

What's `pub` is a deliberate, minimal surface. Storage, HTTP wiring, and internal
helpers are `pub(crate)` — invisible to other modules.

## How modules communicate

Orders needs two things from its neighbours when placing an order:

1. *Does this user exist?* → `users::api::UserDirectory`
2. *What does this product cost?* → `catalog::api::ProductCatalog`

Orders depends on those **traits**, not on `UserService` / `ProductService`. The
composition root (`crates/app/src/main.rs`) injects the real implementations:

```rust
let orders = OrdersModule::new(
    users.service.clone()   as Arc<dyn users::api::UserDirectory>,
    catalog.service.clone() as Arc<dyn catalog::api::ProductCatalog>,
);
```

This dependency inversion is the whole game:
- Modules stay decoupled — Orders never sees how Users is built.
- Modules are testable in isolation — see the fakes in `orders/src/service.rs`.
- Data stays owned — Orders stores a `UserId`, never a copy of `User`.

## Running it

Requires the Rust toolchain (`rustup`, which installs `cargo`). Then:

```bash
cargo run -p app        # starts the server on http://localhost:3000
cargo test              # runs every module's unit tests
cargo clippy            # lints
```

### Try the API

```bash
# health
curl localhost:3000/health

# create a user
curl -X POST localhost:3000/users \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","name":"Ada"}'

# list seeded products (grab a product id)
curl localhost:3000/products

# place an order (use the user id and product id from above)
curl -X POST localhost:3000/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"<USER_ID>","lines":[{"product_id":"<PRODUCT_ID>","quantity":2}]}'
```

## Where to take it next (learning exercises)

- **Extract API crates.** Right now Orders depends on the whole `users` crate but
  only uses its `api`. Move each `api` into a `users-api` / `catalog-api` crate so
  a module physically cannot reach another module's internals.
- **Add domain events.** Instead of Orders calling Users synchronously, publish a
  `OrderPlaced` event on an in-process bus and let modules subscribe. This is the
  step that most resembles future microservices.
- **Swap the in-memory repos** for a real database (e.g. `sqlx` + Postgres),
  changing only each module's `repository.rs`.
- **Give each module its own schema/tables** and forbid cross-module joins — the
  data-ownership rule that keeps a modular monolith from rotting.
```
