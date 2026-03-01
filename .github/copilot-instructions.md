# Archipelago Simulation — Copilot Instructions

## Project Overview

A high-concurrency economic simulation of an archipelago written in Rust. Islands are independent economic nodes; ships are autonomous agents that trade goods and propagate information via a gossip protocol. The core mechanic is that **information and goods both have latency** based on travel distance — ships plan routes using "stale" data modulated by a confidence decay function.

The `OLD/` directory contains an early prototype (different asset model) and should not be used as a reference for the new implementation. `README.md` at the repo root should stay aligned with the current architecture and status.

## Planned Tech Stack

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Parallelism:** `rayon` (ship planning and island production use `par_iter`)
- **Math:** `glam` (or macroquad's built-in `Vec2`)
- **Enums:** `strum` / `strum_macros` for iterable enums

## Build & Run

```sh
cargo build
cargo run
cargo test
# Run a single test:
cargo test <test_name>
```

## Architecture

### Key Structs

```rust
enum Resource { Grain, Timber, Iron, Tools }
type Inventory = [f32; 4];

struct PriceEntry { price: f32, tick_updated: u64 }
type PriceLedger = Vec<PriceEntry>; // one entry per island

struct Island {
    id: usize,
    pos: Vec2,
    inventory: Inventory,
    production_rates: Inventory,
    consumption_rates: Inventory,
    ledger: PriceLedger,  // island's cached view of the whole economy
}

struct Ship {
    pos: Vec2,
    cargo: Option<(Resource, f32)>,
    state: ShipState,     // Idle | Moving | Planning
    ledger: PriceLedger,  // ship's own knowledge of the world
    target_island_id: Option<usize>,
    speed: f32,
}
```

### Simulation Loop (on `World`)

1. **Island Production/Consumption** (via `rayon::par_iter`):  
   `inventory[r] += production_rates[r] * dt`  
   `inventory[r] -= consumption_rates[r] * dt`  
   Price: `price = base_cost / (inventory[r] + 1.0)`

2. **Ship Planning** (via `rayon::par_iter`, ships in `Planning` state):  
   Utility = `(potential_profit × confidence) - (distance × fuel_cost)`  
   Confidence = `exp(-k × (current_tick - data_timestamp + transit_time))`  
   Ships pick the island with maximum utility.

3. **Handshake on Arrival** (ship reaches island):  
   - **Trade:** sell cargo if island price is high; buy new cargo if local price is low.  
   - **Ledger sync:** merge `ship.ledger` and `island.ledger` by keeping the entry with the higher `tick_updated` for each slot.

### Visual Layer (macroquad)

- Simulation space is ~5000×5000 mapped to screen via `Camera2D`.
- `draw_islands()` — points color-coded by most abundant resource.
- `draw_ships()` — small particles; color indicates cargo state.
- `draw_ui()` — click an island to inspect its ledger vs. actual global state.
- The visual loop is driven by `next_frame().await`; simulation runs synchronously within each frame.

## Key Conventions

- `PriceLedger` is indexed by island `id` (`Vec<PriceEntry>` of length = number of islands). Always allocate ledgers at world-init time with a fixed island count.
- The confidence decay constant `k` controls how "local" or "speculative" ship behavior is — tuning it is the primary emergence lever (Phase 3).
- Resources are fixed-size arrays (`[f32; 4]`) rather than maps for cache efficiency; iterate over them using `Resource::iter()` via `strum`.
- Ship ledger merges are the **only** mechanism for information propagation — there is no global broadcast.

## Documentation Hygiene

- Keep `README.md` up to date with implementation changes. If behavior, controls, architecture, setup, dependencies, or status changes, update `README.md` in the same work.

## Development Hygiene

- Run `cargo clippy --all-targets`.
- Run `cargo +nightly fmt`.
