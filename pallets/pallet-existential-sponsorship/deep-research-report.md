# Account Sponsorship Pallet Technical Specification

## Executive summary

This specification defines a Substrate/Polkadot SDK FRAME pallet that enables **account sponsorship**: a sponsor locks native-token funds so a beneficiary account can **exist on-chain** (i.e., avoid reaping due to being below the existential deposit) without the beneficiary needing to acquire tokens themselves. The locked amount **appears in the beneficiary account as held/reserved (i.e., non-spendable)** and is **not transferable or usable for fees** by the beneficiary. Only the sponsor can manually unlock the sponsored funds, at which point the funds are returned to the sponsor.

The pallet uses the **Polkadot SDK fungible “holds” mechanism** (preferred over legacy reserves, which are deprecated in favor of holds) and relies on transactional storage to keep state changes atomic when combining balance transfers and hold placement.

Optional behaviors are included and explicitly configurable: (a) sponsor-opt-in **auto-unlock** when the beneficiary’s own (non-sponsored) balance reaches a threshold, and (b) sponsor-configurable **expiry/lease** so sponsorships can automatically end after a bounded lifetime (useful for faucet-like sponsors). Sponsor safety constraints include **per-sponsor caps**, **per-block caps**, and a **minimum remaining sponsor balance** rule to avoid over-commitment.

## Goals and non-goals

**Goals**

The pallet must:

- Allow any account to become a **sponsor** and lock funds “on behalf of” any **beneficiary** account so the beneficiary account remains live by meeting the chain’s minimum-balance / existential-deposit constraints.
- Represent sponsorship using **holds** so the locked amount is visible as a non-spendable portion consistent with the ecosystem’s notions of free/spendable vs held/reserved balance.
- Ensure sponsored funds are **non-transferable and non-usable** by the beneficiary (including for fee payment), and only become movable again if unlocked by the sponsor or via an enabled policy.
- Provide a **well-defined API surface**:
  - user-facing extrinsics,
  - runtime-facing functions callable by other pallets,
  - (optional) runtime API for RPC clients.
- Include safety limits:
  - configurable cap(s) per sponsor (total active sponsored amount and/or count),
  - configurable per-block cap(s),
  - configurable sponsor “minimum remaining balance” requirement.
- Support transactional atomicity for combined balance/hold operations.

**Non-goals**

- This pallet does **not** solve transaction fee sponsorship for beneficiary-origin transactions. Beneficiaries whose only balance is held/reserved are expected to have **0 spendable balance** and thus typically cannot pay fees under the standard transaction-payment model.
- This pallet does not attempt to create “native-token-free” accounts using the `sufficients` reference counter mechanism. (That mechanism exists for accounts that can remain alive via sufficient non-native assets; this pallet specifically sponsors via native token holds.)

## Integration with balances, holds, and account lifecycle

**Existential deposit and account reaping**

- The Balances pallet describes the **existential deposit (ED)** as the minimum balance needed to create/keep an account alive; if total balance (free + reserved/held) falls below ED, the account can be reaped.
- Polkadot documentation emphasizes ED as an anti-state-bloat mechanism.

**Balance categories relevant to sponsorship**

- “Reserved/held” balance is treated as owned but **suspended**; it cannot be used for transfers or fee payment, and is commonly used for deposits and staking-related mechanics.
- “Spendable” balance is derived from free balance after considering locks/freezes, reserved/held, and ED; spendable balance is what can be transferred and used for fees.

**Why “holds” over legacy “reserves”**

- The Polkadot SDK and related tooling indicate that **named reserves are deprecated in favor of holds**, and “locks” are deprecated in favor of freezes.
- `pallet_balances` notes heavy use of holds and freezes and that legacy `Currency`-family traits are deprecated and expected to be removed over time, motivating design against the fungible traits rather than tight coupling.

**Operational mechanism used by this pallet**

This pallet’s normative design uses:

- `fungible::hold::Mutate::transfer_and_hold` to move funds from sponsor to beneficiary and immediately place them on hold for a pallet-specific hold reason, in a single conceptual operation. This method explicitly warns it may error after partial mutation and should be used within a transactional storage context.
- `fungible::hold::Mutate::transfer_on_hold` to return held funds from the beneficiary back to the sponsor (as free funds), using `Restriction::Free` to ensure the destination receives free balance rather than held balance.

**Fee interactions**

