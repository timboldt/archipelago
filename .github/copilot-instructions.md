# Archipelago Simulation — Copilot Instructions

## Build & Run

```sh
cargo build
cargo run
cargo test
cargo test <test_name>
cargo clippy --all-targets
cargo +nightly fmt
```

## Project Context

Archipelago is an economic simulation built with **Bevy 0.16** ECS. Islands produce and consume goods while autonomous ships trade between them. Market information propagates only through ship-island ledger merges at dock; there is no global broadcast.

## Architecture

- `src/components.rs` — ECS components and shared types (`Commodity`, `PriceEntry`, `PriceLedger`, inventories, ship components)
- `src/resources.rs` — ECS resources (`SimulationTick`, `TimeScale`, `PlanningTuningRes`, etc.)
- `src/island/` — `IslandEconomy` logic and island spawning/constants
- `src/ship/` — `ShipState` and route-scoring utility logic
- `src/simulation/` — phase systems (`economy`, `movement`, `friction`, `docking`, `fleet`, `route_history`)
- `src/rendering/` — camera and world-entity visuals
- `src/ui/` — HUD and inspector panels

## Key Conventions

- Commodities are fixed-size `[f32; 5]` arrays indexed by `Commodity::idx()`.
- `PriceLedger` is indexed by island id and sized at world initialization.
- Ship logic runs through reconstituted `ShipState` from ECS components.
- Ship ledger merges are the only information propagation mechanism.
- Keep `README.md` and this file aligned with behavior/controls/architecture changes.
