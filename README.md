
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
- **Island status bars:** Each island chart now includes three horizontal bars beneath it for Population, Cash, and Infrastructure.
- **Chart readability:** Island chart dimensions are scaled from current view units-per-pixel so bars stay legible across zoom/viewport changes.
- **UI legend:** A fixed top-left legend maps resource colors (and empty ships) for quick visual decoding.
- **Tuning HUD:** The same panel shows the live `speculation_floor` value.
- **Fleet HUD:** The panel also shows the current ship count.
- **Resources:** Grain, Timber, Iron, Tools.
- **Prices:** Island-local with a damped scarcity curve (log-shaped pressure) to avoid extreme low-inventory spikes.
- **Population engine:** Islands now track `population` with a smooth (non-binary) grain-balance response curve; grain abundance supports growth while scarcity increases shrink pressure gradually.
- **Production dynamics:** Tier-1 goods (Grain, Timber, Iron) are labor-driven and scale with population (plus logistic damping), so larger islands produce and consume more.
- **Tier-2 industry:** Tools are manufactured (not passively extracted) by converting Timber + Iron, scaled by island `infrastructure_level`, creating potential industrial hubs.
- **Comparative advantage:** Islands are now initialized with partial resource scarcity (including forced-zero extraction in some resources) and a boosted focus resource, creating stronger specialization and trade dependency.
- **Survival safety net:** If an island falls to minimum population while starving, it automatically re-prioritizes grain extraction to restart its local economy.
- **Tools as multiplier:** Tool stock boosts raw extraction productivity up to a cap, creating industrial demand for tools beyond pure arbitrage.
- **Island capital:** Islands now carry finite `cash`; they can only buy from ships up to affordability, and earn cash when ships purchase local inventory.
- **Liquidity stabilization:** Islands also generate modest endogenous cash from population activity and industrial throughput to avoid system-wide insolvency cascades.
- **Capital sink:** Cash-rich islands automatically reinvest part of excess capital into infrastructure growth, reducing runaway cash hoarding.
- **Transport cost:** Cargo accrues freight cost while traveling; planning accounts for projected freight and realized P&L applies a capped freight deduction.
- **Pair-based load selection:** Empty ships score full `(local resource -> destination island)` pairs and buy the resource from the best pair, rather than picking the cheapest local good first.
- **Anti-roundtrip guard:** A ship will not immediately reload the same resource it just sold in the same dock cycle.
- **Information flow:** Price ledgers are merged only during ship-island docking interactions.
- **Planning:** Route selection uses an expected-value utility (`(expected unit margin × lot size × confidence) - fuel cost`) with confidence decay from data staleness + transit latency, plus probabilistic speculation for route diversity.
- **Liquidity-aware planning:** Ship ledgers now gossip destination `cash`, and route utility caps expected revenue by known market depth so traders avoid chasing phantom high prices at bankrupt islands.
- **Storage-aware planning:** Ship ledgers also gossip inventory snapshots; utility discounts destination demand by available storage headroom so traders avoid over-delivering into saturated markets.
- **Bid/ask spread:** Islands quote a spread (buy from ships at `0.95×` local, sell to ships at `1.05×` local), reducing churn loops and helping islands rebuild reserves.
- **Empty-cargo relocation:** If a ship cannot load, it still picks its next island by maximizing the same expected-value utility over candidate resource opportunities (using its local ledger prices as reference buy prices).
- **Speculation behavior:** Speculation probability now increases further when the currently best destination is crowded, and speculative picks sample among top candidates to improve route diversity.
- **Outlier rescue:** Each actor gossips a `last_seen_tick` estimate per island through ledgers; stale/rarely seen islands receive a capped neglect bonus during planning.
- **Anti-herding:** Planning applies a pheromone-style route signal over the last 10 ticks: if many ships recently left `A -> B`, confidence in `B`'s quoted prices is attenuated by approximately `1/N` for ships departing from `A`.
- **Ship learning:** Each ship maintains a decaying destination memory updated by realized trade margins, and this memory biases future route utility.
- **Wealth tax / upkeep:** Every tick, each ship pays a tiny fixed maintenance cost from cash, so persistently unprofitable traders eventually fail the scuttle threshold and are replaced by fitter descendants without collapsing the whole fleet.
- **Lifecycle selection:** Fleet composition evolves over time: low-cash ships are retired, while wealthy docked ships split into daughter ships with small Gaussian strategy mutations.
- **Trader phenotypes:** Mutated strategy genes now include risk tolerance (`confidence_decay_k` scaling: confident long-range vs cynical local traders) and a `luxury_weight` trait that biases utility toward higher-value cargo.
- **Dock cadence:** Ships that sell on a tick stay docked for at least that tick (no immediate departure while empty), then can reload and depart on a following tick.
- **Tuning controls:** `main.rs` exposes planning/speculation/learning constants (`confidence_decay_k`, `speculation_floor`, `speculation_staleness_scale`, `speculation_uncertainty_bonus`, `learning_rate`, `learning_decay`, `learning_weight`, `transport_cost_per_distance`, `island_neglect_bonus_per_tick`, `island_neglect_bonus_cap`, `luxury_weight`) and applies them via `World::set_planning_tuning(...)`.
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
