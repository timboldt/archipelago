//! All Bevy Component definitions and shared types for the Archipelago simulation.

use bevy::prelude::*;

use crate::island::IslandEconomy;

/// Number of fixed commodities in the simulation economy.
pub const COMMODITY_COUNT: usize = 5;
/// Base (pre-scarcity) unit value per resource.
pub const BASE_COSTS: [f32; COMMODITY_COUNT] = [20.0, 30.0, 45.0, 120.0, 180.0];
/// Nominal per-commodity storage baseline used during island initialization.
pub const INVENTORY_CARRYING_CAPACITY: f32 = 180.0;

/// Fixed-size inventory vector indexed by [`Commodity::idx`].
pub type Inventory = [f32; COMMODITY_COUNT];

use strum_macros::EnumIter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
#[repr(usize)]
/// Commodity kinds traded and consumed across islands and ships.
pub enum Commodity {
    Grain,
    Timber,
    Iron,
    Tools,
    Spices,
}

impl Commodity {
    /// Returns the fixed array index for this resource.
    pub fn idx(self) -> usize {
        self as usize
    }

    /// Returns cargo-space volume used by one unit of this resource.
    pub fn volume_per_unit(self) -> f32 {
        match self {
            Commodity::Grain => 1.0,
            Commodity::Timber => 0.85,
            Commodity::Iron => 0.75,
            Commodity::Tools => 0.2,
            Commodity::Spices => 0.2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
/// Snapshot of one island market for ship/island local ledgers.
pub struct PriceEntry {
    /// Observed local prices by resource.
    pub prices: [f32; COMMODITY_COUNT],
    /// Observed local inventories by resource.
    pub inventories: [f32; COMMODITY_COUNT],
    /// Observed local resource capacities.
    pub capacities: [f32; COMMODITY_COUNT],
    /// Observed island cash/liquidity.
    pub cash: f32,
    /// Observed island infrastructure level.
    pub infrastructure_level: f32,
    /// World tick when the source island last refreshed this entry.
    pub tick_updated: u64,
    /// World tick when this ledger owner last saw the source island directly.
    pub last_seen_tick: u64,
}

/// Fixed-size per-island market cache indexed by island id.
pub type PriceLedger = Vec<PriceEntry>;

// ── Island Components ──────────────────────────────────────────────────

/// Marker component for island entities.
#[derive(Component)]
pub struct IslandMarker;

/// Marker component for the mainland island.
#[derive(Component)]
pub struct MainlandMarker;

/// Stable index for ledger arrays.
#[derive(Component, Clone, Copy, Debug)]
pub struct IslandId(pub usize);

/// Separated because it's heap-allocated and cloned during docking.
#[derive(Component, Clone)]
pub struct MarketLedger(pub PriceLedger);

/// Position in simulation space. Also drives Transform.
#[derive(Component, Clone, Copy, Debug)]
pub struct Position(pub Vec2);

// ── Ship Components ────────────────────────────────────────────────────

/// Marker component for ship entities.
#[derive(Component)]
pub struct ShipMarker;

/// Marker: this ship is currently selected in the UI.
#[derive(Component, Default)]
pub struct SelectedShip;

/// Marker: this island is currently selected in the UI.
#[derive(Component, Default)]
pub struct SelectedIsland;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Broad operational profile for a ship.
pub enum ShipArchetype {
    Clipper,
    Freighter,
    Shorthaul,
}

#[derive(Clone, Copy, Debug)]
pub struct StrategyGenes {
    pub confidence_decay_scale: f32,
    pub risk_tolerance_scale: f32,
}

impl Default for StrategyGenes {
    fn default() -> Self {
        Self {
            confidence_decay_scale: 1.0,
            risk_tolerance_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Last dock action outcome for a ship.
pub enum DockAction {
    None,
    Sold,
    Bought,
}

/// Ship movement state.
#[derive(Component, Clone)]
pub struct ShipMovement {
    pub target: Vec2,
    pub speed: f32,
    pub base_speed: f32,
    pub target_island_id: Option<usize>,
    pub last_step_distance: f32,
}

/// Ship trading state.
#[derive(Component, Clone)]
pub struct ShipTrading {
    pub docked_at: Option<usize>,
    pub last_docked_island_id: Option<usize>,
    pub cargo: Option<(Commodity, f32)>,
    pub cash: f32,
    pub labor_debt: f32,
    pub wear_debt: f32,
    pub purchase_price: f32,
    pub planned_target_after_load: Option<usize>,
    pub cargo_changed_this_dock: bool,
    pub just_sold_resource: Option<Commodity>,
    pub last_dock_action: DockAction,
    pub dock_idle_ticks: u32,
}

/// Ship profile (archetype and genetic traits).
#[derive(Component, Clone)]
pub struct ShipProfile {
    pub archetype: ShipArchetype,
    pub efficiency_rating: f32,
    pub max_cargo_volume: f32,
    pub strategy_genes: StrategyGenes,
    pub home_island_id: Option<usize>,
}

/// Ship's market knowledge and route memory.
#[derive(Component, Clone)]
pub struct ShipLedger {
    pub ledger: PriceLedger,
    pub route_memory: Vec<f32>,
}

// ── Convenience: bundled ship queries ──────────────────────────────────

impl IslandEconomy {
    pub fn resource_label(resource: Commodity) -> &'static str {
        match resource {
            Commodity::Grain => "Grain",
            Commodity::Timber => "Timber",
            Commodity::Iron => "Iron",
            Commodity::Tools => "Tools",
            Commodity::Spices => "Spices",
        }
    }
}

/// Stores the original spawn color so it can be restored when overlay is deactivated.
#[derive(Component)]
pub struct IslandBaseColor(pub Color);

pub fn bid_multiplier(market_spread: f32) -> f32 {
    (1.0 - market_spread.clamp(0.0, 1.8) * 0.5).max(0.05)
}

pub fn ask_multiplier(market_spread: f32) -> f32 {
    1.0 + market_spread.clamp(0.0, 1.8) * 0.5
}