- The transaction-payment pallet secures fee withdrawal **before** running the dispatchable, and the fungible adapter explicitly describes this “withdraw fee before execution” model.
- Because held/reserved funds are not spendable, a beneficiary with only sponsored held balance is expected (under standard fee logic) to have insufficient spendable funds to pay transaction fees.

## Pallet specification

### Conceptual model

A **sponsorship** is a relation:

- One **sponsor** account
- One **beneficiary** account
- One **sponsored amount** `A` (defaults to at least `Currency::minimum_balance()`, i.e., ED)
- One **hold reason** dedicated to this pallet (integrated via the runtime’s `RuntimeHoldReason` type used by `pallet_balances`).
- Optional **auto-unlock policy**
- Optional **expiry policy** (lease) expressed as block numbers

This spec assumes **at most one active sponsorship per beneficiary** (simplifies correctness, avoids multi-sponsor accounting collisions, and aligns with a single “account sponsor” mental model). Extension to multiple sponsors is explicitly out of scope for this version.

### Configuration trait

The pallet must be generic over a fungible token implementation that supports holds and transfers (typically the native Balances pallet). The runtime integrates “hold reasons” through `RuntimeHoldReason`.

Rust-like signature (illustrative; identifiers may be adapted to current Polkadot SDK conventions):

```rust
#[pallet::config]
pub trait Config: frame_system::Config {
  type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

  /// The fungible token used for sponsorship (typically Balances).
  /// Must support holds with reasons and hold transfers.
  type Currency: frame_support::traits::tokens::fungible::Inspect<Self::AccountId>
    + frame_support::traits::tokens::fungible::hold::Inspect<Self::AccountId>
    + frame_support::traits::tokens::fungible::hold::Mutate<Self::AccountId>;

  /// Conversion into the runtime-wide hold reason.
  /// RuntimeHoldReason in Balances is the "overarching hold reason".
  type RuntimeHoldReason: Parameter + Member + MaxEncodedLen + Copy;
  /// Pallet-local hold reason enum, convertible into RuntimeHoldReason.
  type PalletHoldReason: Into<Self::RuntimeHoldReason> + Copy;

  /// WeightInfo for benchmarking-derived weights.
  type WeightInfo: WeightInfo;

  /// Sponsorship minimum amount. Must be >= Currency::minimum_balance() (ED).
  /// Currency::minimum_balance is the minimum any single account may have.
  #[pallet::constant]
  type MinSponsorship: Get<BalanceOf<Self>>;

  /// Sponsor must keep at least this much reducible/free balance after creating sponsorship(s).
  #[pallet::constant]
  type SponsorMinRemaining: Get<BalanceOf<Self>>;

  /// Cap the total active sponsored amount per sponsor.
  #[pallet::constant]
  type MaxTotalSponsoredPerSponsor: Get<BalanceOf<Self>>;

  /// Cap the number of active sponsorships per sponsor.
  #[pallet::constant]
  type MaxSponsoredAccountsPerSponsor: Get<u32>;

  /// Cap total sponsorship amount created per sponsor per block.
  #[pallet::constant]
  type MaxSponsoredAmountPerBlock: Get<BalanceOf<Self>>;

  /// Cap the number of new sponsorships created per sponsor per block.
  #[pallet::constant]
  type MaxNewSponsorshipsPerBlock: Get<u32>;

  /// Maximum allowed lease duration (in blocks) if expiry is enabled.
  #[pallet::constant]
  type MaxLeaseDuration: Get<BlockNumberFor<Self>>;

  /// Limit how many auto-unlock/expiry checks can be processed per block.
  #[pallet::constant]
  type MaxQueueProcessingPerBlock: Get<u32>;

  /// Optional: deposit to mitigate storage bloat, held on the sponsor and returned on removal.
  #[pallet::constant]
  type SponsorshipStorageDeposit: Get<BalanceOf<Self>>;

  /// Privileged origin for force operations (typically Root).
  type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
}
```

Notes:

- The use of holds with reasons relies on runtime-wide hold-reason integration (`RuntimeHoldReason`), which is explicitly present in `pallet_balances::Config` as the overarching hold reason.
- Where legacy reserves/locks appear in runtimes, they are deprecated in favor of holds/freezes; this pallet should implement against holds.

### Types

```rust
pub type BalanceOf<T> =
  <<T as Config>::Currency as frame_support::traits::tokens::fungible::Inspect<
    <T as frame_system::Config>::AccountId
  >>::Balance;

pub type SponsorshipId = u64; // Optional; not required if beneficiary is unique key.
```

