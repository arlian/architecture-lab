# architecture-lab

A personal lab for learning software architecture patterns hands-on. Each
architecture lives in its own self-contained subdirectory with its own build.

## Architectures

| Directory                                 | Pattern          | Stack       |
| ----------------------------------------- | ---------------- | ----------- |
| [`modular-monolith/`](./modular-monolith) | Modular monolith | Rust + Axum |
| [`microservices/`](./microservices)       | Microservices    | Rust + Axum |

The two share the same little e-commerce domain (users, catalog, orders) on
purpose — read them side by side to see how the *same* logic connects
differently when boundaries are enforced by the compiler vs. by the network.

More to come as I explore other patterns (e.g. hexagonal, event-driven, CQRS).

## Layout

```
architecture-lab/
├── modular-monolith/     # one deployable, compiler-enforced module boundaries
└── microservices/        # three deployables, network-enforced boundaries (HTTP)
```

Each subdirectory has its own README explaining that architecture and how to run it.
