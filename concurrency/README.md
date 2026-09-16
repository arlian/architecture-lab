# architecture-lab — Anti-race-condition (concurrency control) in Rust

Every other lab in this repo draws a boundary and asks what crosses it. This one
asks a question all of them quietly assumed away:

> **two requests arrive at the same instant for the same row. Which of them is
> allowed to be wrong?**

Every lab here has a service that reads a number, decides something, and writes
it back. Every one of them is written as if it were the only caller. That
assumption is free right up until it isn't, and when it breaks it does not
produce an error — it produces a number that is quietly, permanently wrong.

## The core idea

One domain rule, four implementations, one contract, and a tool that proves
which of them are lying.

The rule: **never reserve more units than exist**. Four services implement it
behind byte-identical HTTP, so the runner can be pointed at any of them without
knowing which:

```text
GET  /stock/:product                  -> { strategy, product, available, version }
POST /stock/:product/seed    {units}  -> reset to a known state
POST /stock/:product/reserve {units, expected_version?}
                                      -> 200 granted | 404 | 409 | 422 | 503
```

- **The bug is a first-class citizen.** `naive-service` is not a strawman, it is
  the control group, and it is the most-commented file in the lab. Its test
  suite *asserts that it oversells*. If that test ever starts failing, someone
  has accidentally fixed it and the experiment has lost its baseline.
- **The race is reproducible on demand, not "sometimes".** The gap between the
  read and the write is a real `await` — `THINK_MS`, standing in for the payment
  call that is genuinely there in a real handler. Set it to 0 and the bug hides
  perfectly, which is the single most important thing this lab has to say about
  why this class of bug survives code review.
- **The fixes are incompatible, not ranked.** Detection (versions), exclusion
  (locks) and elimination (ownership) are three different bets about how often
  conflicts happen. None of them is the answer.
- **Correctness is measured in units, not in adjectives.** The runner seeds a
  known number, fires everything at once, then checks the books: units handed
  out versus units actually taken off the shelf. The difference is a lost
  update, priced in inventory.

## Layout

```
concurrency/
├── services/
│   ├── naive-service/         read-modify-write, no coordination        :3020
│   ├── optimistic-service/    version check + retry (compare-and-swap)   :3021
│   ├── pessimistic-service/   per-key mutex; callers queue               :3022
│   └── actor-service/         one task owns the data; no shared state    :3023
└── tools/
    └── race-runner/           fires N reservations at once, audits the books
```

Each service is four files, and exactly one of them matters:

| Service | THE FILE | The one-sentence version |
| --- | --- | --- |
| naive | `store.rs` | Read, think, write. Correct under one caller, wrong under two. |
| optimistic | `store.rs` | Write only if the version is still the one you read; else start over. |
| pessimistic | `store.rs` | Take the row's lock first; everyone else waits their turn. |
| actor | `actor.rs` | There is no shared state. One task owns the `HashMap`. |

Read the other three files in each service once and then stop — they are
deliberately near-identical copies. The diff between the four `store.rs` files
*is* the lab.

## The four strategies

