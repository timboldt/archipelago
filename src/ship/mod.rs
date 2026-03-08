//! Ship trading, planning, and movement logic.
//!
//! The simulation math lives in `ShipState` methods. In the ECS, ship data is
//! decomposed across several components, but `ShipState` reassembles them for
//! method calls that need cross-cutting access.

pub mod spawn;
mod utility;

use ::rand::Rng;
use bevy::prelude::{Mut, Vec2};
use strum::IntoEnumIterator;

use crate::components::{
    ask_multiplier, bid_multiplier, Commodity, DockAction, PriceEntry, PriceLedger, ShipArchetype,
    ShipLedger, ShipMovement, ShipProfile, ShipTrading, StrategyGenes, COMMODITY_COUNT,
};
use crate::island::IslandEconomy;

/// Units of commodity bought or sold per dock trade action.
const TRADE_ACTION_VOLUME: f32 = 18.0;
/// Cash each ship starts with.
pub const STARTING_CASH: f32 = 1000.0;
/// Default bid/ask spread fraction applied to market prices.
const DEFAULT_MARKET_SPREAD: f32 = 0.10;
/// EMA learning rate for route profitability history.
const ROUTE_LEARNING_RATE: f32 = 0.20;
/// Per-tick decay on route profitability memory.
const ROUTE_LEARNING_DECAY: f32 = 0.98;
/// Base cargo volume capacity before archetype scaling.
const BASE_CARGO_VOLUME_CAPACITY: f32 = 22.0;
/// Per-tick labor cost (crew wages) base rate.
pub const BASE_LABOR_RATE: f32 = 0.0002;
/// Per-distance wear cost base rate.
pub const BASE_WEAR_RATE: f32 = 0.00006;
/// Minimum sell price as a fraction of the purchase price.
const RESERVE_PRICE_FLOOR: f32 = 0.60;
/// Per-tick utility penalty ramp while idling in dock.
const DOCK_IDLE_RISK_RAMP_PER_TICK: f32 = 0.015;
/// Maximum allowed idle-dock utility floor before forced departure.
const MAX_DOCK_IDLE_RISK_ALLOWANCE: f32 = 1.5;
/// Minimum positive utility required to commit to buying cargo.
const MIN_BEST_UTILITY_FOR_LOAD: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
pub struct PlanningTuning {
    pub global_friction_mult: f32,
    pub info_decay_rate: f32,
    pub market_spread: f32,
}

pub struct LoadPlanningContext<'a> {
    pub current_island_id: usize,
    pub island_positions: &'a [Vec2],
    pub current_tick: u64,
    pub tuning: &'a PlanningTuning,
    pub outbound_recent_departures: &'a [f32],
}

impl Default for PlanningTuning {
    fn default() -> Self {
        Self {
            global_friction_mult: 1.0,
            info_decay_rate: 0.003,
            market_spread: DEFAULT_MARKET_SPREAD,
        }
    }
}

/// Target fleet size per island, used by friction, fleet evolution, and HUD.
pub const TARGET_SHIPS_PER_ISLAND: f32 = 3.0;

/// Minimum ship speed (world units per tick).
const MIN_SHIP_SPEED: f32 = 120.0;
/// Maximum ship speed (world units per tick).
const MAX_SHIP_SPEED: f32 = 600.0;
/// Labor cost multiplier applied while docked (port fees).
pub(crate) const DOCKED_PORT_FEE_MULTIPLIER: f32 = 1.5;
/// Extra wear multiplier when cargo load is heavy.
pub(crate) const HEAVY_LOAD_WEAR_MULTIPLIER: f32 = 1.1;
/// Negative cash threshold that triggers bankruptcy/scuttle.
const BANKRUPTCY_CASH_FLOOR: f32 = -20.0;
/// Base docking tax rate applied on arrival.
const BASE_DOCKING_TAX_RATE: f32 = 0.0015;
/// Maximum docking tax rate cap.
const MAX_DOCKING_TAX_RATE: f32 = 0.02;
/// How steeply docking tax rises with liquidity imbalance.
const LIQUIDITY_IMBALANCE_TAX_SLOPE: f32 = 0.01;
/// Fraction of cash reserved before docking tax applies.
const DOCKING_TAX_CASH_RESERVE_MULTIPLIER: f32 = 0.75;
/// Per-tick fee charged to ships idling in port beyond the grace period.
const IDLE_PORT_FEE_PER_TICK: f32 = STARTING_CASH * 0.002;
/// Ticks of free docking before idle port fees kick in.
const IDLE_FEE_GRACE_TICKS: u32 = 300;
/// Floor on ship efficiency rating (prevents extreme under-performance).
const MIN_EFFICIENCY_RATING: f32 = 0.80;
/// Ceiling on ship efficiency rating.
const MAX_EFFICIENCY_RATING: f32 = 1.30;
/// Minimum gene scaling factor for strategy gene randomization.
const MIN_GENE_SCALE: f32 = 0.80;
/// Maximum gene scaling factor for strategy gene randomization.
const MAX_GENE_SCALE: f32 = 1.20;

/// Unified ship state used by simulation methods. In the ECS, this is
/// decomposed into ShipMovement, ShipTrading, ShipProfile, and ShipLedger
/// components. This struct reassembles them for method calls.
pub struct ShipState {
    pub(crate) pos: Vec2,
    pub(crate) target: Vec2,
    pub(crate) speed: f32,
    pub(crate) base_speed: f32,
    pub(crate) archetype: ShipArchetype,
    pub(crate) efficiency_rating: f32,
    pub(crate) max_cargo_volume: f32,
    pub(crate) target_island_id: Option<usize>,
    pub(crate) docked_at: Option<usize>,
    pub(crate) last_docked_island_id: Option<usize>,
    pub(crate) cargo: Option<(Commodity, f32)>,
    pub(crate) cash: f32,
    pub(crate) labor_debt: f32,
    pub(crate) wear_debt: f32,
    pub(crate) ledger: PriceLedger,
    pub(crate) route_memory: Vec<f32>,
    pub(crate) purchase_price: f32,
    pub(crate) strategy_genes: StrategyGenes,
    pub(crate) planned_target_after_load: Option<usize>,
    pub(crate) cargo_changed_this_dock: bool,
    pub(crate) last_step_distance: f32,
    pub(crate) just_sold_resource: Option<Commodity>,
    pub(crate) last_dock_action: DockAction,
    pub(crate) dock_idle_ticks: u32,
    pub(crate) home_island_id: Option<usize>,
}

