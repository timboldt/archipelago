
# Archipelago

An archipelago economic simulation in Rust, built on **Bevy 0.16** ECS. Islands produce and consume goods, autonomous ship agents trade between them, and market information propagates only through local ship-island ledger merges (a gossip protocol) — there is no global broadcast.

## Current Status

A working simulation with a phased ECS update loop:

1. **TickAdvance** — advance the simulation tick and rebuild cached island positions
2. **Economy** — island production, consumption, population dynamics, and local price recompute
3. **Movement** — ships move toward their target islands
4. **Friction** — accrue maritime friction costs (labor/provisions + distance-based repair wear)
5. **Docking** — docked ships sell cargo, settle debt, reload, merge ledgers, and plan departure
6. **FleetEvolution** — scuttle bankrupt/weak ships, spawn daughter ships from wealthy parents

At startup, ships begin docked at their home island and load cargo before their first departure.

## Quick Start

```sh
cargo build
cargo run
cargo test
```

Bevy `dynamic_linking` is currently enabled in `Cargo.toml` to speed up iteration builds; if you prefer static linking for release builds, you can disable this feature or gate it behind a custom `[features]` flag.

## Development Hygiene

```sh
cargo clippy --all-targets
cargo +nightly fmt
```

## Controls

| Key | Action |
|-----|--------|
| WASD / Arrow keys | Pan camera |
| Q / E | Zoom out / in |
| Scroll wheel | Zoom toward cursor |
| Mouse drag | Pan camera |
| Click | Select nearest ship or island |
| `[` / `]` | Cycle selected ship (prev / next) |
| Shift + `[` / Shift + `]` | Cycle selected island (prev / next) |
| `-` / `=` | Decrease / increase simulation speed (0.25x steps) |
| `\` | Reset simulation speed to 1.0x |

Selection is mutually exclusive: selecting a ship deselects the island and vice versa.

## Visuals

- **Islands** are irregular polygon meshes colored by dominant production: sandy tan (Grain), dark forest green (Timber), rocky grey (Iron), terracotta (Spices), muted olive (Tools). Size scales with population capacity.
- **Ships** encode archetype by shape: triangle (Clipper), rectangle (Freighter), circle (Shorthaul). Ship color reflects the currently carried cargo resource.
- **Selection highlight** — the selected ship or island is marked with a highlight ring/border in world space.
- **HUD panels** — text-based panels overlay the viewport:
  - *Left panel:* global resource totals, ship count and archetype mix, population, cash, average infrastructure, simulation speed, effective friction, and frame performance timing.
  - *Top-right panels:* ship inspector (archetype, status, speed, cargo, cash, etc.) and island inspector (population, cash, infrastructure, inventory, local prices).

## Simulation Design

### Economy

- **5 commodities:** Grain, Timber, Iron, Tools, Spices.
- **Cargo volume:** Resources have per-unit volume (Grain is bulky; Tools and Spices are compact), so value density matters for ship loading.
- **Prices:** Island-local with a damped scarcity curve (log-shaped pressure) to avoid extreme low-inventory spikes.
- **Price incentives:** Tools have an elevated base value (120) and Spices are a luxury good with a high base value (180).
- **Bid/ask spread:** Islands quote a spread (buy from ships below local price, sell to ships above), reducing churn loops and helping islands rebuild reserves.

### Islands

- **Population engine:** Islands track population with a smooth grain-balance response curve; grain abundance supports growth while scarcity increases shrink pressure gradually.
- **Production dynamics:** Tier-1 goods (Grain, Timber, Iron) are labor-driven and scale with population plus logistic damping.
- **Differentiated consumption:** Grain is the dominant population sink, Tools are moderate/durable, and Timber/Iron passive consumption is low so industrial inputs can accumulate.
- **Tier-2 industry:** Tools are manufactured (not passively extracted) by converting Timber + Iron, scaled by island infrastructure level, creating potential industrial hubs. Fabrication also scales with available labor (population).
- **Adaptive controller:** Islands apply a capped fabrication boost when local Tools per capita falls below a target floor, preventing long-run tool collapse.
- **Comparative advantage:** Islands are initialized with partial resource scarcity (including forced-zero extraction in some resources) and a boosted focus resource, creating specialization and trade dependency.
- **Luxury specialization:** Spices are intentionally rarer at production time, creating higher-value but less ubiquitous trade opportunities.
- **Survival safety net:** If an island falls to minimum population while starving, it automatically re-prioritizes grain extraction.
- **Tools as multiplier:** Tool stock boosts raw extraction productivity up to a cap, creating demand for tools beyond pure arbitrage.
- **Island capital:** Islands carry finite cash; they can only buy from ships up to affordability, and earn cash when ships purchase local inventory.
- **Island size limits:** Each island has latent size/endowment caps for inventory, population, and infrastructure that flatten growth near limits.
- **Closed-loop cash:** Islands no longer mint/burn cash from production/upkeep; trade and dock settlements are the primary cash-flow paths.
- **Infrastructure credit loop:** Islands accrue internal infrastructure credit (separate from cash) and spend it on infrastructure growth.

### Ships

- **Archetype profiles:** Three discrete archetypes — Clipper (fast, low capacity, high labor burn), Shorthaul (standard speed/capacity, low overhead, range-limited), Freighter (slow, high capacity, standard overhead). Archetype is fixed at spawn; daughters inherit with a small chance of random mutation.
- **Pair-based load selection:** Empty ships score all (local resource, destination island) pairs and buy the resource with the best expected utility.
- **Anti-roundtrip guard:** Ships will not immediately reload the same resource they just sold in the same dock cycle.
- **Loaded-cargo routing:** When carrying cargo, ships score each destination by expected utility for the carried resource.
- **Capital carry cost:** Utility includes transit-time capital lock-up penalty and high-price risk attenuation.
- **Liquidity-aware planning:** Ship ledgers gossip destination cash; route utility caps expected revenue by known market depth.
- **Storage-aware planning:** Ship ledgers gossip inventory snapshots; utility discounts destination demand by available storage headroom.
- **Industrial routing bonus:** Ledgers gossip destination infrastructure level; ships add a utility bonus for delivering Iron/Timber to higher-infrastructure islands.
- **Recent-broke avoidance:** Ships apply a short-lived utility penalty to destinations recently observed as cash-poor, and hard-reject very recent zero-cash destinations.
- **Dock risk ramp:** Empty ships become gradually more willing to take slightly negative-utility loads the longer they wait docked, preventing deadlocks.
- **Least-worst loading:** Empty ships load the best finite lane even when utility is negative.
- **Forced post-load departure:** Once a ship loads, it keeps its planned target and departs.

### Maritime Friction

- **Transport cost:** Planning prices distance/time friction into expected utility so route choice is cost-aware.
- **Maritime friction:** Ships accrue time-based labor/provisions and distance-based repair wear as dock-payable debt.
- **Dock settlement:** After selling cargo, ships settle accrued labor/repair debt to the island before reloading.
- **Dynamic docking tax:** Ports levy a liquidity-aware tax on ship cash surplus when dock actions occur.
- **Provision scarcity ceiling:** Friction self-adjusts with fleet crowding (ships vs target ships per island), creating self-limiting competitive overhead.

### Information Flow

- **Gossip protocol:** Price ledgers merge only during ship-island docking interactions. There is no global broadcast.
- **Confidence decay:** Planning uses risk-adjusted expected-value utility with confidence decay from data staleness and transit latency.
- **Trader phenotypes:** Strategy genes include confidence-decay scaling and risk tolerance, mutated across generations.

### Fleet Evolution

- **Lifecycle selection:** Low-cash ships are retired; wealthy ships can split into daughter ships with small Gaussian strategy mutations.
- **Scuttle semantics:** Scuttled ships transfer remaining cash to their last docked island.
- **Birth throttling:** Daughter creation pays a birth fee and uses a pressure-scaled threshold tied to effective global friction and fleet saturation.
- **Birth fee routing:** Daughter birth fees are credited to the parent ship's docked island.
- **Bankruptcy failure:** A ship that arrives deeply insolvent and cannot recover via dock settlement is culled immediately.

## Architecture

**Bevy plugin structure** (wired together in `src/main.rs`):

- `SimulationPlugin` — ordered system sets: TickAdvance, Economy, Movement, Friction, Docking, FleetEvolution
- `RenderingPlugin` — camera setup, island/ship visual updates, selection highlights (runs after simulation)
- `UiPlugin` — HUD and inspector text panels (runs after simulation)
- `InputPlugin` — keyboard, mouse, and scroll input (runs before simulation)

**Module layout:**

| Path | Purpose |
|------|---------|
| `src/components.rs` | All ECS components and shared types (Commodity, PriceEntry, PriceLedger, Inventory, ship components) |
| `src/resources.rs` | All ECS resources (SimulationTick, TimeScale, PlanningTuningRes, ShipMeshes, etc.) |
| `src/island/mod.rs` | IslandEconomy component with production/consumption/pricing logic |
| `src/island/spawn.rs` | Island spawning, world constants (WORLD_SIZE, NUM_ISLANDS), arc-based position generation |
| `src/ship/mod.rs` | ShipState reassembles ship components for cross-cutting logic |
| `src/ship/utility.rs` | Route scoring and utility calculations |
| `src/ship/spawn.rs` | Ship spawning constants (NUM_SHIPS, STARTING_SIM_TICK) |
| `src/simulation/` | Per-phase systems: economy, movement, friction, docking, fleet, route_history |
| `src/rendering/` | Camera setup (camera.rs), island visuals (island_ui.rs), ship visuals (ship_ui.rs), selection highlights (selection.rs) |
| `src/ui/` | HUD text panels (hud.rs), ship/island inspector panels (inspector.rs) |
| `src/input.rs` | Keyboard, mouse, and scroll-wheel input handling |

**Key design patterns:**

- Commodities use fixed-size `[f32; 5]` arrays indexed by `Commodity::idx()`. Iterate with `Commodity::iter()` via strum.
- `PriceLedger` (`Vec<PriceEntry>`) is indexed by island id, allocated at world-init with a fixed island count.
- Ship data is decomposed into `ShipMovement`, `ShipTrading`, `ShipProfile`, and `ShipLedger` ECS components. `ShipState` temporarily reassembles them for methods needing cross-cutting access.
- Island economy logic lives entirely in `IslandEconomy` methods, keeping it testable independent of Bevy.
- Ship ledger merges are the only information propagation mechanism.

## Key Constants

- **World size:** 5000 x 5000 simulation units
- **Islands:** 50 (defined in `src/island/spawn.rs`)
- **Ships:** 100 (defined in `src/ship/spawn.rs`)
- **Starting tick:** 500 (ships begin with pre-aged ledger data)
- **Planning tuning defaults** (set in `main.rs`): global friction 1.0, info decay rate 0.003, market spread 0.10

## Tech Stack

- **Language:** Rust (edition 2021)
- **Engine:** Bevy 0.16 (ECS, rendering, input, windowing)
- **Randomization:** `rand` 0.8
- **Enum utilities:** `strum` + `strum_macros` 0.26
- **Test framework:** `rstest` 0.26 (dev dependency)