| | naive | optimistic | pessimistic | actor |
| --- | --- | --- | --- | --- |
| Bet | there is one caller | conflicts are rare | conflicts are likely | don't share, ever |
| Mechanism | none | version compare-and-swap | mutex per key | message passing |
| Writers block each other? | no | no | **yes** | **yes** |
| Wasted work under contention | none (it's wrong) | **retries** | none | none |
| Failure mode | silent oversell | 409s and burnt CPU | queueing, then timeouts | 503s |
| Extra error variant | — | `Conflict` | — | `Busy` |
| Survives being replicated? | no | **yes**, if the CAS is in the store | no | no |
| Can refuse work? | no | no | no | **yes** |
| Forgetting it re-breaks things? | — | yes | yes | **no — unrepresentable** |

The last two rows are the ones worth arguing about, and they point in opposite
directions. Only the optimistic version still means anything once you run three
copies of the service: a mutex in one process is invisible to the other two,
while `UPDATE ... WHERE version = 7` is enforced wherever the data actually
lives. But only the actor makes the mistake impossible to reintroduce — there is
no lock to forget, because there is nothing to lock.

## How it actually flows

The race, drawn out. Two callers, ten units in stock, three each:

```text
naive                                  optimistic
─────                                  ──────────
A: read 10 ─┐                          A: read 10 @v3 ─┐
B: read 10 ─┤ both hold "10"           B: read 10 @v3 ─┤
A: think    │                          A: think        │
B: think    │                          B: think        │
A: write 7 ─┘                          A: CAS v3->v4 OK │  writes 7
B: write 7     <- A's write is gone    B: CAS v3? no, it's v4
                                       B: re-read 7, think, CAS v4->v5, writes 4
stock: 7.  six units sold.             stock: 4.  six units sold. correct.

pessimistic                            actor
───────────                            ─────
A: lock ─────────────┐                 A: ──msg──┐
B: lock ... waiting  │                 B: ──msg──┤ one mailbox, one reader
A: read 10, think    │                           │ handles A: 10 -> 7
A: write 7, unlock ──┘                           │ handles B:  7 -> 4
B: read 7, think, write 4              stock: 4. nothing was ever shared.
stock: 4.  B waited for A.
```

`optimistic` is the only column where nobody waits. `pessimistic` and `actor`
are the same picture drawn twice — which is the lab's most useful surprise, and
the reason the comparison table below has them next to each other.

## Running it

Requires only the Rust toolchain — no broker, no database, no Docker.

From this directory, open four terminals:

```bash
cargo run -p naive-service         # http://localhost:3020
cargo run -p optimistic-service    # http://localhost:3021
cargo run -p pessimistic-service   # http://localhost:3022
cargo run -p actor-service         # http://localhost:3023
```

On Windows, start all four at once:

```powershell
./run-all.ps1
```

Run the tests (every service independently; the race is reproduced
deterministically in-process, so no test opens a socket) and lint the lot:

```bash
cargo test
cargo clippy
```

### …or with Docker

```bash
docker compose up --build
docker compose run --rm runner --target http://naive:3020
```

Note that no service's environment names any other service. These four are not
a system; they are four answers to one question.

## Try it

The whole lab is one command:

```bash
cargo run -p race-runner -- --target all
```

```text
----------------------------------------------------------------
  target        naive  (http://localhost:3020)
  seeded        1 product(s) x 100 units seeded = 100 units on the shelf
  attack        60 concurrent x 2 units = 120 units requested
----------------------------------------------------------------
  200 granted       60
  409 conflict       0
  422 rejected       0
  503 busy           0
----------------------------------------------------------------
  units handed out     120
  units consumed         2   (100 seeded - 98 left)

  *** OVERSOLD BY 118 UNITS ***
  stock was promised to callers and never taken off the shelf.

  latency  p50 6ms   p99 9ms   whole run 41ms
----------------------------------------------------------------
```

Sixty callers, sixty confirmations, and the warehouse is down two mugs. Nothing
logged a warning. Every status code was 200. The p99 is excellent — this is the
fastest service in the lab, and it is fast because it is not doing the work.

Then the comparison at the bottom:

```text
  side by side
  strategy        granted conflicts  oversold    p50      p99      wall
  --------------------------------------------------------------------
  naive                60         0       118    6ms      9ms      41ms
  optimistic           50         2         0    9ms     74ms     121ms
  pessimistic          50         0         0  158ms    311ms     317ms
  actor                50         0         0  157ms    309ms     313ms

  `oversold` is the only column that is allowed to be non-zero nowhere.
  everything else is a trade-off you get to pick.
```

Four things to stare at:

1. **`granted` is 50 for everything that works.** 100 units, 2 each. The correct
   answer was never in doubt; only naive's arithmetic was.
2. **`pessimistic` and `actor` are the same numbers.** They are the same
   strategy. One writer at a time, expressed as a lock or as ownership. The
   actor is not faster; it is harder to get wrong.
3. **`optimistic` is three times quicker and shipped two 409s.** Nobody blocked,
   so the winners were fast — and two callers burnt through their retry budget
   and were handed the problem. That is the trade, in one row.
4. **naive is the fastest.** It always will be. Correctness has a price and
   every column but one is showing you what it costs.

The runner exits non-zero when anything was oversold, so it works as a CI check
and not only as a demo.

### Now turn the knobs

**Hide the bug**, the way it hides in your own test suite:

```bash
THINK_MS=0 cargo run -p naive-service
cargo run -p race-runner -- --target http://localhost:3020    # consistent!
```

Nothing was fixed. The window between the read and the write just got too small
to land in, on this machine, today. Put the 5ms back and it oversells again.

**Stop hiding the conflicts** the optimistic service is papering over:

```bash
RETRIES=0 cargo run -p optimistic-service
cargo run -p race-runner -- --target http://localhost:3021
```

Still zero oversold — the mechanism never needed the retries. But now one caller
wins and fifty-nine get a 409, which is the true contention on that row. The
retry loop was converting that into latency and CPU so nobody had to look at it.

**Let the client hold the version** instead of the server:

```bash
cargo run -p race-runner -- --target http://localhost:3021 --expect-version
```

Every request pins `expected_version` to what it last read, so the server
refuses to retry on anyone's behalf. This is `ETag` / `If-Match` with different
spelling, and it is the right shape whenever the client's *decision* — not just
its write — depended on the value it read.

**Make the lock too coarse**, the most common production version of this
mistake:

```bash
cargo run -p race-runner -- --target http://localhost:3022 --products 8
LOCK_SCOPE=global cargo run -p pessimistic-service
cargo run -p race-runner -- --target http://localhost:3022 --products 8   # same answers, far slower
```

Eight products that share nothing, one lock. Both runs are correct. One of them
has a throughput ceiling of one write at a time for the entire service, and
nothing in the code says so.

**Make the queue visible**:

```bash
MAILBOX=4 cargo run -p actor-service
cargo run -p race-runner -- --target http://localhost:3023 --concurrency 60
```

503s appear. The service is refusing work it cannot do in time. The other three
services are just as overloaded in that scenario and say nothing — their queue is
a pile of parked tasks that no counter anywhere is watching.

## The argument to have

**A 409 is a way of making your contention someone else's problem.**

optimistic-service's default is to retry eight times and hand back a 200. The
caller never learns it lost seven rounds; it just experiences a slow request. So
the service looks healthy — no error rate, no alarms — while burning eight times
the CPU per write on a hot row. The dashboard says fine, the bill says otherwise.

Setting `RETRIES=0` is the honest version, and it is worse for everyone: fifty-nine
409s that clients now have to implement backoff for, badly, in five different
languages, each with its own idea of how many times is too many.

There is no correct answer, only a placement decision: who absorbs contention —
the server (retries, latency, CPU), the client (409s, backoff, complexity), or
the user (a queue, then a timeout)? Pick one deliberately, because the default
is to pick it accidentally.

And a sharper one, worth changing the code over: **should `reserve` be
idempotent?** Right now, a client that times out and retries reserves *twice*.
Every fix in this lab prevents concurrent writes from colliding, and not one of
them prevents the same caller from being counted twice. That is a different
problem with a different solution — a deduplication key — and
`saga/services/inventory-service/src/repository.rs` already solves it, keyed by
`saga_id`. Read the two together: they look like the same problem and they are
not.

## A known gap: everything here is one process

This is the big one, and the lab leaves it wide open on purpose.

All four services hold their data in a `HashMap` in memory. Run two copies of
`pessimistic-service` behind a load balancer and it oversells exactly like
naive-service: the two processes have different mutexes, protecting different
maps, and neither knows the other exists. The same is true of the actor.

**Only the optimistic strategy survives the move**, and only if you put the
compare-and-swap where the data is:

```sql
UPDATE stock SET units = units - 2, version = version + 1
 WHERE product = 'mug' AND version = 7;
-- then check the affected row count. Zero means you lost; retry.
```

That is the same three lines as `optimistic-service/src/store.rs`, executed by
something all the replicas share. Which is the real lesson underneath this whole
lab: **coordination has to live where the data lives.** A lock is not a
technique, it is a location. Move the data and every in-process guarantee on
this page evaporates without a single compiler error.

The distributed versions of the other two are a distributed lock (Redis,
ZooKeeper, `SELECT ... FOR UPDATE`) and a partitioned actor (Orleans, Akka
Cluster Sharding, a Kafka partition per key) — both real, both considerably more
expensive than a version column, and both with a failure mode the in-process
version does not have: the lock holder dying while holding it.

## What changed vs. the other labs

| Concern | Everywhere else in this repo | Here |
| --- | --- | --- |
| Boundaries | between modules, services, read/write sides | between *concurrent callers of one row* |
| The interesting failure | a service is down, slow, or inconsistent | a service is up, fast, and wrong |
| How it surfaces | a 500, a timeout, a stale read | nothing at all |
| Detected by | the caller | an audit, weeks later |
| Fixed by | topology | four lines in `store.rs` |

`saga/` and `choreography/` are the closest relatives: both care about a command
being *delivered twice*, and both solve it with an idempotency ledger. This lab
cares about two *different* commands being processed at once. They are neighbours
and they are not the same, which is exactly why doing one does not protect you
from the other.

## Where to take it next (learning exercises)

- **Make `reserve` idempotent.** Add a client-supplied `request_id`, remember
  the outcome, and return the same answer to a redelivery instead of reserving
  twice. Then notice that the ledger you just added is itself shared mutable
  state, and needs every protection this lab is about.
- **Shard the actor.** One actor per product, addressed by key, spawned on
  demand. It turns actor-service into pessimistic-service's `LOCK_SCOPE=key`
  from the opposite direction — and the registry of live actors is a new piece
  of shared state that needs a lock. Notice where the complexity went rather
  than assuming it left.
- **Move the store into Postgres** and rerun everything. Three of the four
  strategies stop meaning anything; one keeps working unchanged. That single
  exercise is worth more than the rest of this list.
- **Run two replicas of pessimistic-service** behind any load balancer and watch
  it oversell. Ten minutes, and it permanently changes how you read the phrase
  "we use a mutex there".
- **Add a lock timeout** to pessimistic-service and a 503 when it expires. Now
  it can shed load like actor-service, and now it has a new failure: giving up
  on a lock you would have got in another millisecond.
- **Write the deadlock.** Add `POST /transfer` that locks two products and moves
  units between them, then fire A→B and B→A simultaneously. Nothing errors,
  nothing logs, the requests simply never finish. Then fix it by always
  acquiring locks in sorted key order, and note that nothing in the language
  enforces that convention.
- **Replace `version` with a real ETag.** Return it as an HTTP header, accept
  `If-Match`, and return 412 instead of 409. The mechanism does not change at
  all; only the amount of it you had to invent yourself does.
- **Measure, don't reason.** Run the runner at `--concurrency 5`, `50`, and
  `500` against optimistic and pessimistic, and find the crossover point where
  optimism stops paying. It exists, it moves with `THINK_MS`, and knowing it
  exists is most of knowing which one to reach for.
- **Harden the containers**, same exercise as the other labs: healthchecks,
  `condition: service_healthy`, non-root users, pinned digests.
