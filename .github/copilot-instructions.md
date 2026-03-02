# Archipelago Simulation — Copilot Instructions

## Project Overview

A high-concurrency economic simulation of an archipelago written in Rust. Islands are independent economic nodes; ships are autonomous agents that trade goods and propagate information via a gossip protocol. The core mechanic is that **information and goods both have latency** based on travel distance — ships plan routes using "stale" data modulated by a confidence decay function.

The `OLD/` directory contains an early prototype (different asset model) and should not be used as a reference for the new implementation. `README.md` at the repo root should stay aligned with the current architecture and status.

## Planned Tech Stack

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Parallelism:** `rayon` (island economy + dock processing use `par_iter`)
- **Randomization:** `rand`
- **Math:** macroquad `Vec2`
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
enum Resource { Grain, Timber, Iron, Tools, Spices }
type Inventory = [f32; 5];

struct PriceEntry {
    prices: [f32; 5],
    inventories: [f32; 5],
    cash: f32,
    infrastructure_level: f32,
    tick_updated: u64,
    last_seen_tick: u64,
}
type PriceLedger = Vec<PriceEntry>; // indexed by island id

struct PlanningTuning {
    global_friction_mult: f32,
    info_decay_rate: f32,
    market_spread: f32,
}

struct Island {
    id: usize,
    pos: Vec2,
    inventory: Inventory,
    production_rates: Inventory,
    consumption_rates: Inventory,
    population: f32,
    cash: f32,
    infrastructure_level: f32,
    local_prices: [f32; 5],
    ledger: PriceLedger,  // island's cached view of the whole economy
}

struct Ship {
    // selected fields only
    pos: Vec2,
    cargo: Inventory,
    ledger: PriceLedger,  // ship's own knowledge of the world
    target_island_id: Option<usize>,
    speed: f32,
    cash: f32,
}
```

### Simulation Loop (on `World`)

1. **Island Production/Consumption** (via `rayon::par_iter`):  
   Updates production/consumption, population, cash, infrastructure, and recomputes local prices.

2. **Ship Movement + Dock Processing**:  
   - Ships move continuously each frame.
   - Docked ships settle trade (sell / load / barter), sync ledgers via snapshot merge, then plan next destination using deterministic expected utility.
   - Utility accounts for confidence-decayed information, spread-aware prices, market depth, storage headroom, distance/time costs, and staleness risk.

3. **Maritime Friction + Lifecycle**:  
   - Friction is auto-scaled by crowding (`active ships / target ships`), multiplied by `global_friction_mult`.
   - Periodic fleet evolution culls weak ships and spawns daughters from wealthy ships.

### Visual Layer (macroquad)

- Simulation space is ~5000×5000 mapped to screen via `Camera2D`.
- `draw_islands()` — island bar glyphs with macro counters.
- `draw_ships()` — archetype-shape markers with cargo coloring.
- `draw_ui()` — left HUD + selected ship/island inspector panels.
- The visual loop is driven by `next_frame().await`; simulation runs synchronously within each frame.

## Key Conventions

- `PriceLedger` is indexed by island `id` (`Vec<PriceEntry>` of length = number of islands). Always allocate ledgers at world-init time with a fixed island count.
- `PlanningTuning` is intentionally small and environmental: `global_friction_mult`, `info_decay_rate`, `market_spread`.
- Resources are fixed-size arrays (`[f32; 5]`) rather than maps for cache efficiency; iterate over them using `Resource::iter()` via `strum`.
- Ship ledger merges are the **only** mechanism for information propagation — there is no global broadcast.
- Ships spawn with noisy/stale initial beliefs plus accurate home-port knowledge; do not reintroduce perfect global initialization.
- Runtime controls currently: `[` / `]` for ship selection, `Shift+[` / `Shift+]` for island selection.

## Documentation Hygiene

- Keep `README.md` up to date with implementation changes. If behavior, controls, architecture, setup, dependencies, or status changes, update `README.md` in the same work.

## Development Hygiene

- Run `cargo clippy --all-targets`.
- Run `cargo +nightly fmt`.