#[allow(dead_code)]
impl ShipState {
    pub fn new(pos: Vec2, speed: f32, num_islands: usize, docked_island_id: usize) -> Self {
        let mut rng = ::rand::thread_rng();
        let archetype = match rng.gen_range(0u8..3) {
            0 => ShipArchetype::Clipper,
            1 => ShipArchetype::Freighter,
            _ => ShipArchetype::Shorthaul,
        };
        let efficiency_rating = rng.gen_range(MIN_EFFICIENCY_RATING..MAX_EFFICIENCY_RATING);
        Self {
            pos,
            target: pos,
            speed,
            base_speed: speed,
            archetype,
            efficiency_rating,
            max_cargo_volume: 0.0,
            target_island_id: Some(docked_island_id),
            docked_at: Some(docked_island_id),
            last_docked_island_id: Some(docked_island_id),
            cargo: None,
            cash: STARTING_CASH,
            labor_debt: 0.0,
            wear_debt: 0.0,
            ledger: vec![
                PriceEntry {
                    prices: [0.0; COMMODITY_COUNT],
                    inventories: [0.0; COMMODITY_COUNT],
                    capacities: [crate::components::INVENTORY_CARRYING_CAPACITY; COMMODITY_COUNT],
                    cash: 0.0,
                    infrastructure_level: 0.0,
                    tick_updated: 0,
                    last_seen_tick: 0,
                };
                num_islands
            ],
            route_memory: vec![0.0; num_islands],
            purchase_price: 0.0,
            strategy_genes: StrategyGenes::default(),
            planned_target_after_load: None,
            cargo_changed_this_dock: false,
            last_step_distance: 0.0,
            just_sold_resource: None,
            last_dock_action: DockAction::None,
            dock_idle_ticks: 0,
            home_island_id: None,
        }
        .with_recomputed_traits()
    }

    fn with_recomputed_traits(mut self) -> Self {
        self.recompute_operational_traits();
        self
    }

    fn recompute_operational_traits(&mut self) {
        let archetype = self.archetype;
        let (speed_mult, capacity_mult, _, _) = Self::profile_multipliers_static(archetype);

        let efficiency_speed_factor =
            (0.92 + 0.30 * (self.efficiency_rating - 1.0)).clamp(0.85, 1.10);
        self.speed = (self.base_speed * speed_mult * efficiency_speed_factor)
            .clamp(MIN_SHIP_SPEED, MAX_SHIP_SPEED);

        let efficiency_capacity_factor =
            (0.95 + 0.10 * (self.efficiency_rating - 1.0)).clamp(0.90, 1.05);
        self.max_cargo_volume =
            (BASE_CARGO_VOLUME_CAPACITY * capacity_mult * efficiency_capacity_factor)
                .clamp(8.0, 80.0);
    }

    pub fn profile_multipliers_static(archetype: ShipArchetype) -> (f32, f32, f32, f32) {
        match archetype {
            // (speed_mult, capacity_mult, labor_mult, wear_mult)
            ShipArchetype::Clipper => (1.50, 0.50, 1.50, 1.50),
            ShipArchetype::Shorthaul => (1.00, 1.00, 0.50, 0.25),
            ShipArchetype::Freighter => (0.75, 2.00, 0.75, 1.00),
        }
    }

    // ── Accessors ──────────────────────────────────────────────────────

    pub fn pos(&self) -> Vec2 {
        self.pos
    }
    pub fn speed(&self) -> f32 {
        self.speed
    }
    pub fn max_cargo_volume(&self) -> f32 {
        self.max_cargo_volume
    }
    pub fn archetype(&self) -> ShipArchetype {
        self.archetype
    }
    pub fn set_archetype(&mut self, archetype: ShipArchetype) {
        self.archetype = archetype;
        self.recompute_operational_traits();
    }
    pub fn docked_island(&self) -> Option<usize> {
        self.docked_at
    }
    pub fn last_docked_island(&self) -> Option<usize> {
        self.docked_at.or(self.last_docked_island_id)
    }
    pub fn set_home_island(&mut self, island_id: usize) {
        self.home_island_id = Some(island_id);
    }
    pub fn target_island(&self) -> Option<usize> {
        self.target_island_id
    }
    pub fn current_cargo(&self) -> Option<(Commodity, f32)> {
        self.cargo
    }
    pub fn has_no_cargo(&self) -> bool {
        self.cargo.is_none()
    }
    pub fn just_sold_resource(&self) -> Option<Commodity> {
        self.just_sold_resource
    }
    pub fn cargo_changed_this_dock(&self) -> bool {
        self.cargo_changed_this_dock
    }
    pub fn ledger(&self) -> &PriceLedger {
        &self.ledger
    }
    pub fn ledger_mut(&mut self) -> &mut PriceLedger {
        &mut self.ledger
    }
    pub fn set_cash(&mut self, cash: f32) {
        self.cash = cash;
    }
    pub fn current_cash(&self) -> f32 {
        self.cash
    }
    pub fn deduct_cash(&mut self, amount: f32) {
        self.cash -= amount;
    }

    pub fn cargo_volume_used(&self) -> f32 {
        self.total_cargo_volume()
    }

    pub fn wear_rate(&self) -> f32 {
        let (_, _, _, wear_mult) = Self::profile_multipliers_static(self.archetype);
        let efficiency_factor = (1.20 - 0.40 * self.efficiency_rating).clamp(0.65, 1.15);
        BASE_WEAR_RATE * wear_mult * efficiency_factor
    }

    pub fn labor_rate(&self) -> f32 {
        let (_, _, labor_mult, _) = Self::profile_multipliers_static(self.archetype);
        let efficiency_factor = (1.20 - 0.35 * self.efficiency_rating).clamp(0.70, 1.15);
        BASE_LABOR_RATE * labor_mult * efficiency_factor
    }

    fn risk_tolerance(&self) -> f32 {
        self.strategy_genes.risk_tolerance_scale.max(0.25)
    }

    pub fn effective_tuning(&self, base: &PlanningTuning) -> PlanningTuning {
        let mut tuned = *base;
        tuned.info_decay_rate =
            (tuned.info_decay_rate * self.strategy_genes.confidence_decay_scale).max(0.0001);
        tuned.global_friction_mult = tuned.global_friction_mult.clamp(0.2, 6.0);
        tuned.market_spread = tuned.market_spread.clamp(0.02, 0.80);
        tuned
    }

    fn total_cargo_volume(&self) -> f32 {
        match self.cargo {
            Some((resource, amount)) => amount.max(0.0) * resource.volume_per_unit(),
            None => 0.0,
        }
    }

