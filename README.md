
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
- **Resources:** Grain, Timber, Iron, Tools.
- **Prices:** Island-local, inventory-driven (`base_cost / (inventory + 1.0)`).
- **Information flow:** Price ledgers are merged only during ship-island docking interactions.
- **Planning:** Route selection combines utility with confidence decay based on data staleness + transit time, and includes probabilistic speculation to break deterministic route loops.
- **Dock cadence:** Ships that sell on a tick stay docked for at least that tick (no immediate departure while empty), then can reload and depart on a following tick.

## Tech Stack (Current)

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Randomization:** `rand`
- **Enum utilities:** `strum` + `strum_macros`

## Near-Term Roadmap

- Add confidence decay using data staleness and transit time.
- Improve trade sizing and utility scoring.
- Introduce parallel updates (`rayon`) for island and ship phases.