Auto-unlock policy types:

```rust
pub enum AutoUnlockMode {
  Disabled,
  Enabled(AutoUnlockCondition),
}

pub enum AutoUnlockCondition {
  /// Unlock when beneficiary's spendable/reducible balance (fees spendable) >= sponsored amount.
  /// Spendable balance is used for fees.
  SpendableGeSponsored,

  /// Unlock when beneficiary's total free balance (including locked but excluding holds) >= sponsored amount.
  /// (Implementation may require token-specific inspection; see integration notes.)
  FreeGeSponsored,
}
```

Expiry/lease policy types:

```rust
pub enum Lease {
  /// Never expires automatically.
  None,
  /// Expires at (inclusive) block number; on expiry, sponsorship is revoked and funds returned.
  Until(BlockNumber),
}
```

### Storage schema

Storage must support:

- Query by beneficiary (is account sponsored? sponsor? amount? policies?)
- Sponsor accounting (total sponsored amount, sponsored count)
- Per-block rate limiting
- Optional queues/indexes for auto-unlock and expiry processing

Concrete schema (Rust-like; actual hashing can follow common FRAME defaults):

```rust
/// Beneficiary -> Sponsorship record. One active sponsorship per beneficiary.
#[pallet::storage]
pub type Sponsorships<T: Config> = StorageMap<
  _,
  Blake2_128Concat,
  T::AccountId,                // beneficiary
  SponsorshipRecord<T>,
  OptionQuery
>;

pub struct SponsorshipRecord<T: Config> {
  pub sponsor: T::AccountId,
  pub amount: BalanceOf<T>,
  pub auto_unlock: AutoUnlockMode,
  pub lease: Lease<BlockNumberFor<T>>,
  pub created_at: BlockNumberFor<T>,
}

/// Sponsor -> aggregate accounting totals.
#[pallet::storage]
pub type SponsorLedger<T: Config> = StorageMap<
  _,
  Blake2_128Concat,
  T::AccountId,                // sponsor
  SponsorState<BalanceOf<T>>,
  ValueQuery
>;

pub struct SponsorState<Balance> {
  pub active_amount: Balance,
  pub active_count: u32,
  pub last_block: u32,         // or BlockNumberFor<T> encoded
  pub sponsored_amount_in_block: Balance,
  pub new_sponsorships_in_block: u32,
}

/// Optional: sponsorship storage deposit tracked per beneficiary (so it can be released).
#[pallet::storage]
pub type StorageDepositHeld<T: Config> = StorageMap<
  _,
  Blake2_128Concat,
  T::AccountId,      // beneficiary
  BalanceOf<T>,
  ValueQuery
>;

/// Optional: expiry index, so 'on_initialize' can process expiring items without scanning all.
/// BlockNumber -> bounded list of beneficiaries expiring at that block.
#[pallet::storage]
pub type Expirations<T: Config> = StorageMap<
  _,
  Blake2_128Concat,
  BlockNumberFor<T>,
  BoundedVec<T::AccountId, T::MaxQueueProcessingPerBlock>,
  ValueQuery
>;

/// Optional: auto-unlock queue (best-effort / eventual checking).
#[pallet::storage]
pub type AutoUnlockQueue<T: Config> = StorageValue<
  _,
  BoundedVec<T::AccountId, T::MaxQueueProcessingPerBlock>,
  ValueQuery
>;
```

Rationale:

- Avoid indefinite iteration over all sponsorships in hooks; use bounded vectors and per-block processing limits to bound weight.
- Rely on hold availability constraints (some tokens limit concurrent holds) and surface explicit errors when hold placement cannot occur.

### Extrinsics API

All extrinsics must be weight-metered via `T::WeightInfo` and should use `#[transactional]` when they combine currency/hold operations and storage updates, since hold operations (notably `transfer_and_hold`) may return an error after partial mutation if not transactional.

#### Create sponsorship

```rust
/// Sponsor (origin) creates a sponsorship, transferring `amount` and placing it on hold in beneficiary.
#[pallet::call]
#[pallet::weight(T::WeightInfo::sponsor())]
#[transactional]
pub fn sponsor(
  origin: OriginFor<T>,
  beneficiary: T::AccountId,
  amount: BalanceOf<T>,
  auto_unlock: AutoUnlockMode,
  lease: Lease<BlockNumberFor<T>>,
) -> DispatchResult;
```

