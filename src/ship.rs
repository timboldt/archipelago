use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;

use crate::island::{
    Inventory, Island, PriceEntry, PriceLedger, Resource, ASK_PRICE_MULTIPLIER, BASE_COSTS,
    BID_PRICE_MULTIPLIER, INVENTORY_CARRYING_CAPACITY, RESOURCE_COUNT,
};

const TRADE_ACTION_VOLUME: f32 = 18.0;
pub const STARTING_CASH: f32 = 200.0;
const UNKNOWN_CASH_CONFIDENCE_SCALE: f32 = 0.70;
const DEFAULT_MARKET_DEPTH_FALLBACK: f32 = 600.0;
const RECENT_BROKE_TICKS: f32 = 180.0;
const BROKE_ISLAND_UTILITY_PENALTY: f32 = 5.5;
const BROKE_CASH_COVERAGE_RATIO: f32 = 0.35;
const INDUSTRIAL_INFRA_THRESHOLD: f32 = 1.5;
const INDUSTRIAL_INPUT_BONUS_PER_INFRA: f32 = 4.0;
const INDUSTRIAL_INPUT_BONUS_CAP: f32 = 14.0;
const DEFAULT_CAPITAL_CARRY_COST_PER_TIME: f32 = 0.0020;
const HIGH_PRICE_RISK_WEIGHT: f32 = 0.65;
const BASE_CARGO_VOLUME_CAPACITY: f32 = 22.0;
const BASE_FUEL_BURN_RATE: f32 = 1.0;
const BASE_MAINTENANCE_RATE: f32 = 0.002;
const RUNNER_SPEED_MULTIPLIER: f32 = 1.50;
const COASTER_SPEED_MULTIPLIER: f32 = 1.00;
const FREIGHTER_SPEED_MULTIPLIER: f32 = 0.75;
const RUNNER_CAPACITY_MULTIPLIER: f32 = 0.75;
const COASTER_CAPACITY_MULTIPLIER: f32 = 1.00;
const FREIGHTER_CAPACITY_MULTIPLIER: f32 = 2.00;
const RUNNER_MAINT_MULTIPLIER: f32 = 1.50;
const COASTER_MAINT_MULTIPLIER: f32 = 0.75;
const FREIGHTER_MAINT_MULTIPLIER: f32 = 1.00;
const RUNNER_FUEL_MULTIPLIER: f32 = 1.35;
const COASTER_FUEL_MULTIPLIER: f32 = 0.80;
const FREIGHTER_FUEL_MULTIPLIER: f32 = 1.10;

#[derive(Clone, Copy, Debug)]
pub struct PlanningTuning {
    pub confidence_decay_k: f32,
    pub speculation_floor: f32,
    pub speculation_staleness_scale: f32,
    pub speculation_uncertainty_bonus: f32,
    pub learning_rate: f32,
    pub learning_decay: f32,
    pub learning_weight: f32,
    pub transport_cost_per_distance: f32,
    pub cost_per_mile_factor: f32,
    pub capital_carry_cost_per_time: f32,
    pub island_neglect_bonus_per_tick: f32,
    pub island_neglect_bonus_cap: f32,
}

pub struct LoadPlanningContext<'a> {
    pub current_island_id: usize,
    pub island_positions: &'a [Vec2],
    pub current_tick: u64,
    pub tuning: &'a PlanningTuning,
    pub outbound_recent_departures: &'a [f32],
}

struct UtilityContext<'a> {
    island_positions: &'a [Vec2],
    current_tick: u64,
    tuning: &'a PlanningTuning,
    outbound_recent_departures: &'a [f32],
}

impl Default for PlanningTuning {
    fn default() -> Self {
        Self {
            confidence_decay_k: 0.003,
            speculation_floor: 0.08,
            speculation_staleness_scale: 0.35,
            speculation_uncertainty_bonus: 8.0,
            learning_rate: 0.14,
            learning_decay: 0.98,
            learning_weight: 12.0,
            transport_cost_per_distance: 0.0006,
            cost_per_mile_factor: 1.0,
            capital_carry_cost_per_time: DEFAULT_CAPITAL_CARRY_COST_PER_TIME,
            island_neglect_bonus_per_tick: 0.006,
            island_neglect_bonus_cap: 18.0,
        }
    }
}

const MIN_SHIP_SPEED: f32 = 120.0;
const MAX_SHIP_SPEED: f32 = 600.0;
const TRAVEL_CASH_BURN_PER_DISTANCE: f32 = 0.00022;
const BANKRUPTCY_CASH_FLOOR: f32 = -20.0;
const MIN_HULL_SIZE: f32 = 0.75;
const MAX_HULL_SIZE: f32 = 1.60;
const MIN_EFFICIENCY_RATING: f32 = 0.80;
const MAX_EFFICIENCY_RATING: f32 = 1.30;
const MIN_GENE_SCALE: f32 = 0.80;
const MAX_GENE_SCALE: f32 = 1.20;

#[derive(Clone, Copy, Debug)]
pub struct StrategyGenes {
    confidence_decay_scale: f32,
    speculation_floor_scale: f32,
    learning_weight_scale: f32,
    transport_cost_scale: f32,
}

