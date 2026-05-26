# Testnet Venue Plan

**Date:** 2026-05-25
**Status:** Follow-up plan, intentionally separate from the current Alpaca paper live plan.
**Scope:** Testnet-first venue expansion for Orderly, Bybit, and future venues. No mainnet unlock is included.

## Purpose

The current live work should finish Alpaca paper execution and Cline trajectory recording before expanding venues. This plan defines the later testnet venue track so Orderly and Bybit do not leak into the current Alpaca paper scope, while still giving the future implementation a clear shape.

The core model is:

- `Venue`: where the strategy trades, such as `alpaca`, `orderly`, or `bybit`
- `Environment`: money/risk environment, such as `paper`, `testnet`, or `mainnet`
- `VenueLabel`: safety label enforced at runtime, such as `Paper`, `Testnet`, or `Live`
- `BrokerSurface`: the normalized order/fill interface used by the executor
- `LiveDataFeed`: the normalized market data source used by the live loop

For this plan, only `Environment::Testnet` is in scope.

## Non-Goals

- No mainnet execution.
- No real-money unlock.
- No weakening of the existing `VenueLabel::Live` rejection.
- No Bybit or Orderly work in the current Alpaca paper live plan.
- No broker safety redesign beyond integration hooks.

## Phase T0: Venue/Environment Model

Introduce explicit venue and environment selection once Alpaca paper live is working.

Desired operator shape:

```text
xvn eval run --mode live --venue orderly --environment testnet ...
xvn eval run --mode live --venue bybit --environment testnet ...
```

Implementation shape:

```rust
pub enum Venue {
    Alpaca,
    Orderly,
    Bybit,
}

pub enum Environment {
    Paper,
    Testnet,
    Mainnet,
}

pub struct VenueEnvironment {
    pub venue: Venue,
    pub environment: Environment,
}
```

Mapping rules:

- `alpaca + paper` maps to `VenueLabel::Paper`
- `orderly + testnet` maps to `VenueLabel::Testnet`
- `bybit + testnet` maps to `VenueLabel::Testnet`
- any `mainnet` mapping remains rejected until the separate safety track opens it

Done criteria:

- CLI/API accept explicit venue and environment
- `broker_creds_ref` remains only a credential reference
- safety gate checks selected venue/environment against the broker surface label
- invalid tuples fail before network connections are opened

## Phase T1: BrokerSurface Contract Hardening

Before adding venues, make the broker boundary strict enough to compare implementations.

Required contract:

- `BrokerKind` identifies the normalized broker surface
- order submit returns broker-native ids plus normalized fill status
- rejected orders include typed reasons
- partial fills are representable
- leverage, notional, order count, and max-loss checks happen before submit
- fills are broker-reported, not simulated from bars
- every order/fill event includes `VenueLabel`

Test matrix:

- accepted market order
- rejected market order
- partial fill
- cancel or unsupported cancel
- broker timeout
- idempotency or duplicate submit protection
- safety-limit rejection before network submit

## Phase T2: Orderly Testnet

Orderly has real executor code today, but it is split from the normalized `BrokerSurface` path. The testnet track should adapt it rather than adding another one-off executor path.

Work items:

- implement `OrderlyTestnetSurface` over the existing `OrderlyExecutor`
- map supported perps explicitly: BTC, ETH, SOL, AVAX, DOGE, LINK, and any later additions through config
- add testnet credential resolution
- add Orderly testnet base URL and chain/network validation
- define how Orderly order ids, fills, fees, and leverage map into normalized fill events
- implement or select an Orderly testnet market data feed
- reject Orderly testnet live runs if no live data feed is configured

Done criteria:

- hermetic mock Orderly surface passes the shared broker contract tests
- Orderly testnet preflight validates credentials and environment
- Orderly testnet can run against a mock data feed without touching mainnet
- live executor sees only `BrokerSurface + LiveDataFeed`, not Orderly-specific types

## Phase T3: Bybit Testnet

Bybit is greenfield in this repo. Treat it as a full venue implementation, not a small adapter.

Work items:

- create `BybitTestnetSurface`
- implement credential loading and preflight
- implement normalized market order submit
- map Bybit order states into normalized fill status
- implement Bybit testnet market data feed
- define supported symbols and instrument metadata
- add rate-limit and retry behavior
- add mock and contract tests before any real testnet calls

Done criteria:

- Bybit testnet passes the shared broker contract tests
- Bybit testnet feed emits normalized bars
- Bybit testnet run can execute through the same live loop as Alpaca paper and Orderly testnet
- no Bybit code path can select mainnet

## Phase T4: Shared Testnet Verification

Add a venue matrix that runs without network by default and can run opt-in real testnet smoke tests.

Default hermetic matrix:

| Venue | Environment | Broker | Feed | Network |
| --- | --- | --- | --- | --- |
| Orderly | Testnet | mock + contract | mock bars | none |
| Bybit | Testnet | mock + contract | mock bars | none |

Opt-in smoke matrix:

| Venue | Environment | Broker | Feed | Network |
| --- | --- | --- | --- | --- |
| Orderly | Testnet | real testnet | real testnet | explicit env only |
| Bybit | Testnet | real testnet | real testnet | explicit env only |

Smoke tests must require explicit environment variables and must never run in normal CI:

```text
XVN_ENABLE_TESTNET_SMOKE=1
XVN_ORDERLY_TESTNET_CREDS=...
XVN_BYBIT_TESTNET_CREDS=...
```

## Phase T5: Operator Surfaces

After testnet backends work, expose them carefully.

CLI:

- show venue/environment in `eval run` help
- require `--environment testnet` explicitly for Orderly and Bybit
- print the selected venue, environment, and safety label before launch

Dashboard:

- disable mainnet choices
- show testnet labels prominently
- show broker preflight status
- show feed connection status
- show per-venue supported symbols

Run artifacts:

- record venue, environment, broker kind, credential ref name, feed kind, and safety label
- include broker-native order ids and normalized fill ids
- preserve trajectory replay compatibility

## Safety Boundary

This plan defines integration hooks only. It does not approve real-money execution.

Rules:

- `Environment::Mainnet` remains rejected
- `VenueLabel::Live` remains rejected
- no mainnet URL may be selected through testnet config
- all submit paths pass through safety limits before network submit
- every run records venue/environment in immutable run metadata

The separate broker safety track owns real-money unlock conditions.

## Recommended Sequencing

1. Finish Alpaca paper live L1/L2.
2. Finish Cline live recording and trajectory replay from persistent stores.
3. Add explicit venue/environment model.
4. Harden shared `BrokerSurface` contract tests.
5. Add Orderly testnet.
6. Add Bybit testnet.
7. Add opt-in real testnet smoke tests.
8. Revisit mainnet only after the safety track signs off.

