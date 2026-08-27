# architecture-lab — Hexagonal (ports & adapters) in Rust

The same little e-commerce domain as the other labs, shrunk to the part that
matters here: **placing an order**. One piece of order logic, driven two
completely different ways — over HTTP, and from a shell — with two different
storage backends underneath it, and not one line of that logic aware of any
of it.

This is the smallest lab in the repo on purpose. Hexagonal architecture isn't
a topology, so there's nothing to draw and nothing to deploy; it's a rule
about which direction dependencies point. The whole lab exists to make that
rule *checkable*.

## The core idea

Every other lab in this repo answers "where do the boundaries go?" — between
modules, between processes, between reads and writes. This one asks a
different question: **which side of the boundary is the technology on?**

- **The core is the hexagon.** Domain, ports, use cases. It knows nothing
  about the outside world.
- **Ports are traits the core declares**, written in the core's own
  vocabulary. Driven ports (`OrderRepository`, `UserDirectory`,
  `ProductCatalog`) are things the core needs done; the driving port is the
  public API of `OrderService`, which is how the world asks the core for
  things.
- **Adapters live outside and point inwards.** A web framework, a file, a
  test fake — all the same kind of thing, all equally replaceable.
- **A composition root picks the adapters** at startup. It is the only code
  that knows both sides, and it contains no rules.

## Layout

```
hexagonal/
├── orders-core/            # THE HEXAGON — a library with no main()
│   └── src/
│       ├── domain.rs       #   orders, ids, DomainError (no status codes)
│       ├── ports.rs        #   the traits: what the core needs from the world
│       └── service.rs      #   the use cases + unit tests
└── orders-app/             # EVERYTHING OUTSIDE
    └── src/
        ├── repository.rs   #   driven:  in-memory / JSON file
        ├── directory.rs    #   driven:  stub users + catalog tables
        ├── http.rs         #   driving: axum
        ├── console.rs      #   driving: argv
        └── bin/
            ├── serve.rs    #   composition root #1 -> HTTP
            └── cli.rs      #   composition root #2 -> shell
```

No `docker-compose.yml` and no broker in this lab, unlike its neighbours.
There is only one deployable and the interesting boundary is *inside* the
process, so containers would add ceremony without adding a lesson.

## The proof is in the manifests

The two `Cargo.toml` files are the shortest honest summary of the whole idea:

| | `orders-core` | `orders-app` |
| --- | --- | --- |
| axum | — | ✅ |
| tokio | dev-dependency only (test executor) | ✅ |
| serde_json | — | ✅ |
| serde / uuid / thiserror | ✅ | ✅ |

The core *cannot* serve a route or read a file, because it has nothing to do
it with. That's not discipline, it's arithmetic — and `cargo tree` will tell
you the day someone breaks it.

## The files to read first

**`orders-core/src/ports.rs` and `orders-app/src/repository.rs`.** The port
says "give me somewhere to put orders"; the adapters answer with a `HashMap`
and with a JSON file. Note that `OrderRepository::insert` returns `Result`
even though the in-memory implementation can never fail — the port is written
for the general case, so a fallible backend can be plugged in later without
the core changing shape.

**`orders-app/src/http.rs` next to `orders-app/src/console.rs`.** Two driving
adapters, ~100 lines each, with nothing in common but the `OrderService`
calls in the middle. Everything else is translation: one turns a missing
order into `404`, the other turns it into `exit 1`.

### The compiler argues on the architecture's behalf

In [`microservices/`](../microservices), `AppError` carried an
`impl IntoResponse` right next to the domain — one small line that made
"not found" and "404" the same idea, and quietly meant the domain could only
ever live inside a web server.

Try writing that impl in this lab and Rust refuses: `DomainError` is a
foreign type in `orders-app` and `IntoResponse` is a foreign trait in
`orders-core`, so the orphan rule makes the impl illegal in both crates. The
only legal home is a local newtype inside the adapter — `ApiError` in
`http.rs`. **The crate split turns "don't leak HTTP into the domain" from a
code-review opinion into a compile error.**

## What changed vs. microservices

Compare `orders-core/src/service.rs` with
`microservices/services/orders-service/src/service.rs`. The body of `place()`
is nearly identical — same rules, same order of checks. What moved is
everything around it.

| Concern | Microservices orders-service | Hexagonal |
| --- | --- | --- |
| Where the domain lives | in the same crate as axum and reqwest | in a crate that depends on neither |
| Users / catalog ports | traits, backed by HTTP clients | the **same traits**, backed by static tables |
| Persistence | a concrete `InMemoryOrderRepository`, no port | a port with two adapters, chosen at startup |
| Error type | `AppError`, with `impl IntoResponse` beside the domain | `DomainError`, with the status-code mapping in the adapter |
| Ways in | one (HTTP) | two (HTTP, CLI) + the test suite |
| Deploy unit | one binary | two binaries over one library |