Normative behavior:

- Origin must be signed; `sponsor = ensure_signed(origin)`.
- Must reject `beneficiary` if already has an active sponsorship (`Sponsorships.contains_key(beneficiary)`), returning `Error::AlreadySponsored`.
- Must enforce `amount >= T::MinSponsorship` and `T::MinSponsorship >= Currency::minimum_balance()` (ED). `minimum_balance()` is the minimum any account may have.
- Must enforce sponsor caps:
  - `SponsorLedger.active_amount + amount <= MaxTotalSponsoredPerSponsor`
  - `SponsorLedger.active_count + 1 <= MaxSponsoredAccountsPerSponsor`
  - Per-block limits: track `last_block`; if current block differs, reset per-block counters; then enforce `sponsored_amount_in_block + amount <= MaxSponsoredAmountPerBlock` and `new_sponsorships_in_block + 1 <= MaxNewSponsorshipsPerBlock`.
- Must enforce sponsor minimum remaining balance:
  - Evaluate sponsor’s reducible/spendable capacity conservatively and require that the sponsor’s post-operation balance will not violate `SponsorMinRemaining`.
  - Transfers that must keep the sponsor alive should use `Preservation::Preserve` semantics where applicable; `Preservation::Preserve` indicates the account may not be killed and must not be dusted (provider reference remains).
- Must validate lease:
  - `Lease::None` allowed.
  - `Lease::Until(b)` must satisfy `b >= current_block` and `b - current_block <= MaxLeaseDuration`.
- Must place funds on hold such that beneficiary cannot spend/transfer them:
  - Use `Currency::transfer_and_hold(reason, sponsor, beneficiary, amount, Precision::Exact, Preservation::Preserve (for sponsor), Fortitude::Polite)` (exact signature depends on whether single-token or fungibles traits are used; the hold trait explicitly supports transfer-and-hold and warns about transactional usage).
- Optional storage deposit:
  - If `SponsorshipStorageDeposit > 0`, hold an additional amount on the sponsor (or transfer-and-hold it into beneficiary under another reason) to pay for storage footprint and return it upon sponsorship removal. This mirrors common deposit patterns for state bloat control, which in the ecosystem are commonly done via reserved/held deposits.
- Record `Sponsorships[beneficiary] = SponsorshipRecord{...}` and update sponsor ledger totals.
- If lease enabled, append `beneficiary` to `Expirations[expiry_block]`.
- If auto-unlock enabled, insert/append beneficiary into `AutoUnlockQueue` (best-effort processing).
- Emit `Event::Sponsored { sponsor, beneficiary, amount, auto_unlock, lease }`.

Convenience extrinsic (ED amount):

```rust
#[pallet::call]
#[pallet::weight(T::WeightInfo::sponsor_minimum())]
#[transactional]
pub fn sponsor_minimum(
  origin: OriginFor<T>,
  beneficiary: T::AccountId,
  auto_unlock: AutoUnlockMode,
  lease: Lease<BlockNumberFor<T>>,
) -> DispatchResult {
  let amount = T::Currency::minimum_balance(); // ED-like minimum.
  // dispatch to sponsor(...)
}
```

#### Unlock sponsorship manually

```rust
/// Sponsor unlocks and reclaims the sponsored funds from beneficiary.
#[pallet::call]
#[pallet::weight(T::WeightInfo::unlock())]
#[transactional]
pub fn unlock(
  origin: OriginFor<T>,
  beneficiary: T::AccountId,
) -> DispatchResult;
```

Normative behavior:

- Origin must be signed; `caller = ensure_signed(origin)`.
- Must fetch record `rec = Sponsorships[beneficiary]` or return `Error::NotSponsored`.
- Must ensure `caller == rec.sponsor` else `Error::NotSponsor`.
- Must transfer held funds back to the sponsor as **free**:
  - Use `Currency::transfer_on_hold(reason, &beneficiary, &rec.sponsor, rec.amount, Precision::Exact, Restriction::Free, Fortitude::Polite)`.
  - `Restriction` supports `Free` or `OnHold`. Use `Free` to ensure the result is not held in the sponsor.
- Must remove `Sponsorships[beneficiary]`, update sponsor ledger totals, and clean up any auxiliary indexing (expiry map entry, auto-unlock queue membership best-effort).
- If a storage deposit was held, release it back to sponsor.
- Emit `Event::Unlocked { sponsor, beneficiary, amount, reason: Manual }`.