impl Default for StrategyGenes {
    fn default() -> Self {
        Self {
            confidence_decay_scale: 1.0,
            speculation_floor_scale: 1.0,
            learning_weight_scale: 1.0,
            transport_cost_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockAction {
    None,
    Sold,
    Bought,
    Bartered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipArchetype {
    Runner,
    Freighter,
    Coaster,
}

pub struct Ship {
    pub pos: Vec2,
    target: Vec2,
    speed: f32,
    base_speed: f32,
    hull_size: f32,
    efficiency_rating: f32,
    max_cargo_volume: f32,
    fuel_burn_rate: f32,
    maintenance_rate: f32,
    target_island_id: Option<usize>,
    docked_at: Option<usize>,
    cargo: Inventory,
    pub cash: f32,
    pub ledger: PriceLedger,
    route_memory: Vec<f32>,
    purchase_price_by_resource: [f32; RESOURCE_COUNT],
    cargo_distance_accrued: f32,
    strategy_genes: StrategyGenes,
    planned_target_after_load: Option<usize>,
    just_sold_resource: Option<Resource>,
    last_dock_action: DockAction,
}

impl Ship {
    pub fn new(pos: Vec2, speed: f32, num_islands: usize, docked_island_id: usize) -> Self {
        let mut rng = ::rand::thread_rng();
        let hull_size = rng.gen_range(MIN_HULL_SIZE..MAX_HULL_SIZE);
        let efficiency_rating = rng.gen_range(MIN_EFFICIENCY_RATING..MAX_EFFICIENCY_RATING);
        Self {
            pos,
            target: pos,
            speed,
            base_speed: speed,
            hull_size,
            efficiency_rating,
            max_cargo_volume: 0.0,
            fuel_burn_rate: 0.0,
            maintenance_rate: 0.0,
            target_island_id: Some(docked_island_id),
            docked_at: Some(docked_island_id),
            cargo: [0.0; RESOURCE_COUNT],
            cash: STARTING_CASH,
            ledger: vec![
                PriceEntry {
                    prices: [0.0; RESOURCE_COUNT],
                    inventories: [0.0; RESOURCE_COUNT],
                    cash: 0.0,
                    infrastructure_level: 0.0,
                    tick_updated: 0,
                    last_seen_tick: 0,
                };
                num_islands
            ],
            route_memory: vec![0.0; num_islands],
            purchase_price_by_resource: [0.0; RESOURCE_COUNT],
            cargo_distance_accrued: 0.0,
            strategy_genes: StrategyGenes::default(),
            planned_target_after_load: None,
            just_sold_resource: None,
            last_dock_action: DockAction::None,
        }
        .with_recomputed_traits()
    }

    fn with_recomputed_traits(mut self) -> Self {
        self.recompute_operational_traits();
        self
    }

    fn recompute_operational_traits(&mut self) {
        let archetype = self.profile_archetype_from_hull();
        let (speed_mult, capacity_mult, maintenance_mult, fuel_mult) =
            Self::profile_multipliers(archetype);

        let efficiency_speed_factor = (0.92 + 0.30 * (self.efficiency_rating - 1.0)).clamp(0.85, 1.10);
        self.speed = (self.base_speed * speed_mult * efficiency_speed_factor)
            .clamp(MIN_SHIP_SPEED, MAX_SHIP_SPEED);

        let efficiency_capacity_factor = (0.95 + 0.10 * (self.efficiency_rating - 1.0)).clamp(0.90, 1.05);
        self.max_cargo_volume = (BASE_CARGO_VOLUME_CAPACITY
            * capacity_mult
            * efficiency_capacity_factor)
            .clamp(8.0, 80.0);

        let efficiency_fuel_factor = (1.20 - 0.40 * self.efficiency_rating).clamp(0.65, 1.15);
        self.fuel_burn_rate = BASE_FUEL_BURN_RATE * fuel_mult * efficiency_fuel_factor;

        let efficiency_maint_factor = (1.20 - 0.35 * self.efficiency_rating).clamp(0.70, 1.15);
        self.maintenance_rate = BASE_MAINTENANCE_RATE * maintenance_mult * efficiency_maint_factor;
    }

    fn profile_archetype_from_hull(&self) -> ShipArchetype {
        let hull_position = ((self.hull_size - MIN_HULL_SIZE) / (MAX_HULL_SIZE - MIN_HULL_SIZE))
            .clamp(0.0, 1.0);
        if hull_position < (1.0 / 3.0) {
            ShipArchetype::Runner
        } else if hull_position > (2.0 / 3.0) {
            ShipArchetype::Freighter
        } else {
            ShipArchetype::Coaster
        }
    }

    fn profile_multipliers(archetype: ShipArchetype) -> (f32, f32, f32, f32) {
        match archetype {
            ShipArchetype::Runner => (
                RUNNER_SPEED_MULTIPLIER,
                RUNNER_CAPACITY_MULTIPLIER,
                RUNNER_MAINT_MULTIPLIER,
                RUNNER_FUEL_MULTIPLIER,
            ),
            ShipArchetype::Coaster => (
                COASTER_SPEED_MULTIPLIER,
                COASTER_CAPACITY_MULTIPLIER,
                COASTER_MAINT_MULTIPLIER,
                COASTER_FUEL_MULTIPLIER,
            ),
            ShipArchetype::Freighter => (
                FREIGHTER_SPEED_MULTIPLIER,
                FREIGHTER_CAPACITY_MULTIPLIER,
                FREIGHTER_MAINT_MULTIPLIER,
                FREIGHTER_FUEL_MULTIPLIER,
            ),
        }
    }

    pub fn effective_tuning(&self, base: &PlanningTuning) -> PlanningTuning {
        let mut tuned = *base;
        tuned.confidence_decay_k =
            (tuned.confidence_decay_k * self.strategy_genes.confidence_decay_scale).max(0.0001);
        tuned.speculation_floor = (tuned.speculation_floor
            * self.strategy_genes.speculation_floor_scale)
            .clamp(0.01, 0.95);
        tuned.learning_weight =
            (tuned.learning_weight * self.strategy_genes.learning_weight_scale).max(0.0);
        tuned.transport_cost_per_distance =
            (tuned.transport_cost_per_distance * self.strategy_genes.transport_cost_scale).max(0.0);
        tuned.cost_per_mile_factor = tuned.cost_per_mile_factor.clamp(0.2, 5.0);
        tuned.capital_carry_cost_per_time = tuned.capital_carry_cost_per_time.max(0.0);
        tuned
    }

    pub fn spawn_daughter(&mut self, mutation_strength: f32, rng: &mut impl Rng) -> Option<Ship> {
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

        let mut daughter = Ship::new(self.pos, daughter_base_speed, num_islands, spawn_island_id);
        daughter.cash = endowment;
        daughter.ledger = self.ledger.clone();
        daughter.route_memory = self.route_memory.clone();
        if self.docked_island().is_none() {
            if let Some(target_id) = self.target_island_id {
                daughter.set_target(target_id.min(num_islands - 1), self.target);
            }
        }
        daughter.hull_size = mutate_gene_gaussian(
            self.hull_size,
            mutation_strength,
            rng,
            MIN_HULL_SIZE,
            MAX_HULL_SIZE,
        );
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
            speculation_floor_scale: mutate_gene_gaussian(
                self.strategy_genes.speculation_floor_scale,
                mutation_strength,
                rng,
                MIN_GENE_SCALE,
                MAX_GENE_SCALE,
            ),
            learning_weight_scale: mutate_gene_gaussian(
                self.strategy_genes.learning_weight_scale,
                mutation_strength,
                rng,
                MIN_GENE_SCALE,
                MAX_GENE_SCALE,
            ),
            transport_cost_scale: mutate_gene_gaussian(
                self.strategy_genes.transport_cost_scale,
                mutation_strength,
                rng,
                MIN_GENE_SCALE,
                MAX_GENE_SCALE,
            ),
        };

        Some(daughter)
    }

    pub fn set_target(&mut self, target_island_id: usize, target: Vec2) {
        self.target = target;
        self.target_island_id = Some(target_island_id);
        self.docked_at = None;
        self.planned_target_after_load = None;
    }

    pub fn docked_island(&self) -> Option<usize> {
        self.docked_at
    }

    pub fn estimated_net_worth(&self) -> f32 {
        let mut net_worth = self.cash.max(0.0);
        for resource in Resource::iter() {
            let idx = resource.idx();
            let amount = self.cargo[idx].max(0.0);
            if amount <= 0.0 {
                continue;
            }
            let cargo_book_price = if self.purchase_price_by_resource[idx] > 0.0 {
                self.purchase_price_by_resource[idx]
            } else {
                self.median_price_for_resource(resource)
            };
            let conservative_cargo_value =
                (cargo_book_price * BID_PRICE_MULTIPLIER * amount).max(0.0);
            net_worth += conservative_cargo_value;
        }
        net_worth
    }

    pub fn apply_maintenance(&mut self, dt: f32) {
        self.cash -= self.maintenance_rate * dt.max(0.0);
    }

    pub fn is_bankrupt(&self) -> bool {
        self.cash < BANKRUPTCY_CASH_FLOOR
    }

    pub fn archetype(&self) -> ShipArchetype {
        self.profile_archetype_from_hull()
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn max_cargo_volume(&self) -> f32 {
        self.max_cargo_volume
    }

    pub fn cargo_volume_used(&self) -> f32 {
        self.total_cargo_volume()
    }

    pub fn fuel_burn_rate(&self) -> f32 {
        self.fuel_burn_rate
    }

    pub fn maintenance_rate(&self) -> f32 {
        self.maintenance_rate
    }

    pub fn target_island(&self) -> Option<usize> {
        self.target_island_id
    }

    pub fn dominant_cargo_by_value(&self) -> Option<(Resource, f32)> {
        let resource = self.dominant_cargo_resource_by_value()?;
        let idx = resource.idx();
        let amount = self.cargo[idx].max(0.0);
        if amount <= 0.0 {
            return None;
        }
        let unit_price = if self.purchase_price_by_resource[idx] > 0.0 {
            self.purchase_price_by_resource[idx]
        } else {
            self.median_price_for_resource(resource)
        }
        .max(0.0);
        Some((resource, amount * unit_price))
    }

    pub fn has_no_cargo(&self) -> bool {
        self.cargo.iter().all(|amount| *amount <= 0.0)
    }

    fn total_cargo_volume(&self) -> f32 {
        let mut volume = 0.0;
        for resource in Resource::iter() {
            volume += self.cargo[resource.idx()].max(0.0) * resource.volume_per_unit();
        }
        volume
    }

    fn remaining_cargo_volume(&self) -> f32 {
        (self.max_cargo_volume - self.total_cargo_volume()).max(0.0)
    }

    fn max_units_for_trade_action(&self, resource: Resource) -> f32 {
        TRADE_ACTION_VOLUME / resource.volume_per_unit().max(0.01)
    }

    fn dominant_cargo_resource(&self) -> Option<Resource> {
        let mut best_resource = None;
        let mut best_amount = 0.0;
        for resource in Resource::iter() {
            let amount = self.cargo[resource.idx()].max(0.0);
            if amount > best_amount {
                best_amount = amount;
                best_resource = Some(resource);
            }
        }
        best_resource
    }

    fn dominant_cargo_resource_by_value(&self) -> Option<Resource> {
        let mut best_resource = None;
        let mut best_value = 0.0;
        for resource in Resource::iter() {
            let idx = resource.idx();
            let amount = self.cargo[idx].max(0.0);
            if amount <= 0.0 {
                continue;
            }
            let unit_price = if self.purchase_price_by_resource[idx] > 0.0 {
                self.purchase_price_by_resource[idx]
            } else {
                self.median_price_for_resource(resource)
            }
            .max(0.0);
            let value = amount * unit_price;
            if value > best_value {
                best_value = value;
                best_resource = Some(resource);
            }
        }
        best_resource
    }

    pub fn just_sold_resource(&self) -> Option<Resource> {
        self.just_sold_resource
    }

    pub fn begin_dock_tick(&mut self, tuning: &PlanningTuning) {
        self.last_dock_action = DockAction::None;
        self.planned_target_after_load = None;
        self.just_sold_resource = None;
        let decay = tuning.learning_decay.clamp(0.0, 1.0);
        for score in &mut self.route_memory {
            *score *= decay;
        }
    }

    pub fn trade_unload_if_carrying(
        &mut self,
        island_id: usize,
        island: &mut Island,
        tuning: &PlanningTuning,
    ) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }

        let Some(resource) = self.dominant_cargo_resource() else {
            return self.last_dock_action;
        };
        let resource_idx = resource.idx();
        let carrying_amount = self.cargo[resource_idx].max(0.0);
        if carrying_amount <= 0.0 {
            return self.last_dock_action;
        }

        let requested_amount = carrying_amount.min(self.max_units_for_trade_action(resource));
        if requested_amount <= 0.0 {
            return self.last_dock_action;
        }

        let (sold_amount, gross_revenue) = island.sell_to_island(resource, requested_amount);
        if sold_amount <= 0.0 || gross_revenue <= 0.0 {
            return self.last_dock_action;
        }

        let sold_volume = sold_amount * resource.volume_per_unit();
        let raw_freight_cost = self.cargo_distance_accrued
            * sold_volume
            * tuning.transport_cost_per_distance
            * tuning.cost_per_mile_factor
            * self.fuel_burn_rate;
        let net_revenue = gross_revenue - raw_freight_cost;
        self.cash += net_revenue;

        let book_price = self.purchase_price_by_resource[resource_idx];
        if book_price > 0.0 && sold_amount > 0.0 && island_id < self.route_memory.len() {
            let sale_unit_price = net_revenue / sold_amount;
            let normalized_margin = (sale_unit_price - book_price) / (book_price + 1.0);
            self.route_memory[island_id] += normalized_margin * tuning.learning_rate;
            self.route_memory[island_id] = self.route_memory[island_id].clamp(-1.5, 1.5);
        }

        self.cargo[resource_idx] = (self.cargo[resource_idx] - sold_amount).max(0.0);
        if self.cargo[resource_idx] <= 0.0 {
            self.purchase_price_by_resource[resource_idx] = 0.0;
        }
        if self.has_no_cargo() {
            self.cargo_distance_accrued = 0.0;
        }

        self.just_sold_resource = Some(resource);
        self.last_dock_action = DockAction::Sold;
        self.last_dock_action
    }

    pub fn trade_barter_if_carrying(
        &mut self,
        current_island_id: usize,
        island: &mut Island,
        context: &LoadPlanningContext<'_>,
    ) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }

        if self.has_no_cargo() {
            return self.last_dock_action;
        }

        let utility_context = UtilityContext {
            island_positions: context.island_positions,
            current_tick: context.current_tick,
            tuning: context.tuning,
            outbound_recent_departures: context.outbound_recent_departures,
        };

        let mut best_choice: Option<(Resource, Resource, usize, f32, f32, f32)> = None;
        let mut best_utility = f32::NEG_INFINITY;

        for source_resource in Resource::iter() {
            let source_idx = source_resource.idx();
            let available_source = self.cargo[source_idx].max(0.0);
            if available_source <= 0.0 {
                continue;
            }

            let source_bid_price = island.bid_price(source_resource);
            if !source_bid_price.is_finite() || source_bid_price <= 0.0 {
                continue;
            }

            let max_source_action = self.max_units_for_trade_action(source_resource);
            let source_amount = available_source.min(max_source_action);
            if source_amount <= 0.0 {
                continue;
            }

            let cargo_value_budget = source_amount * source_bid_price;
            if cargo_value_budget <= 0.0 {
                continue;
            }

            for resource in Resource::iter() {
                if resource == source_resource {
                    continue;
                }

                let ask_price = island.ask_price(resource);
                if !ask_price.is_finite() || ask_price <= 0.0 {
                    continue;
                }

                let available = island.inventory[resource.idx()].max(0.0);
                if available <= 0.0 {
                    continue;
                }

                let max_units_by_value = cargo_value_budget / ask_price;
                let source_volume_per_unit = source_resource.volume_per_unit().max(0.01);
                let target_volume_per_unit = resource.volume_per_unit().max(0.01);
                let source_units_per_target_unit = ask_price / source_bid_price;
                let net_volume_per_target_unit =
                    target_volume_per_unit - source_units_per_target_unit * source_volume_per_unit;
                let max_units_by_volume = if net_volume_per_target_unit > 0.0 {
                    self.remaining_cargo_volume() / net_volume_per_target_unit
                } else {
                    f32::INFINITY
                };
                let amount_for_utility = max_units_by_value
                    .min(max_units_by_volume)
                    .min(available)
                    .min(self.max_units_for_trade_action(resource));
                if amount_for_utility <= 0.0 {
                    continue;
                }

                for target_id in 0..self.ledger.len() {
                    if target_id == current_island_id {
                        continue;
                    }

                    let (utility, _confidence) = self.calculate_utility(
                        resource,
                        target_id,
                        ask_price,
                        amount_for_utility,
                        &utility_context,
                    );

                    if utility > best_utility {
                        best_utility = utility;
                        best_choice = Some((
                            source_resource,
                            resource,
                            target_id,
                            ask_price,
                            amount_for_utility,
                            source_bid_price,
                        ));
                    }
                }
            }
        }

        let Some((source_resource, resource, target_id, ask_price, mut acquired_amount, source_bid_price)) =
            best_choice
        else {
            return self.last_dock_action;
        };

        if best_utility <= 0.0 || acquired_amount <= 0.0 {
            return self.last_dock_action;
        }

        let source_idx = source_resource.idx();
        let target_idx = resource.idx();
        let source_volume_per_unit = source_resource.volume_per_unit().max(0.01);
        let target_volume_per_unit = resource.volume_per_unit().max(0.01);
        let max_source_sell = self.cargo[source_idx]
            .max(0.0)
            .min(self.max_units_for_trade_action(source_resource));
        acquired_amount = acquired_amount
            .min(island.inventory[target_idx].max(0.0))
            .min(self.max_units_for_trade_action(resource))
            .min(max_source_sell * source_bid_price / ask_price);

        let source_units_per_target_unit = ask_price / source_bid_price;
        let net_volume_per_target_unit =
            target_volume_per_unit - source_units_per_target_unit * source_volume_per_unit;
        if net_volume_per_target_unit > 0.0 {
            acquired_amount = acquired_amount
                .min(self.remaining_cargo_volume() / net_volume_per_target_unit);
        }
        if acquired_amount <= 0.0 {
            return self.last_dock_action;
        }

        let source_amount_sold = (acquired_amount * ask_price / source_bid_price)
            .min(max_source_sell)
            .max(0.0);
        if source_amount_sold <= 0.0 {
            return self.last_dock_action;
        }

        let projected_post_volume = self.total_cargo_volume()
            - source_amount_sold * source_volume_per_unit
            + acquired_amount * target_volume_per_unit;
        if projected_post_volume > self.max_cargo_volume + 0.0001 {
            return self.last_dock_action;
        }

        self.cargo[source_idx] = (self.cargo[source_idx] - source_amount_sold).max(0.0);
        island.inventory[source_idx] += source_amount_sold;
        island.inventory[resource.idx()] =
            (island.inventory[resource.idx()] - acquired_amount).max(0.0);

        let existing_target_amount = self.cargo[target_idx].max(0.0);
        let total_target_amount = existing_target_amount + acquired_amount;
        let existing_book = self.purchase_price_by_resource[target_idx].max(0.0);
        self.purchase_price_by_resource[target_idx] = if total_target_amount > 0.0 {
            ((existing_target_amount * existing_book) + (acquired_amount * ask_price))
                / total_target_amount
        } else {
            0.0
        };
        if self.cargo[source_idx] <= 0.0 {
            self.purchase_price_by_resource[source_idx] = 0.0;
        }
        self.cargo[target_idx] += acquired_amount;
        self.planned_target_after_load = Some(target_id);
        self.cargo_distance_accrued = 0.0;
        self.just_sold_resource = Some(source_resource);
        self.last_dock_action = DockAction::Bartered;
        self.last_dock_action
    }