    fn remaining_cargo_volume(&self) -> f32 {
        (self.max_cargo_volume - self.total_cargo_volume()).max(0.0)
    }

    fn max_units_for_trade_action(&self, resource: Commodity) -> f32 {
        TRADE_ACTION_VOLUME / resource.volume_per_unit().max(0.01)
    }

    fn load_utility_floor(&self) -> f32 {
        -((self.dock_idle_ticks as f32) * DOCK_IDLE_RISK_RAMP_PER_TICK)
            .min(MAX_DOCK_IDLE_RISK_ALLOWANCE)
    }

    fn total_service_debt(&self) -> f32 {
        self.labor_debt.max(0.0) + self.wear_debt.max(0.0)
    }

    pub fn estimated_net_worth(&self) -> f32 {
        let mut net_worth = self.cash.max(0.0);
        if let Some((resource, amount)) = self.cargo {
            let amount = amount.max(0.0);
            if amount > 0.0 {
                let cargo_book_price = if self.purchase_price > 0.0 {
                    self.purchase_price
                } else {
                    self.median_price_for_resource(resource)
                };
                net_worth +=
                    (cargo_book_price * bid_multiplier(DEFAULT_MARKET_SPREAD) * amount).max(0.0);
            }
        }
        net_worth - self.total_service_debt()
    }

    pub fn is_bankrupt(&self) -> bool {
        self.cash - self.total_service_debt() < BANKRUPTCY_CASH_FLOOR
    }

    pub fn removal_cash_settlement(&self) -> f32 {
        self.cash.max(0.0)
    }

    // ── Movement ───────────────────────────────────────────────────────

    pub fn update(&mut self, dt: f32) -> Option<usize> {
        let to_target = self.target - self.pos;
        let dist = to_target.length();
        self.last_step_distance = 0.0;
        if dist < 1.0 {
            self.docked_at = self.target_island_id;
            self.last_docked_island_id = self.docked_at;
            return self.docked_at;
        }
        let step = self.speed * dt;
        self.last_step_distance = step.min(dist);
        if step >= dist {
            self.pos = self.target;
            self.docked_at = self.target_island_id;
            self.last_docked_island_id = self.docked_at;
            self.docked_at
        } else {
            self.pos += to_target.normalize() * step;
            None
        }
    }

    pub fn set_target(&mut self, target_island_id: usize, target: Vec2) {
        self.target = target;
        self.target_island_id = Some(target_island_id);
        self.docked_at = None;
        self.planned_target_after_load = None;
        self.dock_idle_ticks = 0;
    }

    // ── Friction ───────────────────────────────────────────────────────

