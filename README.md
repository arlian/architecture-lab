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

All four share the same little e-commerce domain (users, catalog, orders) on
purpose — read them side by side to see how the *same* logic connects
differently when boundaries are enforced by the compiler, by the network, by
a broker, or by splitting reads from writes entirely.

More to come as I explore other patterns (e.g. hexagonal).

## Layout

```
architecture-lab/
├── modular-monolith/     # one deployable, compiler-enforced module boundaries
├── microservices/        # three deployables, network-enforced boundaries (HTTP)
├── event-driven/         # four deployables, decoupled via a broker (NATS) instead of URLs
└── cqrs-es/              # five deployables; Orders splits into a command side (event-sourced) and a query side (projection)
```

Each subdirectory has its own README explaining that architecture and how to run it.
