# architecture-lab

A personal lab for learning software architecture patterns hands-on. Each
architecture lives in its own self-contained subdirectory with its own build.

## Architectures

| Directory                              | Pattern           | Stack        |
| -------------------------------------- | ----------------- | ------------ |
| [`modular-monolith/`](./modular-monolith) | Modular monolith  | Rust + Axum  |

More to come as I explore other patterns (e.g. hexagonal, event-driven,
microservices, CQRS).

## Layout

```
architecture-lab/
└── modular-monolith/     # one deployable, compiler-enforced module boundaries
```

Each subdirectory has its own README explaining that architecture and how to run it.