    pub fn apply_maritime_friction(&mut self, dt: f32, global_friction_mult: f32) {
        let mut labor = self.labor_rate() * dt.max(0.0) * global_friction_mult;
        if self.docked_at.is_some() {
            labor *= DOCKED_PORT_FEE_MULTIPLIER;
        }

        let cargo_load_ratio = if let Some((resource, amount)) = self.cargo {
            (amount.max(0.0) * resource.volume_per_unit() / self.max_cargo_volume.max(0.01))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let wear_multiplier = 1.0 + cargo_load_ratio * HEAVY_LOAD_WEAR_MULTIPLIER;
        let wear = self.last_step_distance.max(0.0)
            * self.wear_rate()
            * global_friction_mult
            * wear_multiplier;

        self.labor_debt += labor.max(0.0);
        self.wear_debt += wear.max(0.0);
        self.last_step_distance = 0.0;
    }

    // ── Docking / Trading ──────────────────────────────────────────────

    pub fn begin_dock_tick(&mut self) {
        self.last_dock_action = DockAction::None;
        self.planned_target_after_load = None;
        self.cargo_changed_this_dock = false;
        self.just_sold_resource = None;
        let decay = ROUTE_LEARNING_DECAY.clamp(0.0, 1.0);
        for score in &mut self.route_memory {
            *score *= decay;
        }
    }

    pub fn settle_service_debt(&mut self, island: &mut IslandEconomy) -> f32 {
        let total_debt = self.total_service_debt();
        if total_debt <= 0.0 || self.cash <= 0.0 {
            return 0.0;
        }

        let payment = self.cash.min(total_debt);
        let wear_share = (self.wear_debt.max(0.0) / total_debt).clamp(0.0, 1.0);
        let wear_paid = payment * wear_share;
        let labor_paid = payment - wear_paid;

        self.wear_debt = (self.wear_debt - wear_paid).max(0.0);
        self.labor_debt = (self.labor_debt - labor_paid).max(0.0);
        self.cash -= payment;
        island.accept_service_payment(wear_paid, labor_paid);
        payment
    }

    pub fn pay_dynamic_docking_tax(&mut self, island: &mut IslandEconomy) -> f32 {
        let reserve_cash = STARTING_CASH * DOCKING_TAX_CASH_RESERVE_MULTIPLIER;
        let taxable_cash = (self.cash - reserve_cash).max(0.0);
        if taxable_cash <= 0.0 {
            return 0.0;
        }

        let island_cash = (island.cash + STARTING_CASH).max(1.0);
        let ship_island_ratio = self.cash.max(0.0) / island_cash;
        let tax_rate = (BASE_DOCKING_TAX_RATE
            + (ship_island_ratio - 1.0).max(0.0) * LIQUIDITY_IMBALANCE_TAX_SLOPE)
            .clamp(0.0, MAX_DOCKING_TAX_RATE);
        let tax = taxable_cash * tax_rate;
        self.cash -= tax;
        island.cash += tax;
        tax
    }

    pub fn pay_idle_port_fee(&mut self, island: &mut IslandEconomy) -> f32 {
        self.dock_idle_ticks = self.dock_idle_ticks.saturating_add(1);
        if self.dock_idle_ticks <= IDLE_FEE_GRACE_TICKS {
            return 0.0;
        }
        let fee = IDLE_PORT_FEE_PER_TICK;
        self.cash -= fee;
        island.cash += fee.max(0.0);
        fee
    }

    pub fn trade_unload_if_carrying(
        &mut self,
        island_id: usize,
        island: &mut IslandEconomy,
        tuning: &PlanningTuning,
    ) -> f32 {
        let Some((resource, carrying_amount)) = self.cargo else {
            return 0.0;
        };
        if carrying_amount <= 0.0 {
            return 0.0;
        }
        let bid_price = island.bid_price(resource, tuning.market_spread);
        if !bid_price.is_finite() || bid_price <= 0.0 {
            return 0.0;
        }

        // Refuse to sell at a big loss — clear target so departure planning picks a better port.
        if self.purchase_price > 0.0 && bid_price < self.purchase_price * RESERVE_PRICE_FLOOR {
            self.target_island_id = None;
            return 0.0;
        }

        let (sold_amount, gross_revenue) =
            island.sell_to_island(resource, carrying_amount, tuning.market_spread);
        if sold_amount <= 0.0 || gross_revenue <= 0.0 {
            return 0.0;
        }

        self.cash += gross_revenue;

        if self.purchase_price > 0.0 && sold_amount > 0.0 && island_id < self.route_memory.len() {
            let sale_unit_price = gross_revenue / sold_amount;
            let normalized_margin =
                (sale_unit_price - self.purchase_price) / (self.purchase_price + 1.0);
            self.route_memory[island_id] += normalized_margin * ROUTE_LEARNING_RATE;
            self.route_memory[island_id] = self.route_memory[island_id].clamp(-1.5, 1.5);
        }

        let remaining = (carrying_amount - sold_amount).max(0.0);
        if remaining <= 0.0 {
            self.cargo = None;
            self.purchase_price = 0.0;
        } else {
            self.cargo = Some((resource, remaining));
        }
        self.cargo_changed_this_dock = true;

        self.just_sold_resource = Some(resource);
        self.last_dock_action = DockAction::Sold;
        self.dock_idle_ticks = 0;
        sold_amount
    }

    pub fn trade_settle_until_stuck(
        &mut self,
        current_island_id: usize,
        island: &mut IslandEconomy,
        tuning: &PlanningTuning,
        max_steps: usize,
    ) -> bool {
        let mut settled_any = false;
        for _ in 0..max_steps.max(1) {
            if self.has_no_cargo() {
                break;
            }

            self.last_dock_action = DockAction::None;
            let sold_amount = self.trade_unload_if_carrying(current_island_id, island, tuning);
            if sold_amount > 0.0 {
                settled_any = true;
            } else {
                break;
            }

            if self.is_bankrupt() {
                break;
            }
        }

        self.last_dock_action = if settled_any {
            DockAction::Sold
        } else {
            DockAction::None
        };
        settled_any
    }

    pub fn trade_load_if_empty(
        &mut self,
        island: &mut IslandEconomy,
        exclude: Option<Commodity>,
        context: &LoadPlanningContext<'_>,
    ) -> DockAction {
        let remaining_volume = self.remaining_cargo_volume();
        if remaining_volume <= 0.0 {
            return self.last_dock_action;
        }

        let mut chosen_resource: Option<Commodity> = None;
        let mut chosen_target: Option<usize> = None;
        let mut chosen_local_price = 0.0;
        let mut best_utility = f32::NEG_INFINITY;
        let utility_context = utility::UtilityContext {
            island_positions: context.island_positions,
            max_route_distance: self.max_route_distance_for_planning(context.island_positions),
            current_tick: context.current_tick,
            tuning: context.tuning,
            outbound_recent_departures: context.outbound_recent_departures,
        };

        for resource in Commodity::iter() {
            if Some(resource) == exclude {
                continue;
            }
            let idx = resource.idx();
            let local_price = island.ask_price(resource, context.tuning.market_spread);
            if !local_price.is_finite() || local_price <= 0.0 {
                continue;
            }

            let available = island.inventory[idx].max(0.0);
            if available <= 0.0 {
                continue;
            }

            let affordable = (self.cash / local_price).max(0.0);
            let max_units_by_volume = remaining_volume / resource.volume_per_unit().max(0.01);
            let projected_amount = max_units_by_volume.min(affordable).min(available);
            if projected_amount <= 0.0 {
                continue;
            }

            for target_id in 0..self.ledger.len() {
                if target_id == context.current_island_id {
                    continue;
                }

                let utility = self.calculate_utility(
                    resource,
                    target_id,
                    local_price,
                    projected_amount,
                    &utility_context,
                );

                if utility > best_utility {
                    best_utility = utility;
                    chosen_resource = Some(resource);
                    chosen_target = Some(target_id);
                    chosen_local_price = local_price;
                }
            }
        }

        if !chosen_local_price.is_finite() || chosen_local_price <= 0.0 {
            self.dock_idle_ticks = self.dock_idle_ticks.saturating_add(1);
            return self.last_dock_action;
        }

        if !best_utility.is_finite() || best_utility < MIN_BEST_UTILITY_FOR_LOAD {
            self.dock_idle_ticks = self.dock_idle_ticks.saturating_add(1);
            return self.last_dock_action;
        }

        let Some(chosen_resource) = chosen_resource else {
            return self.last_dock_action;
        };

        let Some(chosen_target) = chosen_target else {
            return self.last_dock_action;
        };

        let affordable = (self.cash / chosen_local_price).max(0.0);
        let max_units_by_volume =
            self.remaining_cargo_volume() / chosen_resource.volume_per_unit().max(0.01);
        let requested = max_units_by_volume.min(affordable);
        if requested <= 0.0 {
            return self.last_dock_action;
        }

        let (filled, total_cost) =
            island.buy_from_island(chosen_resource, requested, context.tuning.market_spread);
        if filled <= 0.0 {
            return self.last_dock_action;
        }
        self.cash -= total_cost;
        let filled_unit_price = total_cost / filled;
        self.purchase_price = filled_unit_price;
        self.cargo = Some((chosen_resource, filled));
        self.cargo_changed_this_dock = true;
        self.planned_target_after_load = Some(chosen_target);
        self.last_dock_action = DockAction::Bought;
        self.dock_idle_ticks = 0;
        self.last_dock_action
    }

    // ── Ledger sync ────────────────────────────────────────────────────

    pub fn sync_ledger_from_snapshot(&mut self, island_ledger_snapshot: &[PriceEntry]) {
        let len = self.ledger.len().min(island_ledger_snapshot.len());
        for (i, ship_entry) in self.ledger.iter_mut().enumerate().take(len) {
            if island_ledger_snapshot[i].tick_updated >= ship_entry.tick_updated {
                ship_entry.prices = island_ledger_snapshot[i].prices;
                ship_entry.inventories = island_ledger_snapshot[i].inventories;
                ship_entry.capacities = island_ledger_snapshot[i].capacities;
                ship_entry.cash = island_ledger_snapshot[i].cash;
                ship_entry.infrastructure_level = island_ledger_snapshot[i].infrastructure_level;
                ship_entry.tick_updated = island_ledger_snapshot[i].tick_updated;
            }
            if island_ledger_snapshot[i].last_seen_tick >= ship_entry.last_seen_tick {
                ship_entry.last_seen_tick = island_ledger_snapshot[i].last_seen_tick;
            }
        }
    }

    pub fn contribute_ledger_to_island_buffer(
        &self,
        island_id: usize,
        island_ledger_buffer: &mut PriceLedger,
    ) {
        let len = island_ledger_buffer.len().min(self.ledger.len());
        for (i, incoming_entry) in self.ledger.iter().copied().enumerate().take(len) {
            if i == island_id {
                continue;
            }
            if incoming_entry.tick_updated > island_ledger_buffer[i].tick_updated {
                island_ledger_buffer[i].prices = incoming_entry.prices;
                island_ledger_buffer[i].inventories = incoming_entry.inventories;
                island_ledger_buffer[i].capacities = incoming_entry.capacities;
                island_ledger_buffer[i].cash = incoming_entry.cash;
                island_ledger_buffer[i].infrastructure_level = incoming_entry.infrastructure_level;
                island_ledger_buffer[i].tick_updated = incoming_entry.tick_updated;
            }
            if incoming_entry.last_seen_tick > island_ledger_buffer[i].last_seen_tick {
                island_ledger_buffer[i].last_seen_tick = incoming_entry.last_seen_tick;
            }
        }
    }

    // ── Route planning ─────────────────────────────────────────────────

    fn map_span(island_positions: &[Vec2]) -> f32 {
        if island_positions.is_empty() {
            return 1.0;
        }
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for pos in island_positions {
            min_x = min_x.min(pos.x);
            max_x = max_x.max(pos.x);
            min_y = min_y.min(pos.y);
            max_y = max_y.max(pos.y);
        }
        (max_x - min_x).max(max_y - min_y).max(1.0)
    }

    fn max_route_distance_for_planning(&self, island_positions: &[Vec2]) -> f32 {
        match self.archetype {
            ShipArchetype::Shorthaul => Self::map_span(island_positions) * 0.20,
            _ => f32::INFINITY,
        }
    }

    pub fn plan_next_island(
        &self,
        current_island_id: usize,
        island_positions: &[Vec2],
        current_tick: u64,
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
    ) -> Option<usize> {
        if let Some((resource, amount)) = self.cargo {
            let utility_context = utility::UtilityContext {
                island_positions,
                max_route_distance: self.max_route_distance_for_planning(island_positions),
                current_tick,
                tuning,
                outbound_recent_departures,
            };

            if let Some(target_id) = self.planned_target_after_load {
                if target_id != current_island_id {
                    return Some(target_id);
                }
            }

            let lot_size = amount.max(0.0);
            let reference_buy_price = if self.purchase_price > 0.0 {
                self.purchase_price
            } else {
                self.median_price_for_resource(resource)
            };

            let mut best_target = None;
            let mut best_utility = f32::NEG_INFINITY;
            for target_id in 0..self.ledger.len() {
                if target_id == current_island_id {
                    continue;
                }
                let utility = self.calculate_utility(
                    resource,
                    target_id,
                    reference_buy_price,
                    lot_size,
                    &utility_context,
                );
                if utility.is_finite() && utility > best_utility {
                    best_utility = utility;
                    best_target = Some(target_id);
                }
            }

            if !best_utility.is_finite() {
                return None;
            }

            return best_target;
        }

        let mut best_target = None;
        let mut best_utility = f32::NEG_INFINITY;
        let utility_context = utility::UtilityContext {
            island_positions,
            max_route_distance: self.max_route_distance_for_planning(island_positions),
            current_tick,
            tuning,
            outbound_recent_departures,
        };
        let current_prices = self
            .ledger
            .get(current_island_id)
            .map(|entry| entry.prices)
            .unwrap_or([0.0; COMMODITY_COUNT]);

        for target_id in 0..self.ledger.len() {
            if target_id == current_island_id {
                continue;
            }

            let mut best_resource_utility = f32::NEG_INFINITY;
            for resource in Commodity::iter() {
                let buy_price =
                    current_prices[resource.idx()] * ask_multiplier(tuning.market_spread);
                let lot_size = self
                    .max_units_for_trade_action(resource)
                    .min(self.max_cargo_volume / resource.volume_per_unit().max(0.01));
                let utility = self.calculate_utility(
                    resource,
                    target_id,
                    buy_price,
                    lot_size,
                    &utility_context,
                );

                if utility > best_resource_utility {
                    best_resource_utility = utility;
                }
            }

            if best_resource_utility > best_utility {
                best_utility = best_resource_utility;
                best_target = Some(target_id);
            }
        }

        if best_utility > 0.0 {
            best_target
        } else {
            None
        }
    }

    fn median_price_for_resource(&self, resource: Commodity) -> f32 {
        let index = resource.idx();
        let mut prices: Vec<f32> = self
            .ledger
            .iter()
            .map(|entry| entry.prices[index])
            .filter(|price| price.is_finite() && *price > 0.0)
            .collect();

        if prices.is_empty() {
            return 0.0;
        }

        prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = prices.len() / 2;
        if prices.len().is_multiple_of(2) {
            (prices[mid - 1] + prices[mid]) * 0.5
        } else {
            prices[mid]
        }
    }

    fn median_island_cash(&self) -> Option<f32> {
        let mut cash_values: Vec<f32> = self
            .ledger
            .iter()
            .map(|entry| entry.cash)
            .filter(|cash| cash.is_finite() && *cash > 0.0)
            .collect();

        if cash_values.is_empty() {
            return None;
        }

        cash_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = cash_values.len() / 2;
        if cash_values.len().is_multiple_of(2) {
            Some((cash_values[mid - 1] + cash_values[mid]) * 0.5)
        } else {
            Some(cash_values[mid])
        }
    }

    // ── Seeding ────────────────────────────────────────────────────────

    pub fn seed_initial_market_view(
        &mut self,
        islands: &[(Vec2, IslandEconomy, PriceLedger)],
        current_tick: u64,
        home_island_id: usize,
        rng: &mut impl Rng,
    ) {
        let count = self.ledger.len().min(islands.len());
        for (island_id, (_, economy, _)) in islands.iter().enumerate().take(count) {
            let mut prices = [0.0; COMMODITY_COUNT];
            let mut inventories = [0.0; COMMODITY_COUNT];

            for resource in Commodity::iter() {
                let idx = resource.idx();
                let price_noise = rng.gen_range(0.82..1.18);
                let inventory_noise = rng.gen_range(0.70..1.30);
                prices[idx] = (economy.local_prices[idx] * price_noise).max(0.0);
                inventories[idx] = (economy.inventory[idx] * inventory_noise).max(0.0);
            }

            let observed_cash = (economy.cash * rng.gen_range(0.75..1.25)).max(0.0);
            let observed_infra =
                (economy.infrastructure_level * rng.gen_range(0.90..1.10)).max(0.0);
            let age = rng.gen_range(40_u64..=420_u64);
            let observed_tick = current_tick.saturating_sub(age);

            self.ledger[island_id] = PriceEntry {
                prices,
                inventories,
                capacities: economy.resource_capacity,
                cash: observed_cash,
                infrastructure_level: observed_infra,
                tick_updated: observed_tick,
                last_seen_tick: observed_tick,
            };
        }

        if home_island_id < count {
            let (_, economy, _) = &islands[home_island_id];
            self.ledger[home_island_id] = PriceEntry {
                prices: economy.local_prices,
                inventories: economy.inventory,
                capacities: economy.resource_capacity,
                cash: economy.cash,
                infrastructure_level: economy.infrastructure_level,
                tick_updated: current_tick,
                last_seen_tick: current_tick,
            };
        }
    }

    // ── Spawning daughters ─────────────────────────────────────────────

    pub fn spawn_daughter(
        &mut self,
        mutation_strength: f32,
        rng: &mut impl Rng,
    ) -> Option<ShipState> {
        let num_islands = self.ledger.len();
        if num_islands == 0 {
            return None;
        }
        let spawn_island_id = self
            .docked_island()
            .or(self.target_island_id)
            .unwrap_or(0)
            .min(num_islands - 1);

        let speed_mutation = 1.0 + rng.gen_range(-mutation_strength..mutation_strength);
        let daughter_base_speed =
            (self.base_speed * speed_mutation).clamp(MIN_SHIP_SPEED, MAX_SHIP_SPEED);

        let endowment = self.cash * 0.5;
        self.cash -= endowment;

        let mut daughter =
            ShipState::new(self.pos, daughter_base_speed, num_islands, spawn_island_id);
        daughter.cash = endowment;
        daughter.ledger = self.ledger.clone();
        daughter.route_memory = self.route_memory.clone();
        if self.docked_island().is_none() {
            if let Some(target_id) = self.target_island_id {
                daughter.set_target(target_id.min(num_islands - 1), self.target);
            }
        }
        daughter.archetype = if rng.gen::<f32>() < mutation_strength * 0.5 {
            match rng.gen_range(0u8..3) {
                0 => ShipArchetype::Clipper,
                1 => ShipArchetype::Freighter,
                _ => ShipArchetype::Shorthaul,
            }
        } else {
            self.archetype
        };
        daughter.efficiency_rating = mutate_gene_gaussian(
            self.efficiency_rating,
            mutation_strength,
            rng,
            MIN_EFFICIENCY_RATING,
            MAX_EFFICIENCY_RATING,
        );
        daughter.recompute_operational_traits();

        daughter.strategy_genes = StrategyGenes {
            confidence_decay_scale: mutate_gene_gaussian(
                self.strategy_genes.confidence_decay_scale,
                mutation_strength,
                rng,
                MIN_GENE_SCALE,
                MAX_GENE_SCALE,
            ),
            risk_tolerance_scale: mutate_gene_gaussian(
                self.strategy_genes.risk_tolerance_scale,
                mutation_strength,
                rng,
                MIN_GENE_SCALE,
                MAX_GENE_SCALE,
            ),
        };

        Some(daughter)
    }

    // ── ECS conversion helpers ─────────────────────────────────────────

    /// Convert this ShipState into ECS components.
    pub fn into_components(self) -> (ShipMovement, ShipTrading, ShipProfile, ShipLedger) {
        (
            ShipMovement {
                target: self.target,
                speed: self.speed,
                base_speed: self.base_speed,
                target_island_id: self.target_island_id,
                last_step_distance: self.last_step_distance,
            },
            ShipTrading {
                docked_at: self.docked_at,
                last_docked_island_id: self.last_docked_island_id,
                cargo: self.cargo,
                cash: self.cash,
                labor_debt: self.labor_debt,
                wear_debt: self.wear_debt,
                purchase_price: self.purchase_price,
                planned_target_after_load: self.planned_target_after_load,
                cargo_changed_this_dock: self.cargo_changed_this_dock,
                just_sold_resource: self.just_sold_resource,
                last_dock_action: self.last_dock_action,
                dock_idle_ticks: self.dock_idle_ticks,
            },
            ShipProfile {
                archetype: self.archetype,
                efficiency_rating: self.efficiency_rating,
                max_cargo_volume: self.max_cargo_volume,
                strategy_genes: self.strategy_genes,
                home_island_id: self.home_island_id,
            },
            ShipLedger {
                ledger: self.ledger,
                route_memory: self.route_memory,
            },
        )
    }

    /// Reconstitute a ShipState from ECS components (borrows).
    pub fn from_components(
        pos: Vec2,
        movement: &ShipMovement,
        trading: &ShipTrading,
        profile: &ShipProfile,
        ship_ledger: &ShipLedger,
    ) -> Self {
        Self {
            pos,
            target: movement.target,
            speed: movement.speed,
            base_speed: movement.base_speed,
            target_island_id: movement.target_island_id,
            last_step_distance: movement.last_step_distance,
            archetype: profile.archetype,
            efficiency_rating: profile.efficiency_rating,
            max_cargo_volume: profile.max_cargo_volume,
            docked_at: trading.docked_at,
            last_docked_island_id: trading.last_docked_island_id,
            cargo: trading.cargo,
            cash: trading.cash,
            labor_debt: trading.labor_debt,
            wear_debt: trading.wear_debt,
            purchase_price: trading.purchase_price,
            strategy_genes: profile.strategy_genes,
            home_island_id: profile.home_island_id,
            planned_target_after_load: trading.planned_target_after_load,
            cargo_changed_this_dock: trading.cargo_changed_this_dock,
            just_sold_resource: trading.just_sold_resource,
            last_dock_action: trading.last_dock_action,
            dock_idle_ticks: trading.dock_idle_ticks,
            ledger: ship_ledger.ledger.clone(),
            route_memory: ship_ledger.route_memory.clone(),
        }
    }

    /// Write back mutated state into ECS components (Mut refs from exclusive system).
    pub fn write_back_to_components(
        self,
        movement: &mut Mut<ShipMovement>,
        trading: &mut Mut<ShipTrading>,
        profile: &mut Mut<ShipProfile>,
        ship_ledger: &mut Mut<ShipLedger>,
    ) {
        movement.target = self.target;
        movement.speed = self.speed;
        movement.base_speed = self.base_speed;
        movement.target_island_id = self.target_island_id;
        movement.last_step_distance = self.last_step_distance;
        trading.docked_at = self.docked_at;
        trading.last_docked_island_id = self.last_docked_island_id;
        trading.cargo = self.cargo;
        trading.cash = self.cash;
        trading.labor_debt = self.labor_debt;
        trading.wear_debt = self.wear_debt;
        trading.purchase_price = self.purchase_price;
        trading.planned_target_after_load = self.planned_target_after_load;
        trading.cargo_changed_this_dock = self.cargo_changed_this_dock;
        trading.just_sold_resource = self.just_sold_resource;
        trading.last_dock_action = self.last_dock_action;
        trading.dock_idle_ticks = self.dock_idle_ticks;
        profile.archetype = self.archetype;
        profile.efficiency_rating = self.efficiency_rating;
        profile.max_cargo_volume = self.max_cargo_volume;
        profile.strategy_genes = self.strategy_genes;
        profile.home_island_id = self.home_island_id;
        ship_ledger.ledger = self.ledger;
        ship_ledger.route_memory = self.route_memory;
    }
}

fn mutate_gene_gaussian(
    value: f32,
    mutation_strength: f32,
    rng: &mut impl Rng,
    min_value: f32,
    max_value: f32,
) -> f32 {
    let sigma = mutation_strength.max(0.0001);
    let u1: f32 = rng.gen_range(f32::EPSILON..1.0);
    let u2: f32 = rng.gen_range(0.0..1.0);
    let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
    (value + z0 * sigma).clamp(min_value, max_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island::IslandEconomy;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn update_reaches_target_and_docks() {
        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 3, 0);
        ship.set_target(1, Vec2::new(10.0, 0.0));

        let docked = ship.update(1.0);

        assert_eq!(docked, Some(1));
        assert_eq!(ship.docked_island(), Some(1));
        assert_eq!(ship.pos, Vec2::new(10.0, 0.0));
    }

    #[test]
    fn effective_tuning_applies_gene_and_clamps() {
        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        ship.strategy_genes.confidence_decay_scale = 0.01;
        let base = PlanningTuning {
            global_friction_mult: 10.0,
            info_decay_rate: 0.001,
            market_spread: 2.0,
        };

        let tuned = ship.effective_tuning(&base);

        assert_eq!(tuned.global_friction_mult, 6.0);
        assert_eq!(tuned.market_spread, 0.80);
        assert_eq!(tuned.info_decay_rate, 0.0001);
    }

    #[test]
    fn bankruptcy_uses_service_debt_floor() {
        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        ship.cash = -10.0;
        ship.labor_debt = 8.0;
        ship.wear_debt = 4.0;

        assert!(ship.is_bankrupt());
    }

    #[test]
    fn load_utility_floor_gets_more_permissive_with_idle_time() {
        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        assert_eq!(ship.load_utility_floor(), 0.0);
        ship.dock_idle_ticks = 20;
        assert!(ship.load_utility_floor() < -0.25);
        ship.dock_idle_ticks = 1000;
        assert_eq!(ship.load_utility_floor(), -MAX_DOCK_IDLE_RISK_ALLOWANCE);
    }

    #[test]
    fn trade_load_if_empty_uses_full_available_capacity() {
        let mut rng = StdRng::seed_from_u64(77);
        let (mut island, _ledger) = IslandEconomy::new(0, 2, &mut rng);
        island.inventory = [500.0, 0.0, 0.0, 0.0, 0.0];
        island.local_prices = [10.0, 1000.0, 1000.0, 1000.0, 1000.0];
        island.cash = 10_000.0;

        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        ship.cash = 10_000.0;
        ship.archetype = ShipArchetype::Clipper;
        ship.recompute_operational_traits();
        ship.max_cargo_volume = 60.0;
        ship.ledger[1].prices[Commodity::Grain.idx()] = 400.0;
        ship.ledger[1].inventories[Commodity::Grain.idx()] = 0.0;
        ship.ledger[1].cash = 0.0;
        ship.ledger[1].tick_updated = 100;
        ship.ledger[1].last_seen_tick = 100;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let tuning = PlanningTuning::default();
        let departures = [0.0_f32, 0.0_f32];
        let ctx = LoadPlanningContext {
            current_island_id: 0,
            island_positions: &positions,
            current_tick: 100,
            tuning: &tuning,
            outbound_recent_departures: &departures,
        };

        let action = ship.trade_load_if_empty(&mut island, None, &ctx);

        assert_eq!(action, DockAction::Bought);
        assert!(ship.current_cargo().map_or(0.0, |(_, a)| a) > TRADE_ACTION_VOLUME);
    }

    #[test]
    fn trade_load_if_empty_loads_least_worst_finite_lane() {
        let mut rng = StdRng::seed_from_u64(91);
        let (mut island, _ledger) = IslandEconomy::new(0, 2, &mut rng);
        island.inventory = [300.0, 0.0, 0.0, 0.0, 0.0];
        island.local_prices = [90.0, 1000.0, 1000.0, 1000.0, 1000.0];

        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        ship.cash = 10_000.0;
        ship.archetype = ShipArchetype::Clipper;
        ship.recompute_operational_traits();
        ship.max_cargo_volume = 20.0;
        ship.ledger[1].prices[Commodity::Grain.idx()] = 110.0;
        ship.ledger[1].inventories[Commodity::Grain.idx()] = 0.0;
        ship.ledger[1].cash = 100_000.0;
        ship.ledger[1].tick_updated = 200;
        ship.ledger[1].last_seen_tick = 200;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let tuning = PlanningTuning::default();
        let departures = [0.0_f32, 0.0_f32];
        let ctx = LoadPlanningContext {
            current_island_id: 0,
            island_positions: &positions,
            current_tick: 200,
            tuning: &tuning,
            outbound_recent_departures: &departures,
        };

        let action = ship.trade_load_if_empty(&mut island, None, &ctx);

        assert_eq!(action, DockAction::Bought);
        assert!(ship.current_cargo().map_or(0.0, |(_, a)| a) > 0.0);
    }

    #[test]
    fn plan_next_island_uses_planned_target_for_loaded_ship() {
        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 3, 0);
        ship.cargo = Some((Commodity::Grain, 5.0));
        ship.purchase_price = 100.0;
        ship.planned_target_after_load = Some(2);
        let positions = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(20.0, 0.0),
        ];
        let tuning = PlanningTuning::default();
        let departures = [0.0_f32, 0.0_f32, 0.0_f32];