The middle row is the one to sit with. `UserDirectory` and `ProductCatalog`
were *already* ports in the monolith and in microservices — that lab's tests
already ran without a socket. Hexagonal architecture is mostly the act of
noticing that seam worked and applying it to everything else, persistence
included.

## Running it

Requires the Rust toolchain (`rustup`, which installs `cargo`). From this
directory:

```bash
cargo test      # the core's use cases, driven by fakes — no server, no files
cargo clippy
```

The seeded ids are fixed so the commands below can be pasted as-is:

```
user     11111111-1111-1111-1111-111111111111  (Ada)
product  22222222-2222-2222-2222-222222222222  (Coffee Mug, 1299)
product  33333333-3333-3333-3333-333333333333  (Notebook,    850)
```

### One core, two ways in

Place an order from the shell. The CLI composition root always picks the file
repository, so this writes `orders.json` next to you:

```bash
cargo run --bin cli -- place \
  11111111-1111-1111-1111-111111111111 \
  22222222-2222-2222-2222-222222222222 3

cargo run --bin cli -- list
```

Now start the *other* adapter on the *same* store and ask over HTTP:

```bash
# bash
ORDERS_FILE=orders.json cargo run --bin serve      # http://localhost:3003

# PowerShell
$env:ORDERS_FILE="orders.json"; cargo run --bin serve
```

```bash
curl localhost:3003/orders
# the order you placed from the shell, now as JSON

curl -X POST localhost:3003/orders \
  -H 'content-type: application/json' \
  -d '{"user_id":"11111111-1111-1111-1111-111111111111",
       "lines":[{"product_id":"33333333-3333-3333-3333-333333333333","quantity":2}]}'

cargo run --bin cli -- list
# ...and the order you placed over HTTP shows up in the shell
```

Two programs with no code in common, agreeing perfectly, because the rules
they obey live in one place that neither of them can see into.

### Watch the adapters swap

```bash
# no ORDERS_FILE: the composition root picks the in-memory repository instead
cargo run --bin serve
curl localhost:3003/orders     # [] — a restart forgets everything
```

That is a one-line change of storage technology, at startup, with
`orders-core` compiled identically either way.

```bash
# and the failure path, without breaking anything for real:
echo 'not json' > broken.json
ORDERS_FILE=broken.json cargo run --bin cli -- list
# "dependency unavailable: broken.json is not valid order JSON..." and exit 1
```

The core reported `Unavailable` without knowing what a file is. The adapter
translated a `serde_json` parse error at the boundary, which is exactly the
job a port exists to do.

## Known gaps (deliberate)

- **The domain types derive `serde`.** Strictly, the hexagon now knows that
  serialization exists. The pure version keeps DTOs in the adapters and maps
  them at the boundary; this lab takes the shortcut to stay small, and it's
  the first exercise below.
- **The driving port is a struct, not a trait.** `OrderService`'s public
  methods serve as the inbound port. Good enough to keep adapters honest,
  short of what a purist would write.
- **The JSON repository rewrites the whole file** under an in-process mutex.
  Two processes writing at once (a `cli` call while `serve` is running) can
  clobber each other. It's a teaching adapter, not a database.
- **Users and catalog are hardcoded tables**, so this lab has no partial
  failure across a network — that story lives in `microservices/`. Here,
  `DomainError::Unavailable` is exercised by the file adapter and the tests.

## Where to take it next (learning exercises)

- **Get `serde` out of the core.** Give `orders-app` its own `OrderDto` and
  map at the boundary. Then delete `serde` from `orders-core/Cargo.toml` and
  see what breaks — the compiler will list every leak for you.
- **Make the driving port a trait.** Declare `trait PlaceOrderUseCase` in the
  core, have both adapters depend on `Arc<dyn PlaceOrderUseCase>`, and then
  write a decorator that logs or times every call without touching the
  implementation.
- **Add a third driving adapter.** A NATS consumer that places orders off an
  `orders.requested` message would let this core join the
  [choreography](../choreography) lab without the core noticing.
- **Point the stubs at the real thing.** Reimplement `StaticUserDirectory`
  and `StaticProductCatalog` with `reqwest`, aimed at
  [microservices](../microservices)' users/catalog services on :3001/:3002.
  Copy the bodies straight out of that lab's `clients.rs` — the traits match
  because they never changed. Note what has to move: one new file in
  `orders-app`, one line in each composition root, and nothing else.
- **Add a real database adapter** (`sqlx` + Postgres) behind
  `OrderRepository`, and keep `cargo test` running with zero infrastructure.
- **Add an outbound notification port** (`OrderPlacedNotifier`) with a
  logging adapter and a broker adapter, and watch the core stay ignorant of
  which one is installed.