#### Sponsor updates policy

```rust
#[pallet::call]
#[pallet::weight(T::WeightInfo::set_policy())]
pub fn set_policy(
  origin: OriginFor<T>,
  beneficiary: T::AccountId,
  auto_unlock: AutoUnlockMode,
  lease: Lease<BlockNumberFor<T>>,
) -> DispatchResult;
```

Normative behavior:

- Only sponsor may update.
- If lease is changed, must update `Expirations` index accordingly (remove from old bucket, insert to new bucket).
- If auto-unlock toggled ON, ensure beneficiary is queued for checking.
- Emit `Event::PolicyUpdated`.

#### Force operations

```rust
/// Force revoke sponsorship and return funds to sponsor (privileged).
#[pallet::call]
#[pallet::weight(T::WeightInfo::force_unlock())]
#[transactional]
pub fn force_unlock(
  origin: OriginFor<T>,
  beneficiary: T::AccountId,
) -> DispatchResult;
```

- Origin must satisfy `T::ForceOrigin`. This is the pallet’s last-resort escape hatch for stuck funds (e.g., sponsor key lost) while preserving the invariant that beneficiaries cannot unlock. (This is a governance/root policy decision.)

### Runtime-facing functions for other pallets

The pallet must expose an internal trait for other pallets to use without coupling to storage layout. This aligns with the Balances pallet guidance that, when traits provide needed functionality, avoid tight coupling.

```rust
pub trait SponsorshipProvider<AccountId, Balance, BlockNumber> {
  fn sponsorship_of(beneficiary: &AccountId) -> Option<SponsorshipView<AccountId, Balance, BlockNumber>>;

  fn is_sponsored(beneficiary: &AccountId) -> bool;

  fn sponsored_amount(beneficiary: &AccountId) -> Balance;

  fn sponsor_of(beneficiary: &AccountId) -> Option<AccountId>;

  /// Create sponsorship with pallet-controlled permissioning.
  /// The caller pallet is responsible for origin/authorization; this function enforces caps and invariants.
  fn create_sponsorship(
    sponsor: &AccountId,
    beneficiary: &AccountId,
    amount: Balance,
    auto_unlock: AutoUnlockMode,
    lease: Lease<BlockNumber>,
  ) -> DispatchResult;

  /// Manually unlock (requires sponsor match or ForceOrigin at the call site).
  fn unlock_sponsorship(
    sponsor: &AccountId,
    beneficiary: &AccountId,
  ) -> DispatchResult;

  /// Evaluate and execute auto-unlock for a beneficiary if policy enabled and condition met.
  fn try_auto_unlock(beneficiary: &AccountId) -> DispatchResult;
}
```

Permission rules:

- `create_sponsorship` must be callable by other pallets for abstractions like faucets or onboarding modules, but must apply the same caps and sponsor-min rules as the signed extrinsic path.
- `unlock_sponsorship` must require the provided `sponsor` matches the stored sponsor (or be invoked only by privileged contexts using `force_unlock`).

### Optional runtime API for RPC clients

If the chain wants a read-optimized query surface beyond storage reads, define a runtime API (in a `runtime_api` module/crate similar to pallets that ship a runtime API module).

Example (Rust-like, `sp_api::decl_runtime_apis!` style implied):

```rust
pub struct SponsorshipApiView<AccountId, Balance, BlockNumber> {
  pub sponsor: AccountId,
  pub beneficiary: AccountId,
  pub amount: Balance,
  pub auto_unlock: AutoUnlockMode,
  pub lease: Lease<BlockNumber>,
  pub created_at: BlockNumber,
}

pub trait SponsorshipRuntimeApi<AccountId, Balance, BlockNumber> {
  fn sponsorship_of(beneficiary: AccountId) -> Option<SponsorshipApiView<AccountId, Balance, BlockNumber>>;
  fn sponsor_ledger(sponsor: AccountId) -> SponsorState<Balance>;
}
```

Runtime API calls are supported by common tooling (e.g., `api.call.*` surfaces) and are explicitly documented in the Polkadot Developer Docs “Call Runtime APIs” guides.

### Events

Events must include enough information for indexers and UIs:

