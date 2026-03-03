
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
	- merge ship gossip into island ledgers in a parallel buffered phase
	- plan departure target from a stable merged island-ledger snapshot
	- process island batches in parallel and reinsert ships by stable global slot ID

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
- **Island spawn spacing:** Islands spawn with a minimum separation target to reduce chart/icon overlap in dense regions.
- **Island visuals:** Islands are drawn as compact 5-bar charts for Grain, Timber, Iron, Tools, and Spices abundance.
- **Island module boundaries:** `Island` now contains economy/market logic only; world-space island rendering lives under `src/world/` UI helpers.
- **Ship module boundaries:** `Ship` now contains movement/planning/settlement logic only; world-space ship rendering lives under `src/world/` UI helpers.
- **Ship visuals:** Ship shape encodes archetype (Freighter = square, Runner = triangle, Coaster = circle), and ship color reflects whichever cargo resource is largest by onboard value.
- **Island status bars:** Each island chart now includes three horizontal bars beneath it for Population, Cash, and Infrastructure.
- **Chart readability:** Island chart dimensions are scaled from current view units-per-pixel so bars stay legible across zoom/viewport changes.
- **UI legend:** A fixed top-left legend maps resource colors (and empty ships) for quick visual decoding.
- **Ship shape key:** The same panel includes a compact ship-shape legend (Runner triangle, Freighter square, Coaster circle).
- **Legend counters:** The legend now shows total archipelago inventory beside each resource label.
- **Macro counters:** The same panel also shows global Population, global Cash, and average Industry (infrastructure level).
- **Tuning HUD:** The left panel shows effective global friction for ship economics.
- **Fleet HUD:** The panel shows current ship count plus archetype mix (`R/F/C` = Runner/Freighter/Coaster).
- **Ship inspector HUD:** A top-right panel shows one selected ship's details (archetype, status, speed, cargo volume usage, rigging/labor rates, cash, and dominant cargo by value).
- **Selection highlight:** The currently selected ship is marked in world space with a red ring.
- **Island inspector HUD:** A second top-right panel shows one selected island's details (population, cash, infrastructure, inventory mix, and local prices).
- **Island highlight:** The currently selected island is highlighted in world space with a bold red border.
- **Resources:** Grain, Timber, Iron, Tools, Spices.
- **Cargo volume:** Resources have per-unit volume; Grain is bulky while Tools/Spices are compact, so value density matters for ship loading.
- **Prices:** Island-local with a damped scarcity curve (log-shaped pressure) to avoid extreme low-inventory spikes.
- **Price incentives:** Tools base value is elevated (120), and Spices are modeled as a luxury good with a high base value (180).
- **Population engine:** Islands now track `population` with a smooth (non-binary) grain-balance response curve; grain abundance supports growth while scarcity increases shrink pressure gradually.
- **Production dynamics:** Tier-1 goods (Grain, Timber, Iron) are labor-driven and scale with population (plus logistic damping), so larger islands produce and consume more.
- **Differentiated consumption:** Grain is the dominant population sink, Tools are moderate/durable, and Timber/Iron passive consumption is low so industrial inputs can accumulate.
- **Tool durability:** Tool demand is explicitly down-scaled relative to other goods to avoid consuming tools faster than the manufacturing system can replenish them.
- **Tier-2 industry:** Tools are manufactured (not passively extracted) by converting Timber + Iron, scaled by island `infrastructure_level`, creating potential industrial hubs.
- **Industrial scaling:** Tool fabrication now scales with both infrastructure and available labor (population), so growing islands can expand manufacturing throughput.
- **Adaptive controller:** Islands apply a capped fabrication boost when local `Tools / 1k pop` falls below a target floor, helping prevent long-run tool collapse.
- **Industrial throughput:** Tool fabrication now runs with a moderated base rate (`0.45`) and moderated output per batch (`2.2`) to curb long-run tools overshoot while preserving replenishment.
- **Supply-chain rebalance:** Timber extraction is now biased higher than iron extraction, and tool fabrication consumes more iron per batch while producing more tools, which helps drain iron gluts and raise tool availability.
- **Comparative advantage:** Islands are now initialized with partial resource scarcity (including forced-zero extraction in some resources) and a boosted focus resource, creating stronger specialization and trade dependency.
- **Luxury specialization:** Spices are intentionally rarer at production time than staple resources, creating higher-value but less ubiquitous trade opportunities.
- **Specialization tuning:** Timber/Iron zero-production probability is reduced to `0.20` to preserve baseline raw-material flow while still allowing specialization.
- **Survival safety net:** If an island falls to minimum population while starving, it automatically re-prioritizes grain extraction to restart its local economy.
- **Tools as multiplier:** Tool stock boosts raw extraction productivity up to a cap, creating industrial demand for tools beyond pure arbitrage.
- **Island capital:** Islands now carry finite `cash`; they can only buy from ships up to affordability, and earn cash when ships purchase local inventory.
- **Island size limits:** Each island now has latent size/endowment caps for per-resource inventory, population, and infrastructure, which flatten growth near limits.
- **Closed-loop island cash:** Islands no longer mint/burn cash from endogenous production/upkeep terms; trade and dock settlements are the primary cash-flow paths.
- **Infrastructure credit loop:** Islands accrue internal infrastructure credit (separate from cash) and spend that credit on infrastructure growth.
- **Transport cost:** Planning continues to price distance/time friction into expected utility so route choice remains cost-aware.
- **Maritime friction:** Ships accrue (1) time-based labor/provisions and (2) distance-based repair wear as dock-payable debt.
- **Dock settlement:** After selling/bartering cargo, ships settle accrued labor/repair debt to the island before loading the next leg.
- **Dynamic docking tax:** Ports levy a liquidity-aware tax on ship cash surplus (above a reserve) when dock actions occur; rates rise when ships are cash-rich relative to the island.
- **Dock-only bankruptcy:** Bankruptcy culling is resolved at dock; a bankrupt ship transfers its remaining cash to the docked island before removal.
- **Provision scarcity ceiling:** Friction self-adjusts with fleet crowding (ships vs target ships per island), creating a self-limiting competitive overhead as population grows.
- **Pair-based load selection:** Empty ships score full `(local resource -> destination island)` pairs and buy the resource from the best pair, rather than picking the cheapest local good first.
- **Anti-roundtrip guard:** A ship will not immediately reload the same resource it just sold in the same dock cycle.
- **Information flow:** Price ledgers are merged only during ship-island docking interactions, with a dedicated parallel per-island buffered merge and stable snapshot reads so island world-view does not shift mid-tick due to ship-processing order.
- **Stable ship IDs:** Fleet storage now uses stable slot IDs (`Vec<Option<Ship>>`); per-island dock processing temporarily extracts docked ships, processes in parallel, then reinserts each ship into its original slot.
- **Planning:** Route selection uses deterministic, risk-adjusted expected-value utility over volume-constrained lot sizes (`expected profit - distance/time/carry costs - staleness risk`) with confidence decay from data staleness + transit latency.
- **Loaded-cargo routing:** When carrying mixed cargo, ships now score each destination by summing utility across all carried resources (portfolio optimization) rather than following only the single best cargo lane.
- **Capital carry cost:** Utility now includes a transit-time capital lock-up penalty and high-price risk attenuation, reducing over-selection of expensive cargo when long-haul uncertainty is high.
- **Liquidity-aware planning:** Ship ledgers now gossip destination `cash`, and route utility caps expected revenue by known market depth so traders avoid chasing phantom high prices at bankrupt islands.
- **Storage-aware planning:** Ship ledgers also gossip inventory snapshots; utility discounts destination demand by available storage headroom so traders avoid over-delivering into saturated markets.
- **Industrial routing bonus:** Ledgers also gossip destination infrastructure level; ships add a proportional utility bonus for delivering Iron/Timber to higher-infrastructure islands (above a threshold).
- **Recent-broke avoidance:** Ships apply a short-lived utility penalty to destinations recently observed as cash-poor, reducing repeated revisits to liquidity-starved islands after partial unloads.
- **Broke-route suppression:** Ships now hard-reject very recent zero-cash destinations during utility evaluation, preventing persistent back-and-forth loops between bankrupt islands.
- **Bid/ask spread:** Islands quote a spread (buy from ships at `0.95×` local, sell to ships at `1.05×` local), reducing churn loops and helping islands rebuild reserves.
- **Barter swap-and-go:** When cash settlement is constrained, carrying ships can perform value-equivalent cargo swaps at dock (barter), allowing goods to keep flowing even during local liquidity crunches.
- **Partial unloads:** Ships already sell whatever quantity an island can currently afford; if a sale is only partial and cargo remains, ships are now allowed to redepart in the same tick instead of waiting docked.
- **Empty-cargo relocation:** If a ship cannot load, it still picks its next island by maximizing the same expected-value utility over candidate resource opportunities (using its local ledger prices as reference buy prices).
- **Anti-herding:** Planning applies a pheromone-style route signal over the last 10 ticks: if many ships recently left `A -> B`, confidence in `B`'s quoted prices is attenuated by approximately `1/N` for ships departing from `A`.
- **Ship learning:** Each ship maintains a decaying destination memory updated by realized trade margins, and this memory biases future route utility.
- **Ship trade-off triangle:** Each ship now carries coupled hull traits (size + efficiency) that jointly determine speed, cargo volume capacity, rigging/repair wear rate, and ongoing labor/provisions burn; hull size strongly anchors speed class so runners are visibly faster while freighters are slower.
- **Archetype profiles:** Hull bands map to explicit profiles with clear separation: Runner (`speed≈1.5x`, `capacity≈0.75x`, `labor burn≈1.5x`), Coaster (`1.0x`, `1.0x`, `0.75x`), Freighter (`0.75x`, `2.0x`, `1.0x`) before efficiency modulation.
- **Operational niches:** Mutation and selection can produce fast runners (high speed/low capacity), bulk haulers (high capacity/lower speed), and efficient coasters (lower rigging/labor drag).
- **Wealth tax / upkeep:** Ships accrue trait-derived labor/provisions and wear liabilities in transit/dock phases, and also face liquidity-aware docking taxes, so persistently unprofitable traders eventually fail lifecycle thresholds.
- **Bankruptcy failure:** If a ship arrives deeply insolvent and cannot recover via dock settlement (sell/barter phase), it is culled immediately (using a negative-cash floor rather than zero).
- **Lifecycle selection:** Fleet composition evolves over time: low-cash ships are retired, while wealthy ships can split into daughter ships with small Gaussian strategy mutations (not restricted to docked-only parents).
- **Scuttle semantics:** Scuttled ships are marked as empty slots (`None`) instead of compacting the ship array, and their remaining cash is applied to their last docked island.
- **Birth throttling:** Daughter creation now pays a birth fee and uses a pressure-scaled threshold tied to effective global friction and fleet saturation (ships per island), curbing runaway fleet growth.
- **Birth fee routing:** Daughter birth fees are credited to the parent ship's docked island instead of being removed from the economy.
- **Trader phenotypes:** Mutated strategy genes include confidence-decay scaling and risk tolerance.
- **Derived ship costs:** Trade planning and settlement derive `cost_per_distance` and `cost_per_time` from ship traits (speed/distance-cost rate and maintenance/hull profile), rather than independent tuning dials.
- **Dock cadence:** Ships that sell on a tick stay docked for at least that tick (no immediate departure while empty), then can reload and depart on a following tick.
- **Dock risk ramp:** Empty ships become gradually more willing to take slightly negative-utility loads the longer they wait docked, helping break deadlock states.
- **Least-worst loading:** Empty ships now load the best finite lane even when utility is negative, preventing dock deadlocks where all options are unattractive.
- **Forced post-load departure:** Once a ship loads, it keeps its planned outbound target and departs instead of waiting docked for a positive re-evaluation.
- **Dock performance path:** Dock settlement iterations are capped lower, loaded ships use a preselected post-load destination fast-path when viable, and loaded ships skip full destination rescans on ticks where dock actions did not change cargo.
- **Tuning controls:** `main.rs` exposes environmental constants (`global_friction_mult`, `info_decay_rate`, `market_spread`) and applies them via `World::set_planning_tuning(...)`.
- **Ship selection controls:** Press `[` and `]` during runtime to cycle the selected ship in the top-right inspector panel.
- **Island selection controls:** Press `{` and `}` (Shift + `[` / Shift + `]`) to cycle the selected island in the island inspector panel.
- **Time-scale controls:** Press `-` / `=` to decrease/increase simulation speed, and `\` to reset to `1.0x`.
- **Debug snapshot control:** Press `F9` to write a runtime snapshot (`debug_snapshot_tick_<tick>.txt`) with selected ship/island details, docked-empty ship buy-capacity diagnostics, and per-ship load-utility scan stats (`scanned/feasible/best utility`).

## Tech Stack (Current)

- **Language:** Rust (edition 2021)
- **Visualization/Input:** `macroquad`
- **Randomization:** `rand`
- **Parallelism:** `rayon` (island economy update phase)
- **Enum utilities:** `strum` + `strum_macros`

## Near-Term Roadmap

- Improve trade sizing and utility scoring.
- Extend parallel updates to additional phases where data dependencies allow.
