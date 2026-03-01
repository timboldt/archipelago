
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

Each ship can perform at most one dock action per tick (`sell` or `buy`, not both).

## Quick Start

```sh
cargo build
cargo run
cargo test
```

## Simulation Notes

- **World size:** 5000×5000 simulation space rendered with a `macroquad` camera.
- **Resources:** Grain, Timber, Iron, Tools.
- **Prices:** Island-local, inventory-driven (`base_cost / (inventory + 1.0)`).
- **Information flow:** Price ledgers are merged only during ship-island docking interactions.
- **Planning:** Current route planning is a simple utility heuristic; confidence-decay and richer economics are planned next.

## Tech Stack (Current)

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Randomization:** `rand`
- **Enum utilities:** `strum` + `strum_macros`

## Near-Term Roadmap

- Add confidence decay using data staleness and transit time.
- Improve trade sizing and utility scoring.
- Introduce parallel updates (`rayon`) for island and ship phases.