```rust
pub enum Event<T: Config> {
  Sponsored {
    sponsor: T::AccountId,
    beneficiary: T::AccountId,
    amount: BalanceOf<T>,
    auto_unlock: AutoUnlockMode,
    lease: Lease<BlockNumberFor<T>>,
  },
  Unlocked {
    sponsor: T::AccountId,
    beneficiary: T::AccountId,
    amount: BalanceOf<T>,
    reason: UnlockReason,
  },
  PolicyUpdated {
    sponsor: T::AccountId,
    beneficiary: T::AccountId,
    auto_unlock: AutoUnlockMode,
    lease: Lease<BlockNumberFor<T>>,
  },
  Expired {
    sponsor: T::AccountId,
    beneficiary: T::AccountId,
    amount: BalanceOf<T>,
  },
  AutoUnlocked {
    sponsor: T::AccountId,
    beneficiary: T::AccountId,
    amount: BalanceOf<T>,
    condition: AutoUnlockCondition,
  },
}
```

### Errors

```rust
pub enum Error<T> {
  AlreadySponsored,
  NotSponsored,
  NotSponsor,
  AmountTooLow,
  LeaseTooLong,
  LeaseInPast,
  SponsorCapExceeded,
  SponsorCountCapExceeded,
  SponsorPerBlockCapExceeded,
  SponsorPerBlockCountExceeded,
  SponsorMinRemainingViolation,
  HoldUnavailable,          // e.g., too many concurrent holds or no provider reference.
  CannotTransferAndHold,
  CannotTransferOnHold,
  StorageDepositFailed,
}
```

### Hooks

#### Auto-unlock processing

Because fully reactive “on balance changed” hooks are not part of the standard fungible interfaces exposed to pallets, auto-unlock must be implemented either as:

- **best-effort periodic processing** via `Hooks::on_initialize` bounded by `MaxQueueProcessingPerBlock`, or
- a **user/pallet-triggered** `refresh(beneficiary)` extrinsic that checks a single beneficiary on-demand.

This spec includes both:

```rust
fn on_initialize(n: BlockNumberFor<T>) -> Weight {
  // Process up to MaxQueueProcessingPerBlock from AutoUnlockQueue:
  // - if beneficiary still sponsored and auto_unlock enabled:
  //   - check condition
  //   - if met, execute unlock to sponsor
}
```

The condition check should be explicitly chosen (see configuration tables below). Guidance:

- If using spendable checks, base them on spendable/reducible balance concepts because spendable balance is the portion used for transfers and fees.
- If using free-balance checks, ensure the definition excludes the held sponsorship amount to avoid immediate unlock.

#### Lease expiry processing

Use `Expirations[block]` mapping to process expiries at block `n`:

- For each beneficiary in `Expirations[n]`, if still sponsored and lease matches `n`, call internal revoke/unlock path.
- Bound processing by `MaxQueueProcessingPerBlock`.

### Genesis configuration

Genesis config should be supported to allow pre-funded sponsorships at chain start, noting it requires sponsor balances exist in genesis allocations.

```rust
#[pallet::genesis_config]
pub struct GenesisConfig<T: Config> {
  pub initial_sponsorships: Vec<(T::AccountId, T::AccountId, BalanceOf<T>, AutoUnlockMode, Lease<BlockNumberFor<T>>)>,
}
```

Genesis build must:

- Apply the same invariant checks as the runtime path (caps, minimums, lease validity),
- Use hold transfer semantics so beneficiaries begin with held balances,
- Prefer transactional-like sequencing (in genesis, this may be implemented as careful “check-then-apply” loops).

### Configuration comparison tables

#### Sponsor cap strategies

| Strategy | What it limits | Storage needed | Pros | Cons |
|---|---|---|---|---|
| `MaxTotalSponsoredPerSponsor` | Total active sponsored amount per sponsor | `SponsorLedger.active_amount` | Simple, directly bounds locked capital | Does not directly bound #accounts; many tiny sponsorships possible |
| `MaxSponsoredAccountsPerSponsor` | Count of active sponsored beneficiaries | `SponsorLedger.active_count` | Bounds storage growth per sponsor | Does not bound locked capital amount |
| Per-block amount cap | Amount newly sponsored by sponsor in a single block | `SponsorLedger.last_block`, `sponsored_amount_in_block` | Limits bursty issuance by automated faucet pallets | Requires per-block tracking; does not bound long-term total |
| Per-block count cap | Number of new sponsorships per sponsor per block | `new_sponsorships_in_block` | Strong DoS mitigation for burst creation | Same as above |
| Lease duration cap | How long a sponsorship may last | `lease`, `Expirations` | Prevents permanent “capital drain” by indefinite holds | Requires expiry processing hooks/indexing |

