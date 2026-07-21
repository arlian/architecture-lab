# architecture-lab

A personal lab for learning software architecture patterns hands-on. Each
architecture lives in its own self-contained subdirectory with its own build.

## Architectures

| Directory                                 | Pattern          | Stack             |
| ----------------------------------------- | ---------------- | ----------------- |
| [`modular-monolith/`](./modular-monolith) | Modular monolith | Rust + Axum        |
| [`microservices/`](./microservices)       | Microservices    | Rust + Axum        |
| [`event-driven/`](./event-driven)         | Event-driven     | Rust + Axum + NATS |

All three share the same little e-commerce domain (users, catalog, orders) on
purpose — read them side by side to see how the *same* logic connects
differently when boundaries are enforced by the compiler, by the network, or
by a broker.

More to come as I explore other patterns (e.g. hexagonal, CQRS).

## Layout

```
architecture-lab/
├── modular-monolith/     # one deployable, compiler-enforced module boundaries
├── microservices/        # three deployables, network-enforced boundaries (HTTP)
└── event-driven/         # four deployables, decoupled via a broker (NATS) instead of URLs
```

Each subdirectory has its own README explaining that architecture and how to run it.
