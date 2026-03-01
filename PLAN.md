# PLAN.md: Project "Archi" (Archipelago Simulation)

## 1. Project Overview
A high-concurrency economic simulation of an archipelago.
- **Islands:** ~100 independent nodes with local production/consumption and a "Memory Ledger" of the world.
- **Ships:** Independent agents that move goods and information (gossip protocol) between islands.
- **Core Mechanic:** Information and goods have latency based on travel distance. Ships plan routes based on "stale" data vs. risk.

## 2. Technical Stack
- **Language:** Rust
- **Framework:** Macroquad (Visualization/Input)
- **Parallelism:** Rayon (Ship Planning/Island Production)
- **Math:** Linear Algebra (`glam` or Macroquad's built-in `Vec2`)

---

## 3. Data Structures

### 3.1 Resource Model
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resource { Grain, Timber, Iron, Tools }
type Inventory = [f32; 4];
type PriceLedger = Vec<PriceEntry>; // Size = Number of islands

#[derive(Clone, Copy)]
struct PriceEntry {
    price: f32,
    tick_updated: u64,
}
```

### 3.2 Agents
- **Island:**
    - `id: usize`
    - `pos: Vec2`
    - `inventory: Inventory`
    - `production_rates: Inventory`
    - `consumption_rates: Inventory`
    - `ledger: PriceLedger` (Local cache of the archipelago's economy)
- **Ship:**
    - `pos: Vec2`
    - `cargo: Option<(Resource, f32)>`
    - `state: ShipState { Idle, Moving, Planning }`
    - `ledger: PriceLedger` (The ship's "knowledge" of the world)
    - `target_island_id: Option<usize>`
    - `speed: f32`

---

## 4. Phase 1: Core Simulation Engine
1.  **Island Logic:**
    - **Production:** `inventory[r] += production_rates[r] * dt`.
    - **Consumption:** `inventory[r] -= consumption_rates[r] * dt`.
    - **Price Discovery:** `Price = Base_Cost / (inventory[r] + 1.0)`. (Simple supply/demand curve).
2.  **Ship Planning (Rayon `par_iter`):**
    - For each `Ship` in `Planning` state:
        - Evaluate every `Island` in `ship.ledger`.
        - Calculate **Utility**: `(Potential_Profit * Confidence) - (Distance * Fuel_Cost)`.
        - `Confidence = exp(-k * (Current_Tick - Data_Timestamp + Transit_Time))`.
3.  **The Handshake (Interaction):**
    - When `Ship` reaches `Island`:
        - **Trade:** Ship sells cargo if island price is high; buys new cargo if local price is low.
        - **Sync:** `Ship.ledger` and `Island.ledger` merge. For each entry, keep the one with the higher `tick_updated`.

---

## 5. Phase 2: Macroquad Implementation
1.  **Coordinate Mapping:**
    - Map Simulation Space (e.g., 5000x5000) to Screen Space using `Camera2D`.
2.  **Renderers:**
    - `draw_islands()`: Points color-coded by current most abundant resource.
    - `draw_ships()`: Small particles; color indicates if they are carrying cargo.
    - `draw_ui()`: Click an island to see its local ledger vs. the "actual" global state.

---

## 6. Phase 3: Emergence Tuning
1.  **Information Decay:** Adjust the `k` constant in the confidence formula. High `k` = ships stay local. Low `k` = ships take big risks on distant rumors.
2.  **Resource Dependencies:** Make `Tools` production require both `Iron` and `Timber` to see if manufacturing hubs emerge.
3.  **Population Growth:** Tie `consumption_rates` to a population variable that grows when `Grain` is plentiful.

---

## 7. Instructions for Agentic CLI
1.  Initialize a Cargo project with `macroquad`, `rayon`, `rand`, and `glam`.
2.  Implement `Island` and `Ship` structs as defined.
3.  Implement the `update_simulation` method on a `World` struct that uses `Rayon` for planning and production.
4.  Ensure `next_frame().await` is used to drive the visual loop while the simulation runs synchronously.
