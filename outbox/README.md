# outbox — transactional outbox + inbox

Every NATS lab in this repo saves a row and then publishes an event. Those are
two writes to two systems, and a crash between them loses the event silently
(`event-driven/services/users-service/src/service.rs` already admits it). This
lab puts a number on that loss, and then fixes it.

```
audit-runner ──POST /orders──▶ orders-service (MODE=naive  :3030) ──POST /events──▶ ledger-service :3040
                              orders-service (MODE=outbox :3031) ──relay───────▶
```

| Mode     | Write path                                    | Crash lands…                     | Result                     |
| -------- | --------------------------------------------- | -------------------------------- | -------------------------- |
| `naive`  | commit order → publish                         | after commit, before publish     | **lost** events            |
| `outbox` | commit order + outbox row in one transaction; relay publishes → marks sent | after publish, before mark sent | **duplicates**, never loss |

The ledger keeps its books twice from the same deliveries: without an inbox
(apply everything) and with one (skip an `event_id` it has already applied).
Outbox + inbox is the combination that reconciles exactly.

## Run

```powershell
./run-all.ps1
cargo run -p audit-runner
```

Expect `naive` to report roughly `CRASH_RATE × orders` lost, and `outbox` to
report zero lost, some duplicates, and a ledger that only matches the orders
table in the "with inbox" column. The runner exits 1 when anything is lost.

Knobs: `CRASH_RATE` (default 0.1) on orders-service; `--orders`, `--amount`
on the runner.

## Read in this order

1. `services/orders-service/src/write.rs` — both strategies, side by side.
2. `services/orders-service/src/relay.rs` — where the dual write moved to.
3. `services/ledger-service/src/main.rs` — the inbox, in four lines.

## Deliberate shortcuts

- The database is a `Mutex` over two tables; holding it is the transaction.
- The broker is an HTTP POST. The problem is the same with NATS.
- Crashes are coin flips, not killed processes.
- The relay polls. Production relays often tail the database log instead (CDC).
