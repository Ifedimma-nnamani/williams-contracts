# ADR 0002: `total_locked_by_token` — Bounded Vec vs. Unbounded Growth

## Status

Accepted and Implemented (Issue #22).

Per-token locked totals are stored as discrete **persistent** storage entries
(`DataKey::TotalLocked(Address)`), keeping the per-transaction read/write
footprint and gas consumption of a locked-total update constant in the number
of distinct protocol tokens. The original `ProtocolStats.total_locked_by_token`
vector is retained only as a legacy, read-only fallback.

## Adopted decision

The discrete persistent-key design — a variant of **Option C** below, using
`DataKey::TotalLocked(Address)` in persistent storage rather than a separate
instance key — was adopted in #22 and is the current implementation. Options A
and B were considered and rejected; they are preserved here for historical
context only. Any statement in this document that describes a `Vec`-scan or a
`Map<Address, i128>` inside `ProtocolStats` as "current" or "recommended" is
superseded by this section.

### Storage namespaces

| Key | Namespace | Purpose |
| --- | --- | --- |
| `DataKey::TotalLocked(Address)` | **Persistent** | Per-token aggregate of locked amounts. Each entry has its own TTL and is bumped on write. |
| `DataKey::ProtocolTokens` | **Instance** | `Vec<Address>` directory of tokens with a non-zero locked total. |
| `DataKey::ActiveWillCount` | **Instance** | Count of active wills, maintained alongside the token directory. |
| `DataKey::ProtocolStats` | **Instance** | Legacy `ProtocolStats` entry (including the old `total_locked_by_token` vector). Read-only fallback; never written by the current code path. |

Persistent entries are independent ledger entries: reading or writing one
token's total does not deserialise any other token's total. Instance entries
are read as a single ledger entry, so the token directory and the active-will
count share the instance entry's read/write cost.

## Context

`ProtocolStats` historically carried a `total_locked_by_token:
Vec<TokenLockedBalance>` that aggregated, for every distinct token ever locked
by any will, the current sum of amounts held across all active wills. It was
stored in **instance storage** and updated on every `create_will`, `top_up`,
and `cancel_will`.

The helper that updates it is `storage::adjust_locked_value`:

```rust
// storage.rs — current implementation
pub fn adjust_locked_value(env: &Env, token: &Address, delta: i128) {
    let key = DataKey::TotalLocked(token.clone());
    let current = get_total_locked(env, token);
    let updated = current + delta;
    env.storage().persistent().set(&key, &updated);
    bump_persistent(env, &key);

    // Maintain the instance-storage token directory.
    let mut tokens = get_protocol_tokens(env);
    let mut found = false;
    let mut rebuilt = Vec::new(env);
    for entry in tokens.iter() {
        if entry == *token {
            found = true;
            if updated != 0 {
                rebuilt.push_back(entry);
            }
        } else {
            rebuilt.push_back(entry);
        }
    }
    if !found && updated != 0 {
        rebuilt.push_back(token.clone());
    }
    if rebuilt.len() != tokens.len() {
        env.storage().instance().set(&DataKey::ProtocolTokens, &rebuilt);
    }
}
```

`adjust_locked_for_balances` iterates `will.balances` and calls
`adjust_locked_value` once per token, so multi-token wills are decremented on
cancellation, release and distribution.

Two costs scale with `N`, the number of **distinct token addresses with a
non-zero locked total**:

1. **CPU instructions** — the persistent read/write is `O(1)` in `N`, but the
   token-directory rebuild in instance storage is `O(N)` and runs on the first
   insert of a token and on removal of a token whose total reaches zero.
2. **Read and write bytes** — the persistent entry is a single `i128` per
   token, so its cost is independent of `N`. The instance `ProtocolTokens`
   vector is rewritten whenever the directory changes, so its byte cost grows
   as `O(N × sizeof(Address))` on those calls.

`N` is still unbounded: any address on the Stellar network can create a will
that references a novel token, adding one entry to the directory. However, the
cost of that growth is now confined to the calls that change the directory
(first insert, removal to zero) rather than to every locked-total update.

### Additional issue discovered during analysis

`cancel_will` used to call `adjust_locked_value` only for `will.token` (the
legacy single-token field retained for backward compatibility), not for all
entries in `will.balances`.  For a multi-token will this meant the
`total_locked` counters for every non-primary token were **never decremented on
cancellation**, so the aggregate totals drifted upward over time.  **Fixed in
#353**: `cancel_will` now decrements `adjust_locked_value` for every token in
its `balances` snapshot.  The same omission may still exist in
`release_inheritance` / `distribute` (no call to `adjust_locked_value` was
found there at all).  That is a correctness bug compounding the scalability
issue: the vector grows unboundedly *and* its values stay wrong for
multi-token wills that are released rather than cancelled.

## Decision drivers

* Soroban charges for bytes read and written per invocation; a growing entry
  inflates the fee of every call that touches `ProtocolStats`, not just the
  ones that add new tokens.
* Instance storage is read as a single ledger entry; there is no partial-read
  API.  A large `total_locked_by_token` vector means the entire entry is
  deserialised on every call, even if only one token needs updating.
* The contract already caps per-will token diversity at `MAX_TOKENS = 10`.
  That cap does *not* bound `N` — it only limits how many tokens a single new
  will can add in one call.
* An off-chain consumer (SDK, explorer) only needs the aggregate totals for
  `get_protocol_stats`; it does not need them to be recomputed in-process.

## Options considered

### Option A — Enforce a hard cap on `N` (document / enforce)

Add a constant `MAX_PROTOCOL_TOKENS: u32` checked inside `adjust_locked_value`
before inserting a new entry.  If the cap is reached, either:

* (A1) panic with a new `WillError::TooManyProtocolTokens`, or
* (A2) silently skip the update (the total becomes approximate but the call
  succeeds).

**Pros:** trivial to implement; makes worst-case cost predictable at
compile time; no data-structure migration.

**Cons (A1):** a sufficiently popular protocol with many distinct tokens would
eventually refuse new `create_will` calls for novel tokens, which is a
liveness issue the owner cannot work around.

**Cons (A2):** silent data loss; aggregate totals become unreliable.

### Option B — Switch `total_locked_by_token` to a `Map<Address, i128>` (rejected)

Replace `Vec<TokenLockedBalance>` with a Soroban `Map<Address, i128>` keyed by
token address.  The update becomes a single `get`/`set` pair instead of a
full-scan rebuild:

```rust
// proposed
pub fn adjust_locked_value(env: &Env, token: &Address, delta: i128) {
    let mut stats = get_protocol_stats(env);
    let current = stats.total_locked_by_token.get(token.clone()).unwrap_or(0);
    stats.total_locked_by_token.set(token.clone(), current + delta);
    save_protocol_stats(env, &stats);
}
```

CPU cost drops from `O(N)` to `O(log N)` (Soroban `Map` is an ordered map with
`O(log N)` access).  Byte traffic still grows as more distinct tokens are added,
because the whole `ProtocolStats` entry is still serialised as one ledger entry,
but that is now irreducible: any representation that stores N token totals must
carry at least N entries.

**Pros:** eliminates the avoidable quadratic work; consistent with how `Will`
already stores `balances: Map<Address, i128>`; no new error variants or liveness
risk; the `Vec<TokenLockedBalance>` type returned by `get_protocol_stats` can
be reconstructed on read if backward-compatible serialisation is needed.

**Cons:** requires a storage-layout migration for the `ProtocolStats` instance
entry (the `contracttype` encoding of `Map` differs from `Vec`); the byte
footprint of instance storage still grows with `N`, just unavoidably so.
**Rejected:** the whole `ProtocolStats` entry would still be deserialised on
every update, so byte traffic would remain `O(N)` per call.

### Option C — Move per-token totals out of instance storage (adopted, persistent-key variant)

Store each token's aggregate as a separate storage key
`DataKey::TotalLocked(Address)`.  `adjust_locked_value` then reads and writes
exactly one small entry regardless of how many distinct tokens the protocol
has ever seen.  `get_protocol_stats` becomes a scan across all
`TotalLocked` keys, which is not directly supported by the Soroban storage
API (no enumerate/scan).

**Pros:** per-call read/write bytes are constant in `N`; no single bloated
entry; the per-token entry is a single `i128` with its own TTL.

**Cons:** Soroban's storage API does not expose key enumeration, so
`get_protocol_stats` must reconstruct the list from an explicit index. The
implementation keeps that index as `DataKey::ProtocolTokens` (a `Vec<Address>`
in instance storage), so the directory cost scales with the number of distinct
active tokens and is rewritten on first insert and on removal to zero.

**Adopted in its persistent-key form:** `DataKey::TotalLocked(Address)` lives
in persistent storage (not instance storage), with `DataKey::ProtocolTokens`
and `DataKey::ActiveWillCount` in instance storage. See "Adopted decision"
above.

## Implemented design

**Option C (persistent-key variant)**, as shipped in #22.

1. `DataKey::TotalLocked(Address)` holds the per-token aggregate in persistent
   storage. `adjust_locked_value` reads it via `get_total_locked`, adds the
   delta, writes it back and bumps the persistent TTL.
2. `DataKey::ProtocolTokens` (instance storage) is the token directory. It is
   rewritten only when the set of tokens with a non-zero total changes: on the
   first insert of a token and on removal of a token whose total reaches zero.
3. `DataKey::ActiveWillCount` (instance storage) tracks the number of active
   wills.
4. `DataKey::ProtocolStats` (instance storage) is retained as a legacy,
   read-only fallback. `get_protocol_stats` reconstructs the public
   `ProtocolStats` view from the persistent totals and the token directory,
   falling back to the legacy vector only when the new keys are absent.
5. `cancel_will`, `release_inheritance` and `distribute` call
   `adjust_locked_for_balances`, which invokes `adjust_locked_value` for
   **every token in `will.balances`**, not just `will.token`.

### Legacy fallback reads

`get_total_locked` resolves a token's total in this order:

1. `DataKey::TotalLocked(Address)` in persistent storage;
2. the same key in instance storage (pre-migration layout);
3. the legacy `ProtocolStats.total_locked_by_token` vector.

`get_protocol_tokens` and `get_active_will_count` similarly fall back to the
legacy `ProtocolStats` entry when their instance keys are absent. These
fallbacks exist only to read state written before #22; the current code never
writes the legacy vector.

### Migration authority

* The legacy `ProtocolStats` entry is a **read-only fallback**. It must not be
  reintroduced as a write target; new code must write only the discrete keys.
* Schema changes to `DataKey`, `ProtocolStats` or the token directory require
  the documented migration path: a contract upgrade plus an admin-only
  migration entry point that rewrites the affected keys and leaves the legacy
  entry untouched.
* Migrations are performed by the contract admin through the upgrade path
  only; no other caller may mutate the storage layout.
* Operators should monitor the size of `DataKey::ProtocolTokens`; the protocol
  works correctly at any `N`, but the directory rewrite cost grows with the
  number of distinct active tokens.

### Why not Option A

Capping `N` at a fixed number and panicking creates a latent liveness hazard:
once the cap is hit, no owner can create a will with a token not already in the
vector, regardless of how many of those existing wills are still active.  On
a long-lived protocol this is unacceptable.  The `MAX_TOKENS = 10` per-will cap
provides a natural rate-of-growth bound (`N` grows by at most 10 per `create_will`
call) without creating a hard ceiling on `N` itself.

## Consequences

### As implemented (Option C, persistent-key variant)

* `adjust_locked_value` performs an `O(1)` persistent read/write of a single
  `i128` per token, independent of `N`.
* The `ProtocolTokens` directory in instance storage is rewritten only when
  the set of tokens with a non-zero total changes, so its `O(N)` cost is paid
  on first insert and on removal to zero rather than on every update.
* `ActiveWillCount` and the token directory share the instance entry's
  read/write cost.
* The legacy `ProtocolStats` entry is read only as a fallback; it is never
  written by the current code path.
* `get_protocol_stats` reconstructs `total_locked_by_token:
  Vec<TokenLockedBalance>` from the persistent totals and the token directory,
  so callers see no breaking change.

### If the discrete-key design had not been adopted

Every `create_will`, `top_up`, and `cancel_will` call would become permanently
and unavoidably more expensive as the protocol matures, with no action the
caller could take to avoid it. At ~100 distinct tokens the extra
deserialization would be a minor annoyance; at ~1 000 it would be a meaningful
fraction of the total invocation fee.

## Related

* `docs/RESOURCE_COSTS.md` — profiles `create_will` cost at baseline; a
  supplementary scenario measuring cost as `N` grows is tracked there.
* `contracts/will/src/storage.rs` — `DataKey`, `get_total_locked`,
  `adjust_locked_value`, `adjust_locked_for_balances`, `get_protocol_tokens`,
  `get_active_will_count`, `get_protocol_stats`, `save_protocol_stats`.
* `contracts/will/src/types.rs` — `ProtocolStats`, `TokenLockedBalance`.
* `contracts/will/src/lib.rs` — `create_will` (calls `adjust_locked_value` per
  token), `cancel_will`, `release_inheritance` and `distribute` (call
  `adjust_locked_for_balances` for every token in `will.balances`), `top_up`
  (calls it once for the topped-up token).