    pub fn trade_load_if_empty(
        &mut self,
        island: &mut Island,
        exclude: Option<Resource>,
        context: &LoadPlanningContext<'_>,
    ) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }

        let remaining_volume = self.remaining_cargo_volume();
        if remaining_volume <= 0.0 {
            return self.last_dock_action;
        }

        // Multi-resource speculation: evaluate (local resource -> destination island) pairs
        // and buy the resource from the highest-utility pair.
        let mut chosen_resource: Option<Resource> = None;
        let mut chosen_target: Option<usize> = None;
        let mut chosen_local_price = 0.0;
        let mut best_utility = f32::NEG_INFINITY;
        let utility_context = UtilityContext {
            island_positions: context.island_positions,
            current_tick: context.current_tick,
            tuning: context.tuning,
            outbound_recent_departures: context.outbound_recent_departures,
        };

        for resource in Resource::iter() {
            if Some(resource) == exclude {
                continue;
            }
            let idx = resource.idx();
            let local_price = island.ask_price(resource);
            if !local_price.is_finite() || local_price <= 0.0 {
                continue;
            }

            let available = island.inventory[idx].max(0.0);
            if available <= 0.0 {
                continue;
            }

            let affordable = (self.cash / local_price).max(0.0);
            let max_units_by_volume = remaining_volume / resource.volume_per_unit().max(0.01);
            let projected_amount = self
                .max_units_for_trade_action(resource)
                .min(max_units_by_volume)
                .min(affordable)
                .min(available);
            if projected_amount <= 0.0 {
                continue;
            }

            for target_id in 0..self.ledger.len() {
                if target_id == context.current_island_id {
                    continue;
                }

                let (utility, _confidence) = self.calculate_utility(
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
            return self.last_dock_action;
        }

        if best_utility <= 0.0 {
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
        let requested = self
            .max_units_for_trade_action(chosen_resource)
            .min(max_units_by_volume)
            .min(affordable);
        if requested <= 0.0 {
            return self.last_dock_action;
        }

        let (filled, total_cost) = island.buy_from_island(chosen_resource, requested);
        if filled <= 0.0 {
            return self.last_dock_action;
        }
        self.cash -= total_cost;
        let idx = chosen_resource.idx();
        let existing_amount = self.cargo[idx].max(0.0);
        let existing_book = self.purchase_price_by_resource[idx].max(0.0);
        let filled_unit_price = total_cost / filled;
        let total_amount = existing_amount + filled;
        self.purchase_price_by_resource[idx] = if total_amount > 0.0 {
            ((existing_amount * existing_book) + (filled * filled_unit_price)) / total_amount
        } else {
            0.0
        };
        self.cargo[idx] += filled;
        self.planned_target_after_load = Some(chosen_target);
        let preexisting_volume = self.total_cargo_volume() - (filled * chosen_resource.volume_per_unit());
        let post_volume = self.total_cargo_volume();
        if post_volume > 0.0 {
            self.cargo_distance_accrued = (self.cargo_distance_accrued * preexisting_volume.max(0.0)) / post_volume;
        }
        self.last_dock_action = DockAction::Bought;
        self.last_dock_action
    }

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

    pub fn plan_next_island(
        &self,
        current_island_id: usize,
        island_positions: &[Vec2],
        current_tick: u64,
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
    ) -> Option<usize> {
        if !self.has_no_cargo() {
            if let Some(preselected_target) = self.planned_target_after_load {
                if preselected_target != current_island_id && preselected_target < self.ledger.len()
                {
                    return Some(preselected_target);
                }
            }

            let mut best_target = None;
            let mut best_utility = f32::NEG_INFINITY;
            let mut best_confidence = 0.0;
            let mut candidates: Vec<(usize, f32, f32)> = Vec::new();
            let utility_context = UtilityContext {
                island_positions,
                current_tick,
                tuning,
                outbound_recent_departures,
            };

            for target_id in 0..self.ledger.len() {
                if target_id == current_island_id {
                    continue;
                }

                let mut resource_best_utility = f32::NEG_INFINITY;
                let mut resource_best_confidence = 0.0;
                for resource in Resource::iter() {
                    let idx = resource.idx();
                    let lot_size = self.cargo[idx].max(0.0);
                    if lot_size <= 0.0 {
                        continue;
                    }

                    let fallback_buy_price = self.median_price_for_resource(resource);
                    let reference_buy_price = if self.purchase_price_by_resource[idx] > 0.0 {
                        self.purchase_price_by_resource[idx]
                    } else {
                        fallback_buy_price
                    };

                    let (utility, confidence) = self.calculate_utility(
                        resource,
                        target_id,
                        reference_buy_price,
                        lot_size,
                        &utility_context,
                    );
                    if utility > resource_best_utility {
                        resource_best_utility = utility;
                        resource_best_confidence = confidence;
                    }
                }
                if !resource_best_utility.is_finite() {
                    continue;
                }

                candidates.push((target_id, resource_best_utility, resource_best_confidence));

                if resource_best_utility > best_utility {
                    best_utility = resource_best_utility;
                    best_target = Some(target_id);
                    best_confidence = resource_best_confidence;
                }
            }

            if candidates.is_empty() {
                return None;
            }

            let best_neglect_ticks = best_target
                .map(|target_id| {
                    current_tick.saturating_sub(self.ledger[target_id].last_seen_tick) as f32
                })
                .unwrap_or(0.0);
            let neglect_boost = (best_neglect_ticks * 0.0008).min(0.20);

            let speculation_chance = (tuning.speculation_floor
                + (1.0 - best_confidence) * tuning.speculation_staleness_scale
                + neglect_boost)
                .clamp(tuning.speculation_floor, 0.85);

            let mut rng = ::rand::thread_rng();
            if rng.gen_bool(speculation_chance as f64) {
                let mut scored: Vec<(usize, f32)> = candidates
                    .into_iter()
                    .map(|(target_id, utility, confidence)| {
                        let uncertainty_bonus =
                            (1.0 - confidence) * tuning.speculation_uncertainty_bonus;
                        (target_id, utility + uncertainty_bonus)
                    })
                    .collect();

                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                let top_k = scored.len().min(3);
                if top_k == 0 {
                    return best_target;
                }

                let top_slice = &scored[..top_k];
                let temperature = (1.0 + tuning.speculation_uncertainty_bonus * 0.05).max(0.1);
                let best_score = top_slice[0].1;

                let mut weighted_sum = 0.0_f32;
                let mut weighted: Vec<(usize, f32)> = Vec::with_capacity(top_k);
                for (target_id, score) in top_slice.iter().copied() {
                    let stabilized = ((score - best_score) / temperature).exp().max(0.001);
                    weighted_sum += stabilized;
                    weighted.push((target_id, stabilized));
                }

                let mut draw = rng.gen_range(0.0..weighted_sum.max(0.001));
                for (target_id, weight) in weighted {
                    draw -= weight;
                    if draw <= 0.0 {
                        return Some(target_id);
                    }
                }

                return Some(top_slice[0].0);
            }

            return best_target;
        }

        let mut best_target = None;
        let mut best_utility = f32::NEG_INFINITY;
        let utility_context = UtilityContext {
            island_positions,
            current_tick,
            tuning,
            outbound_recent_departures,
        };
        let current_prices = self
            .ledger
            .get(current_island_id)
            .map(|entry| entry.prices)
            .unwrap_or([0.0; RESOURCE_COUNT]);

        for target_id in 0..self.ledger.len() {
            if target_id == current_island_id {
                continue;
            }

            let mut best_resource_utility = f32::NEG_INFINITY;
            for resource in Resource::iter() {
                let buy_price = current_prices[resource.idx()] * ASK_PRICE_MULTIPLIER;
                let lot_size = self
                    .max_units_for_trade_action(resource)
                    .min(self.max_cargo_volume / resource.volume_per_unit().max(0.01));
                let (utility, _confidence) = self.calculate_utility(
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

        best_target
    }

    fn median_price_for_resource(&self, resource: Resource) -> f32 {
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

    fn destination_confidence(
        &self,
        target_id: usize,
        distance: f32,
        current_tick: u64,
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
    ) -> f32 {
        let transit_time = distance / self.speed.max(1.0);
        let data_age = current_tick.saturating_sub(self.ledger[target_id].tick_updated) as f32;
        let base_confidence = (-tuning.confidence_decay_k * (data_age + transit_time))
            .exp()
            .clamp(0.05, 1.0);

        let recent_route_flow = outbound_recent_departures
            .get(target_id)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let route_confidence_factor = if recent_route_flow >= 1.0 {
            1.0 / recent_route_flow
        } else {
            1.0
        };

        (base_confidence * route_confidence_factor).clamp(0.02, 1.0)
    }

    fn calculate_utility(
        &self,
        resource: Resource,
        target_id: usize,
        buy_price: f32,
        lot_size: f32,
        context: &UtilityContext<'_>,
    ) -> (f32, f32) {
        if target_id >= self.ledger.len() || target_id >= context.island_positions.len() {
            return (f32::NEG_INFINITY, 0.0);
        }

        if !buy_price.is_finite() || buy_price <= 0.0 {
            return (f32::NEG_INFINITY, 0.0);
        }

        let quoted_sell_price = self.ledger[target_id].prices[resource.idx()];
        let quoted_inventory = self.ledger[target_id].inventories[resource.idx()].max(0.0);
        let has_quoted_sell_price = quoted_sell_price.is_finite() && quoted_sell_price > 0.0;
        let median_market_price = self.median_price_for_resource(resource);
        let quoted_bid_price = quoted_sell_price * BID_PRICE_MULTIPLIER;
        let expected_sell_price = if has_quoted_sell_price {
            quoted_bid_price
        } else if median_market_price > 0.0 {
            median_market_price * BID_PRICE_MULTIPLIER
        } else {
            buy_price
        };

        let distance = (self.pos - context.island_positions[target_id]).length();
        let transit_time = distance / self.speed.max(1.0);
        let mut confidence = self.destination_confidence(
            target_id,
            distance,
            context.current_tick,
            context.tuning,
            context.outbound_recent_departures,
        );
        if !has_quoted_sell_price {
            confidence = (confidence * 0.45).clamp(0.02, 1.0);
        }

        let quoted_island_cash = self.ledger[target_id].cash;
        let has_quoted_cash = quoted_island_cash.is_finite() && quoted_island_cash > 0.0;
        let fallback_cash = self
            .median_island_cash()
            .unwrap_or(DEFAULT_MARKET_DEPTH_FALLBACK)
            .max(DEFAULT_MARKET_DEPTH_FALLBACK);
        let market_depth_cash = if has_quoted_cash {
            quoted_island_cash
        } else {
            confidence = (confidence * UNKNOWN_CASH_CONFIDENCE_SCALE).clamp(0.02, 1.0);
            fallback_cash
        };

        let available_storage = (INVENTORY_CARRYING_CAPACITY - quoted_inventory).max(0.0);
        let effective_lot_size = lot_size.max(0.0).min(available_storage);
        if effective_lot_size <= 0.0 {
            return (f32::NEG_INFINITY, confidence);
        }

        let gross_expected_revenue = expected_sell_price * effective_lot_size;
        let real_expected_revenue = gross_expected_revenue.min(market_depth_cash * 0.9);
        let real_expected_profit = real_expected_revenue - (buy_price * effective_lot_size);

        let average_base_cost = BASE_COSTS.iter().copied().sum::<f32>() / RESOURCE_COUNT as f32;
        let relative_price = (buy_price / average_base_cost).max(0.0);
        let price_risk_penalty = (relative_price - 1.0).max(0.0) * HIGH_PRICE_RISK_WEIGHT;
        let price_risk_factor = (1.0 / (1.0 + price_risk_penalty)).clamp(0.35, 1.0);
        confidence *= price_risk_factor;

        let expected_profit = real_expected_profit * confidence;
        let fuel_cost = distance
            * context.tuning.transport_cost_per_distance
            * context.tuning.cost_per_mile_factor
            * self.fuel_burn_rate;
        let maintenance_trip_cost = transit_time * self.maintenance_rate;
        let capital_carry_cost = buy_price
            * effective_lot_size
            * transit_time
            * context.tuning.capital_carry_cost_per_time;

        let data_age = context
            .current_tick
            .saturating_sub(self.ledger[target_id].tick_updated) as f32;
        let broke_revenue_threshold = gross_expected_revenue * BROKE_CASH_COVERAGE_RATIO;
        let recent_broke_factor = (1.0 - data_age / RECENT_BROKE_TICKS).clamp(0.0, 1.0);
        let broke_penalty = if has_quoted_cash && quoted_island_cash < broke_revenue_threshold {
            BROKE_ISLAND_UTILITY_PENALTY * recent_broke_factor
        } else {
            0.0
        };

        let last_seen_tick = self.ledger[target_id].last_seen_tick;
        let neglect_ticks = context.current_tick.saturating_sub(last_seen_tick) as f32;
        let neglect_bonus = (neglect_ticks * context.tuning.island_neglect_bonus_per_tick)
            .min(context.tuning.island_neglect_bonus_cap);

        let industrial_bonus = if resource == Resource::Iron || resource == Resource::Timber {
            let infra_excess =
                (self.ledger[target_id].infrastructure_level - INDUSTRIAL_INFRA_THRESHOLD).max(0.0);
            (infra_excess * INDUSTRIAL_INPUT_BONUS_PER_INFRA).min(INDUSTRIAL_INPUT_BONUS_CAP)
        } else {
            0.0
        };

        let utility = expected_profit
            - fuel_cost
            - maintenance_trip_cost
            - capital_carry_cost
            + neglect_bonus
            + industrial_bonus
            - broke_penalty;

        (utility, confidence)
    }

    /// Move toward target. Returns the island id when docking this tick.
    pub fn update(&mut self, dt: f32, cost_per_mile_factor: f32) -> Option<usize> {
        let to_target = self.target - self.pos;
        let dist = to_target.length();
        if dist < 1.0 {
            self.docked_at = self.target_island_id;
            return self.docked_at;
        }
        let step = self.speed * dt;
        let travel_distance = step.min(dist);
        self.cash -= travel_distance
            * self.fuel_burn_rate
            * TRAVEL_CASH_BURN_PER_DISTANCE
            * cost_per_mile_factor.clamp(0.2, 5.0);
        if step >= dist {
            if !self.has_no_cargo() {
                self.cargo_distance_accrued += dist;
            }
            self.pos = self.target;
            self.docked_at = self.target_island_id;
            self.docked_at
        } else {
            if !self.has_no_cargo() {
                self.cargo_distance_accrued += step;
            }
            self.pos += to_target.normalize() * step;
            None
        }
    }

    pub fn draw(&self) {
        let fill = match self.dominant_cargo_resource_by_value() {
            Some(resource) => match resource {
                Resource::Grain => YELLOW,
                Resource::Timber => GREEN,
                Resource::Iron => DARKGRAY,
                Resource::Tools => RED,
                Resource::Spices => PURPLE,
            },
            None => WHITE,
        };

        match self.archetype() {
            ShipArchetype::Freighter => {
                let half_size = 7.0;
                draw_rectangle(
                    self.pos.x - half_size,
                    self.pos.y - half_size,
                    half_size * 2.0,
                    half_size * 2.0,
                    fill,
                );
                draw_rectangle_lines(
                    self.pos.x - half_size,
                    self.pos.y - half_size,
                    half_size * 2.0,
                    half_size * 2.0,
                    2.0,
                    LIGHTGRAY,
                );
            }
            ShipArchetype::Runner => {
                let top = vec2(self.pos.x, self.pos.y - 8.0);
                let left = vec2(self.pos.x - 7.0, self.pos.y + 6.0);
                let right = vec2(self.pos.x + 7.0, self.pos.y + 6.0);
                draw_triangle(top, left, right, fill);
                draw_triangle_lines(top, left, right, 2.0, LIGHTGRAY);
            }
            ShipArchetype::Coaster => {
                draw_circle(self.pos.x, self.pos.y, 8.0, fill);
                draw_circle_lines(self.pos.x, self.pos.y, 8.0, 2.0, LIGHTGRAY);
            }
        }
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