#### Auto-unlock modes

| Mode | Trigger condition | Checking mechanism | Pros | Cons |
|---|---|---|---|---|
| Disabled | None | None | Minimal complexity | Sponsor must manually unlock |
| Spendable-based | Unlock when beneficiary spendable/reducible ≥ sponsored amount | Periodic queue or refresh extrinsic | Aligns with fees/spendability definition | Beneficiary may accumulate locked funds and still not auto-unlock |
| Free-based | Unlock when beneficiary free (excluding holds) ≥ sponsored amount | Requires consistent “free excluding hold” definition | Matches intuitive “owns enough funds” notion | Depends on token bookkeeping semantics and/or custom inspection |

Spendable balance is explicitly defined as transferable and also available for fees; it is derived from free, frozen/locked, reserved/held, and ED.

#### Fee-handling strategies in the ecosystem context

| Strategy | Who pays fees for beneficiary-origin extrinsics? | Pallet scope | Notes |
|---|---|---|---|
| Default (no fee sponsorship) | Beneficiary | Out of scope | Transaction payment secures fees before execution; beneficiary with only held funds likely cannot pay.
| Separate fee-sponsor mechanism | Another account (paymaster/sponsor) | Out of scope | Can be implemented via custom transaction-payment logic; not in this spec.
| Asset-based fees | Pay fees with other assets | Out of scope | Requires runtime support for alternative fee payment.

### Lifecycle timeline diagram

```mermaid
sequenceDiagram
  autonumber
  actor Sponsor
  actor Beneficiary
  participant Pallet as SponsorshipPallet
  participant Currency as Balances/Holds

  Sponsor->>Pallet: sponsor(beneficiary, amount, auto_unlock?, lease?)
  Pallet->>Currency: transfer_and_hold(reason, Sponsor, Beneficiary, amount)
  Currency-->>Pallet: ok(amount held)
  Pallet-->>Sponsor: Event::Sponsored
  Note over Beneficiary: Account exists with held/reserved balance\n(non-spendable; not usable for fees)

  opt Auto-unlock enabled
    Pallet->>Pallet: on_initialize / refresh(beneficiary)
    Pallet->>Currency: check reducible/spendable or free condition
    alt Condition met
      Pallet->>Currency: transfer_on_hold(reason, Beneficiary, Sponsor, amount, Restriction::Free)
      Currency-->>Pallet: ok(amount returned)
      Pallet-->>Sponsor: Event::AutoUnlocked
    else Condition not met
      Pallet-->>Pallet: Keep sponsorship active
    end
  end

  opt Manual unlock
    Sponsor->>Pallet: unlock(beneficiary)
    Pallet->>Currency: transfer_on_hold(... Restriction::Free)
    Currency-->>Pallet: ok(amount returned)
    Pallet-->>Sponsor: Event::Unlocked(reason=Manual)
  end

  opt Lease expiry
    Pallet->>Pallet: on_initialize at expiry block
    Pallet->>Currency: transfer_on_hold(... Restriction::Free)
    Currency-->>Pallet: ok(amount returned)
    Pallet-->>Sponsor: Event::Expired
  end

  opt Force revoke
    Pallet->>Pallet: force_unlock(beneficiary) by ForceOrigin
    Pallet->>Currency: transfer_on_hold(... Restriction::Free)
    Currency-->>Pallet: ok(amount returned)
    Pallet-->>Pallet: Event::Unlocked(reason=Force)
  end
```

Key calls used in the diagram (`transfer_and_hold`, `transfer_on_hold`, `Restriction::Free`, transactional requirement, and “keep alive” semantics via preservation/fortitude concepts) correspond directly to the Polkadot SDK hold and token utility definitions.

## Weight, fees, and benchmarking considerations

**Transactional correctness and weight**

- Because `transfer_and_hold` explicitly warns it may error after partial storage mutation and advises transactional usage with rollback on `Err`, the `sponsor` extrinsic (and any extrinsic that composes multiple storage writes and hold operations) must be `#[transactional]`. The transactional macro guarantees storage changes are discarded on error and committed on success for `Result`-returning functions.
- Weight functions must account for:
  - reads/writes to `Sponsorships`, `SponsorLedger`, and auxiliary indexes (expiry buckets, queues),
  - underlying currency operations which touch Balances storage (account data, holds). `pallet_balances` exposes holds and freezes as first-class storage types and implementations.

