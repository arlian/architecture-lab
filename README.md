# architecture-lab

A personal lab for learning software architecture patterns hands-on. Each
architecture lives in its own self-contained subdirectory with its own build.

## Architectures

| Directory                                 | Pattern             | Stack             |
| ----------------------------------------- | ------------------- | ----------------- |
| [`modular-monolith/`](./modular-monolith) | Modular monolith     | Rust + Axum        |
| [`microservices/`](./microservices)       | Microservices        | Rust + Axum        |
| [`event-driven/`](./event-driven)         | Event-driven         | Rust + Axum + NATS |
| [`cqrs-es/`](./cqrs-es)                   | CQRS + event sourcing| Rust + Axum + NATS |
| [`saga/`](./saga)                         | Saga (orchestrated) | Rust + Axum + NATS |
| [`choreography/`](./choreography)         | Saga (choreographed) | Rust + Axum + NATS |
| [`bff/`](./bff)                           | Backend for Frontend | Rust + Axum        |
| [`hexagonal/`](./hexagonal)               | Hexagonal (ports & adapters) | Rust + Axum |

The first five share the same little e-commerce domain (users, catalog,
orders) on purpose — read them side by side to see how the *same* logic
connects differently when boundaries are enforced by the compiler, by the
network, by a broker, by splitting reads from writes entirely, or by an
explicit workflow orchestrator coordinating a distributed transaction.

`choreography/` and `bff/` both branch off that chain rather than continuing
it. `choreography/` is `saga/` with the orchestrator deleted — same domain,
same distributed transaction, same five order states, but no service drives
the workflow; each one reacts to facts and acts on its own authority. It's
the sharpest A/B in the repo: diff `saga/services/orders-service/src/saga.rs`
against `choreography/services/orders-service/src/tracker.rs` and the entire
trade is visible in one file. `bff/` branches off `microservices/` instead:
same backend, but now two different client-facing gateways decide how much of
it each client actually sees.

`hexagonal/` is the odd one out, and the smallest: it asks a question none of
the others do. Every lab above decides *where* boundaries go — between
modules, processes, or read and write sides. Hexagonal asks which **side** of
the boundary the technology sits on, and answers it by putting the order
rules in a crate that can't depend on axum, and then driving that one crate
from two unrelated programs (an HTTP server and a CLI) over two interchangeable
storage backends. Diff `hexagonal/orders-core/src/service.rs` against
`microservices/services/orders-service/src/service.rs`: the logic barely
moves, but everything around it does.

More to come as I explore other patterns.

## Layout

```
architecture-lab/
├── modular-monolith/     # one deployable, compiler-enforced module boundaries
├── microservices/        # three deployables, network-enforced boundaries (HTTP)
├── event-driven/         # four deployables, decoupled via a broker (NATS) instead of URLs
├── cqrs-es/              # five deployables; Orders splits into a command side (event-sourced) and a query side (projection)
├── saga/                 # six deployables; Orders orchestrates a saga across Inventory and Payments, with an explicit compensating action on failure
├── choreography/         # the same six, with the orchestrator removed; the workflow is emergent, compensation is self-triggered, and no service can say whether an order is done
├── bff/                  # five deployables; a web gateway and a mobile gateway each aggregate the same three backend services differently
└── hexagonal/            # one library that can't see the outside world, plus two programs that drive it (HTTP and CLI) over swappable storage
```

Each subdirectory has its own README explaining that architecture and how to run it.
