# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```sh
cargo build
cargo run
cargo test
cargo test <test_name>        # run a single test
cargo clippy --all-targets    # lint
cargo +nightly fmt            # format
```

Bevy `dynamic_linking` is enabled in dev for faster iteration builds.

## Project Overview

Archipelago is an economic simulation where islands produce/consume goods and autonomous ship agents trade between them. Information propagates only via ship-island gossip (ledger merges at dock) — there is no global broadcast. Ships plan routes using stale, confidence-decayed market data.

The project is migrating from macroquad to **Bevy 0.16** ECS (current branch: `bevy`). The README still references macroquad in places but the codebase now uses Bevy throughout.

## Architecture

**Bevy plugin structure** (`src/main.rs` wires these together):

- `SimulationPlugin` — ordered system sets: TickAdvance → Economy → Movement → Friction → Docking → FleetEvolution
- `RenderingPlugin` — camera setup, island/ship visual updates (run after simulation)
- `UiPlugin` — HUD and inspector panels (run after simulation)
- `InputPlugin` — keyboard/mouse handling (runs before simulation)

**Module layout:**

- `src/components.rs` — all ECS components and shared types (`Commodity`, `PriceEntry`, `PriceLedger`, `Inventory`, ship components)
- `src/resources.rs` — all ECS resources (`SimulationTick`, `TimeScale`, `PlanningTuningRes`, `SelectionState`, etc.)
- `src/island/` — `IslandEconomy` component with production/consumption/pricing logic; `spawn.rs` for constants
- `src/ship/` — `ShipState` reassembles ship components for cross-cutting logic; `utility.rs` for route scoring; `spawn.rs` for constants
- `src/simulation/` — per-phase systems: `economy.rs`, `movement.rs`, `friction.rs`, `docking.rs`, `fleet.rs`, `route_history.rs`
- `src/rendering/` — `camera.rs`, `island_ui.rs`, `ship_ui.rs`
- `src/ui/` — `hud.rs`, `inspector.rs`

**Key design patterns:**

- Commodities are fixed-size `[f32; 5]` arrays (Grain, Timber, Iron, Tools, Spices) indexed by `Commodity::idx()`. Iterate with `Commodity::iter()` via strum.
- `PriceLedger` (`Vec<PriceEntry>`) is indexed by island id. Allocated at world-init with fixed island count.
- Ship data is decomposed into `ShipMovement`, `ShipTrading`, `ShipProfile`, and `ShipLedger` components. `ShipState` temporarily reassembles them for methods needing cross-cutting access.
- Island economy logic lives entirely in `IslandEconomy` methods, keeping it testable independent of Bevy.
- Ship ledger merges are the **only** information propagation mechanism — no global broadcast.

## Key Constants

World size, island/ship counts, and starting tick are defined in `src/island/spawn.rs` and `src/ship/spawn.rs`. Planning tuning defaults (friction, decay, spread) are set in `main.rs`.

## Runtime Controls

- `[`/`]` — cycle selected ship; `Shift+[`/`Shift+]` — cycle selected island
- `-`/`=` — decrease/increase sim speed
- WASD / arrow keys — pan camera; Q/E or scroll wheel — zoom

## Documentation Hygiene

Keep `README.md` aligned with implementation changes. If behavior, controls, architecture, or dependencies change, update README in the same work.
