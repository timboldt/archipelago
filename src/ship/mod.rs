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

const TRADE_ACTION_VOLUME: f32 = 18.0;
pub const STARTING_CASH: f32 = 200.0;
const DEFAULT_MARKET_SPREAD: f32 = 0.10;
const ROUTE_LEARNING_RATE: f32 = 0.20;
const ROUTE_LEARNING_DECAY: f32 = 0.98;
const BASE_CARGO_VOLUME_CAPACITY: f32 = 22.0;
const BASE_LABOR_RATE: f32 = 0.0004;
const BASE_WEAR_RATE: f32 = 0.00012;
#[allow(dead_code)]
const DOCK_IDLE_RISK_RAMP_PER_TICK: f32 = 0.015;
#[allow(dead_code)]
const MAX_DOCK_IDLE_RISK_ALLOWANCE: f32 = 1.5;

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

pub const BASE_LABOR_RATE_PUB: f32 = BASE_LABOR_RATE;
pub const BASE_WEAR_RATE_PUB: f32 = BASE_WEAR_RATE;

const MIN_SHIP_SPEED: f32 = 120.0;
const MAX_SHIP_SPEED: f32 = 600.0;
#[allow(dead_code)]
const DOCKED_PORT_FEE_MULTIPLIER: f32 = 1.5;
pub(crate) const HEAVY_LOAD_WEAR_MULTIPLIER: f32 = 1.1;
const BANKRUPTCY_CASH_FLOOR: f32 = -20.0;
const BASE_DOCKING_TAX_RATE: f32 = 0.0015;
const MAX_DOCKING_TAX_RATE: f32 = 0.02;
const LIQUIDITY_IMBALANCE_TAX_SLOPE: f32 = 0.01;
const DOCKING_TAX_CASH_RESERVE_MULTIPLIER: f32 = 0.75;
const IDLE_PORT_FEE_PER_TICK: f32 = STARTING_CASH * 0.006;
const IDLE_FEE_GRACE_TICKS: u32 = 100;
const MIN_EFFICIENCY_RATING: f32 = 0.80;
const MAX_EFFICIENCY_RATING: f32 = 1.30;
const MIN_GENE_SCALE: f32 = 0.80;
const MAX_GENE_SCALE: f32 = 1.20;

/// Unified ship state used by simulation methods. In the ECS, this is
/// decomposed into ShipMovement, ShipTrading, ShipProfile, and ShipLedger
/// components. This struct reassembles them for method calls.
pub struct ShipState {
    pos: Vec2,
    target: Vec2,
    speed: f32,
    base_speed: f32,
    archetype: ShipArchetype,
    efficiency_rating: f32,
    max_cargo_volume: f32,
    target_island_id: Option<usize>,
    docked_at: Option<usize>,
    last_docked_island_id: Option<usize>,
    cargo: Option<(Commodity, f32)>,
    cash: f32,
    labor_debt: f32,
    wear_debt: f32,
    ledger: PriceLedger,
    route_memory: Vec<f32>,
    purchase_price: f32,
    strategy_genes: StrategyGenes,
    planned_target_after_load: Option<usize>,
    cargo_changed_this_dock: bool,
    last_step_distance: f32,
    just_sold_resource: Option<Commodity>,
    last_dock_action: DockAction,
    dock_idle_ticks: u32,
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
        }
        .with_recomputed_traits()
    }

    fn with_recomputed_traits(mut self) -> Self {
        self.recompute_operational_traits();
        self
    }

    fn recompute_operational_traits(&mut self) {
        let archetype = self.archetype;
        let (speed_mult, capacity_mult, _, _) = Self::profile_multipliers(archetype);

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

    fn profile_multipliers(archetype: ShipArchetype) -> (f32, f32, f32, f32) {
        Self::profile_multipliers_static(archetype)
    }

    pub fn profile_multipliers_static(archetype: ShipArchetype) -> (f32, f32, f32, f32) {
        match archetype {
            ShipArchetype::Clipper => (1.50, 0.75, 1.50, 1.35),
            ShipArchetype::Shorthaul => (1.00, 1.00, 0.75, 0.80),
            ShipArchetype::Freighter => (0.75, 2.00, 1.00, 1.10),
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
    pub fn docked_island(&self) -> Option<usize> {
        self.docked_at
    }
    pub fn last_docked_island(&self) -> Option<usize> {
        self.docked_at.or(self.last_docked_island_id)
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
        let (_, _, _, wear_mult) = Self::profile_multipliers(self.archetype);
        let efficiency_factor = (1.20 - 0.40 * self.efficiency_rating).clamp(0.65, 1.15);
        BASE_WEAR_RATE * wear_mult * efficiency_factor
    }

    pub fn labor_rate(&self) -> f32 {
        let (_, _, labor_mult, _) = Self::profile_multipliers(self.archetype);
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
    ) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }

        let Some((resource, carrying_amount)) = self.cargo else {
            return self.last_dock_action;
        };
        if carrying_amount <= 0.0 {
            return self.last_dock_action;
        }
        let bid_price = island.bid_price(resource, tuning.market_spread);
        if !bid_price.is_finite() || bid_price <= 0.0 {
            return self.last_dock_action;
        }

        let (sold_amount, gross_revenue) =
            island.sell_to_island(resource, carrying_amount, tuning.market_spread);
        if sold_amount <= 0.0 || gross_revenue <= 0.0 {
            return self.last_dock_action;
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
        self.last_dock_action
    }

    pub fn trade_settle_until_stuck(
        &mut self,
        current_island_id: usize,
        island: &mut IslandEconomy,
        tuning: &PlanningTuning,
        max_steps: usize,
    ) -> bool {
        if self.last_dock_action != DockAction::None {
            return false;
        }

        let mut settled_any = false;
        for _ in 0..max_steps.max(1) {
            if self.has_no_cargo() {
                break;
            }

            self.last_dock_action = DockAction::None;
            match self.trade_unload_if_carrying(current_island_id, island, tuning) {
                DockAction::Sold => {
                    settled_any = true;
                }
                DockAction::None | DockAction::Bought => break,
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
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }

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

        if !best_utility.is_finite() {
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

    pub fn sync_ledger_from_snapshot(&mut self, island_ledger_snapshot: &PriceLedger) {
        let len = self.ledger.len().min(island_ledger_snapshot.len());
        for (i, ship_entry) in self.ledger.iter_mut().enumerate().take(len) {
            if island_ledger_snapshot[i].tick_updated >= ship_entry.tick_updated {
                ship_entry.prices = island_ledger_snapshot[i].prices;
                ship_entry.inventories = island_ledger_snapshot[i].inventories;
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
        island.local_prices = [100.0, 1000.0, 1000.0, 1000.0, 1000.0];

        let mut ship = ShipState::new(Vec2::new(0.0, 0.0), 300.0, 2, 0);
        ship.cash = 10_000.0;
        ship.archetype = ShipArchetype::Clipper;
        ship.recompute_operational_traits();
        ship.max_cargo_volume = 20.0;
        ship.ledger[1].prices[Commodity::Grain.idx()] = 90.0;
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
}