        let target = ship.plan_next_island(0, &positions, 200, &tuning, &departures);

        assert_eq!(target, Some(2));
    }

    #[test]
    fn docking_cycle_sell_merge_buy_depart() {
        // Integration test: create 2 islands and 1 ship, run a manual docking cycle.
        let mut rng = StdRng::seed_from_u64(55);
        let (mut island0, ledger0) = IslandEconomy::new(0, 2, &mut rng);
        let (mut island1, _ledger1) = IslandEconomy::new(1, 2, &mut rng);

        // Setup: island0 has plenty of grain, island1 wants grain (high price).
        island0.inventory[Commodity::Grain.idx()] = 500.0;
        island0.local_prices[Commodity::Grain.idx()] = 10.0;
        island0.cash = 5000.0;
        island1.inventory[Commodity::Grain.idx()] = 5.0;
        island1.local_prices[Commodity::Grain.idx()] = 200.0;
        // Give island1 some timber to sell to the ship.
        island1.inventory[Commodity::Timber.idx()] = 100.0;
        island1.local_prices[Commodity::Timber.idx()] = 30.0;
        island1.cash = 5000.0;

        // Ship starts docked at island1 with grain cargo.
        let mut ship = ShipState::new(Vec2::new(10.0, 0.0), 300.0, 2, 1);
        ship.recompute_operational_traits();
        ship.cash = 1000.0;
        ship.cargo = Some((Commodity::Grain, 15.0));
        ship.purchase_price = 10.0;
        ship.docked_at = Some(1);

        // Update ledger with island1's info so ship knows about it.
        ship.ledger[0].prices[Commodity::Grain.idx()] = 10.0;
        ship.ledger[0].prices[Commodity::Timber.idx()] = 200.0;
        ship.ledger[0].inventories = island0.inventory;
        ship.ledger[0].cash = island0.cash;
        ship.ledger[0].tick_updated = 100;
        ship.ledger[1].prices = island1.local_prices;
        ship.ledger[1].inventories = island1.inventory;
        ship.ledger[1].cash = island1.cash;
        ship.ledger[1].tick_updated = 100;

        let tuning = PlanningTuning::default();
        let ship_tuning = ship.effective_tuning(&tuning);

        // Step 1 & 4: sell cargo at island1 AND buy next cargo.
        ship.begin_dock_tick();
        let _settled = ship.trade_settle_until_stuck(1, &mut island1, &ship_tuning, 3);
        assert!(ship.has_no_cargo(), "Ship should have sold its cargo");
        let cash_after_sell = ship.current_cash();
        assert!(
            cash_after_sell > 1000.0,
            "Ship should have earned cash from selling"
        );

        // Step 2: settle service debt.
        let _ = ship.settle_service_debt(&mut island1);

        // Step 3: ledger merge — ship contributes to island ledger.
        let mut island_ledger_buf = ledger0.clone();
        ship.contribute_ledger_to_island_buffer(1, &mut island_ledger_buf);
        // Island ledger should now have data from the ship's knowledge.
        assert!(island_ledger_buf[0].tick_updated > 0);

        // Step 4: buy cargo for next leg (now possible in same tick/cycle after Sell).
        let departures = [0.0_f32, 0.0_f32];
        let positions = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let ctx = LoadPlanningContext {
            current_island_id: 1,
            island_positions: &positions,
            current_tick: 100,
            tuning: &ship_tuning,
            outbound_recent_departures: &departures,
        };
        let action = ship.trade_load_if_empty(&mut island1, ship.just_sold_resource(), &ctx);
        assert_eq!(action, DockAction::Bought);
        assert!(!ship.has_no_cargo(), "Ship should have bought new cargo");

        // Step 5: plan departure — sync merged ledger and verify no panic.
        ship.sync_ledger_from_snapshot(&island_ledger_buf);
        let _target = ship.plan_next_island(1, &positions, 100, &ship_tuning, &departures);
        // Planning may return None if no profitable route exists with only 2 islands,
        // but the important thing is the full cycle ran without panic.
    }

    #[test]
    fn daughter_ship_spawns_with_mutated_traits() {
        let mut rng = StdRng::seed_from_u64(99);
        let mut parent = ShipState::new(Vec2::new(100.0, 200.0), 400.0, 3, 0);
        parent.cash = 10_000.0;

        let daughter = parent
            .spawn_daughter(0.10, &mut rng)
            .expect("Wealthy parent should spawn a daughter");

        // Daughter should be near parent position.
        assert!(daughter.pos().distance(parent.pos()) < 20.0);
        // Daughter should have some cash.
        assert!(daughter.current_cash() > 0.0);
        // Daughter should have a ledger of the same size.
        assert_eq!(daughter.ledger.len(), parent.ledger.len());
    }

    #[test]
    fn scuttle_threshold_behavior() {
        let ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        // A fresh ship with STARTING_CASH should be above scuttle threshold.
        assert!(ship.estimated_net_worth() > STARTING_CASH * 0.35);

        let mut broke_ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        broke_ship.cash = 10.0;
        broke_ship.labor_debt = 100.0;
        broke_ship.wear_debt = 100.0;
        // Ship with high debts should have low net worth.
        assert!(broke_ship.estimated_net_worth() < STARTING_CASH * 0.35);
    }

    #[test]
    fn stress_1000_ticks_no_nan_or_panic() {
        // Run 1000 ticks of island economy to verify no NaN/infinity.
        let mut rng = StdRng::seed_from_u64(42);
        let num_islands = 5;
        let mut economies: Vec<(IslandEconomy, Vec<PriceEntry>)> = (0..num_islands)
            .map(|id| IslandEconomy::new(id, num_islands, &mut rng))
            .collect();

        for tick in 0..1000u64 {
            for (economy, ledger) in &mut economies {
                economy.produce_consume_and_price(1.0, tick, ledger);
            }
        }

        for (economy, _) in &economies {
            for &inv in &economy.inventory {
                assert!(inv.is_finite(), "Inventory became non-finite");
                assert!(inv >= 0.0, "Inventory went negative");
            }
            for &price in &economy.local_prices {
                assert!(price.is_finite(), "Price became non-finite");
                assert!(price > 0.0, "Price became non-positive");
            }
            assert!(economy.population.is_finite(), "Population non-finite");
            assert!(economy.population >= 0.0, "Population negative");
            assert!(economy.cash.is_finite(), "Cash non-finite");
        }
    }
}