**Hook processing limits**

- `on_initialize` processing must be strictly bounded by `MaxQueueProcessingPerBlock` to avoid unbounded work and block-weight exhaustion.
- When processing expiry buckets, the bucket data structure must be bounded; if an insertion would exceed bounds, the extrinsic must fail with a deterministic error (e.g., `Error::QueueFull` if introduced).

**Fee impacts**

- Transaction fees are designed to price resource usage and are typically deducted ahead of execution; fee computation includes base, length, weight, and a multiplier.
- Since sponsorship funds are held/reserved and spendable balance is what can pay fees, beneficiaries with only sponsorship-held balance will usually have insufficient spendable balance to submit signed extrinsics under standard fee payment.

**Benchmarking**

- Provide a `WeightInfo` trait with functions for each extrinsic and hook batch:
  - `sponsor()`, `sponsor_minimum()`, `unlock()`, `set_policy()`, `force_unlock()`,
  - `on_initialize_process_auto_unlock(n)`, `on_initialize_process_expiries(n)`.
- Benchmarks should include cases where:
  - sponsor ledger is at cap thresholds,
  - beneficiary already exists vs created by sponsorship,
  - queue/index updates occur (lease + auto-unlock enabled),
  - failure paths (hold unavailable, caps exceeded) are tested for constant-time behavior.

## Threat model and mitigations

**State-bloat / storage DoS via many sponsored accounts**

Threat: a sponsor (or an automated pallet acting with sponsor funds) creates many beneficiary accounts, increasing state size.

Mitigations:

- Per-sponsor active count cap (`MaxSponsoredAccountsPerSponsor`) and total amount cap (`MaxTotalSponsoredPerSponsor`).
- Optional storage deposit held from sponsor and returned on cleanup, aligning with the ecosystem’s use of held deposits for on-chain storage usage.
- Lease expiry so sponsorships do not last forever unless explicitly configured. This avoids indefinite funds being stuck under holds.

**Burst creation / per-block DoS**

Threat: rapid creation of many sponsorships in a single block by an automated sponsor, stressing block weight and storage writes.

Mitigations:

- Per-block sponsor caps on amount and count, tracked in `SponsorLedger` and reset when `last_block` changes.
- Bounded vectors for indexing (`Expirations`, `AutoUnlockQueue`), and deterministic failure when full.

**Hold-slot exhaustion (“too many holds”)**

Threat: beneficiary already has many holds from other pallets (governance deposits, identity deposits, etc.), and Balances is configured with a maximum number of holds per account (`MaxHolds`). A sponsorship attempt could fail due to hold availability constraints.

Mitigations:

- Pre-check via the hold inspection API where available (e.g., `hold_available` semantics in hold inspection traits) and fail early with `Error::HoldUnavailable`.
- Use transactional execution so any partial state changes are reverted on error.

**Sponsor over-commitment / sponsor account risk**

Threat: sponsor attempts to sponsor too much and risks becoming dusted/insufficient.

Mitigations:

- Enforce `SponsorMinRemaining`.
- Use “keep alive” semantics via preservation modes; `Preservation::Preserve` indicates the account may not be killed and provider reference must remain.

**Beneficiary attempting to move sponsored funds**

Threat: beneficiary tries to transfer or pay fees using sponsored funds.

Mitigations:

- Funds are held/reserved, which are non-spendable and not used for transfers/fees. Spendable balance is the portion used for transfers and fees.
- No extrinsic is provided to beneficiaries to release the hold; only sponsor-checked unlock is exposed.

**Reentrancy and atomicity concerns**

Threat: partial execution could leave funds transferred but not held, making them spendable.

Mitigations:

- Use `transfer_and_hold` for single-step conceptual transfer + hold, and wrap the extrinsic in `#[transactional]` so that any `Err` rolls back all storage changes. The transactional macro explicitly specifies rollback behavior.
- Avoid calling hold mutation functions with `Fortitude::Force` except in privileged contexts; `Fortitude::Force` is intended for system-level operations such as slashing.

**Sponsorship cycles**

Threat: A sponsors B, B sponsors A.

Mitigation:

- Cycles are not inherently unsafe in this model because each sponsorship is independent and funds are held in the respective beneficiary account. Additionally, a beneficiary with only held funds generally lacks spendable balance to pay fees and thus cannot easily initiate further sponsored actions under default fee rules.
