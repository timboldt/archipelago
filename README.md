
# Archipelago

An archipelago economic simulation in Rust. Islands produce and consume goods, ships move between islands, trade cargo, and share market information through local ledger syncs.

## Current Status

The project currently implements a working simulation scaffold with a phased world update loop:

1. Island production/consumption update and local price recompute
2. Ship movement toward targets
3. Docked ship processing (island-batched):
	- unload cargo (sell)
	- reprice island market
	- load cargo (buy)
	- sync ship/island ledgers
	- plan departure target

At startup, ships begin docked and load cargo before their first departure.

Each ship can perform at most one dock action per tick (`sell` or `buy`, not both).

## Quick Start

```sh
cargo build
cargo run
cargo test
```

## Development Hygiene

```sh
cargo clippy --all-targets
cargo +nightly fmt
```

## Simulation Notes

- **World size:** 5000×5000 simulation space rendered with a `macroquad` camera.
- **Island visuals:** Islands are drawn as compact 4-bar charts for Grain, Timber, Iron, and Tools abundance.
- **Chart readability:** Island chart dimensions are scaled from current view units-per-pixel so bars stay legible across zoom/viewport changes.
- **UI legend:** A fixed top-left legend maps resource colors (and empty ships) for quick visual decoding.
- **Tuning HUD:** The same panel shows the live `speculation_floor` value.
- **Resources:** Grain, Timber, Iron, Tools.
- **Prices:** Island-local, inventory-driven (`base_cost / (inventory + 1.0)`).
- **Production dynamics:** Island production is damped by a logistic factor as inventory approaches a carrying capacity, reducing runaway growth and oscillation.
- **Transport cost:** Cargo accrues freight cost while traveling; planning accounts for projected freight and realized P&L applies a capped freight deduction.
- **Load selection:** Empty ships choose cargo by expected net margin (confidence-weighted destination spread minus transport cost), not just lowest local price.
- **Information flow:** Price ledgers are merged only during ship-island docking interactions.
- **Planning:** Route selection combines utility with confidence decay based on data staleness + transit time, and includes probabilistic speculation to break deterministic route loops.
- **Speculation behavior:** Speculation probability now increases further when the currently best destination is crowded, and speculative picks sample among top candidates to improve route diversity.
- **Outlier rescue:** Each actor gossips a `last_seen_tick` estimate per island through ledgers; stale/rarely seen islands receive a capped neglect bonus during planning.
- **Anti-herding:** Planning applies congestion using a decaying memory of recent departures from the current island to each destination (`from -> to`), reducing local gold-rush pileups without global coordination.
- **Ship learning:** Each ship maintains a decaying destination memory updated by realized trade margins, and this memory biases future route utility.
- **Dock cadence:** Ships that sell on a tick stay docked for at least that tick (no immediate departure while empty), then can reload and depart on a following tick.
- **Tuning controls:** `main.rs` exposes planning/speculation/learning constants (`confidence_decay_k`, `speculation_floor`, `speculation_staleness_scale`, `speculation_uncertainty_bonus`, `learning_rate`, `learning_decay`, `learning_weight`, `congestion_penalty`, `congestion_exponent`, `route_congestion_decay`, `transport_cost_per_distance`, `island_neglect_bonus_per_tick`, `island_neglect_bonus_cap`) and applies them via `World::set_planning_tuning(...)`.
- **Live tuning:** Press `[` and `]` during runtime to decrease/increase `speculation_floor`.

## Tech Stack (Current)

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Randomization:** `rand`
- **Parallelism:** `rayon` (island economy update phase)
- **Enum utilities:** `strum` + `strum_macros`

## Near-Term Roadmap

- Improve trade sizing and utility scoring.
- Extend parallel updates to additional phases where data dependencies allow.
