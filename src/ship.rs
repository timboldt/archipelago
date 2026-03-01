use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;

use crate::island::{
    Island, PriceEntry, PriceLedger, Resource, ASK_PRICE_MULTIPLIER, BASE_COSTS,
    BID_PRICE_MULTIPLIER, INVENTORY_CARRYING_CAPACITY, RESOURCE_COUNT,
};

const TRADE_LOT_SIZE: f32 = 16.0;
pub const STARTING_CASH: f32 = 200.0;
const UNKNOWN_CASH_CONFIDENCE_SCALE: f32 = 0.70;
const DEFAULT_MARKET_DEPTH_FALLBACK: f32 = 600.0;
const RECENT_BROKE_TICKS: f32 = 180.0;
const BROKE_ISLAND_UTILITY_PENALTY: f32 = 5.5;
const BROKE_CASH_COVERAGE_RATIO: f32 = 0.35;

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
    pub island_neglect_bonus_per_tick: f32,
    pub island_neglect_bonus_cap: f32,
    pub luxury_weight: f32,
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
            island_neglect_bonus_per_tick: 0.006,
            island_neglect_bonus_cap: 18.0,
            luxury_weight: 0.12,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PurchaseRecord {
    unit_price: f32,
    resource: Resource,
}

const MAX_REALIZED_FREIGHT_SHARE: f32 = 0.50;
const MIN_SHIP_SPEED: f32 = 120.0;
const MAX_SHIP_SPEED: f32 = 600.0;
const MIN_GENE_SCALE: f32 = 0.80;
const MAX_GENE_SCALE: f32 = 1.20;

#[derive(Clone, Copy, Debug)]
pub struct Cargo {
    pub resource: Resource,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct StrategyGenes {
    confidence_decay_scale: f32,
    speculation_floor_scale: f32,
    learning_weight_scale: f32,
    transport_cost_scale: f32,
    luxury_weight_scale: f32,
}

impl Default for StrategyGenes {
    fn default() -> Self {
        Self {
            confidence_decay_scale: 1.0,
            speculation_floor_scale: 1.0,
            learning_weight_scale: 1.0,
            transport_cost_scale: 1.0,
            luxury_weight_scale: 1.0,
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

pub struct Ship {
    pub pos: Vec2,
    target: Vec2,
    speed: f32,
    target_island_id: Option<usize>,
    docked_at: Option<usize>,
    pub cargo: Option<Cargo>,
    pub cash: f32,
    pub ledger: PriceLedger,
    route_memory: Vec<f32>,
    last_purchase: Option<PurchaseRecord>,
    cargo_distance_accrued: f32,
    strategy_genes: StrategyGenes,
    planned_target_after_load: Option<usize>,
    just_sold_resource: Option<Resource>,
    last_dock_action: DockAction,
}

impl Ship {
    pub fn new(pos: Vec2, speed: f32, num_islands: usize, docked_island_id: usize) -> Self {
        Self {
            pos,
            target: pos,
            speed,
            target_island_id: Some(docked_island_id),
            docked_at: Some(docked_island_id),
            cargo: None,
            cash: STARTING_CASH,
            ledger: vec![
                PriceEntry {
                    prices: [0.0; RESOURCE_COUNT],
                    inventories: [0.0; RESOURCE_COUNT],
                    cash: 0.0,
                    tick_updated: 0,
                    last_seen_tick: 0,
                };
                num_islands
            ],
            route_memory: vec![0.0; num_islands],
            last_purchase: None,
            cargo_distance_accrued: 0.0,
            strategy_genes: StrategyGenes::default(),
            planned_target_after_load: None,
            just_sold_resource: None,
            last_dock_action: DockAction::None,
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
        tuned.luxury_weight =
            (tuned.luxury_weight * self.strategy_genes.luxury_weight_scale).clamp(-2.0, 2.0);
        tuned
    }

    pub fn spawn_daughter(&mut self, mutation_strength: f32, rng: &mut impl Rng) -> Option<Ship> {
        let docked_island_id = self.docked_island()?;
        let num_islands = self.ledger.len();

        let speed_mutation = 1.0 + rng.gen_range(-mutation_strength..mutation_strength);
        let daughter_speed = (self.speed * speed_mutation).clamp(MIN_SHIP_SPEED, MAX_SHIP_SPEED);

        let endowment = self.cash * 0.5;
        self.cash -= endowment;

        let mut daughter = Ship::new(self.pos, daughter_speed, num_islands, docked_island_id);
        daughter.cash = endowment;
        daughter.ledger = self.ledger.clone();
        daughter.route_memory = self.route_memory.clone();

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
            luxury_weight_scale: mutate_gene_gaussian(
                self.strategy_genes.luxury_weight_scale,
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
        if let Some(cargo) = self.cargo {
            let cargo_book_price = self
                .last_purchase
                .filter(|purchase| purchase.resource == cargo.resource)
                .map(|purchase| purchase.unit_price)
                .unwrap_or_else(|| self.median_price_for_resource(cargo.resource));
            let conservative_cargo_value =
                (cargo_book_price * BID_PRICE_MULTIPLIER * cargo.amount).max(0.0);
            net_worth += conservative_cargo_value;
        }
        net_worth
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
        if let Some(mut cargo) = self.cargo.take() {
            let (sold_amount, gross_revenue) = island.sell_to_island(cargo.resource, cargo.amount);
            if sold_amount <= 0.0 || gross_revenue <= 0.0 {
                self.cargo = Some(cargo);
                return self.last_dock_action;
            }

            let raw_freight_cost =
                self.cargo_distance_accrued * sold_amount * tuning.transport_cost_per_distance;
            let realized_freight_cost =
                raw_freight_cost.min(gross_revenue * MAX_REALIZED_FREIGHT_SHARE);
            let net_revenue = gross_revenue - realized_freight_cost;
            self.cash += net_revenue;

            if let Some(purchase) = self.last_purchase {
                if purchase.resource == cargo.resource
                    && sold_amount > 0.0
                    && island_id < self.route_memory.len()
                {
                    let sale_unit_price = net_revenue / sold_amount;
                    let normalized_margin =
                        (sale_unit_price - purchase.unit_price) / (purchase.unit_price + 1.0);
                    self.route_memory[island_id] += normalized_margin * tuning.learning_rate;
                    self.route_memory[island_id] = self.route_memory[island_id].clamp(-1.5, 1.5);
                }
            }

            let remaining = (cargo.amount - sold_amount).max(0.0);
            if remaining > 0.0 {
                cargo.amount = remaining;
                self.cargo = Some(cargo);
            } else {
                self.last_purchase = None;
                self.cargo_distance_accrued = 0.0;
            }

            self.just_sold_resource = Some(cargo.resource);

            self.last_dock_action = DockAction::Sold;
        }
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

        let Some(cargo) = self.cargo else {
            return self.last_dock_action;
        };

        let cargo_bid_price = island.bid_price(cargo.resource);
        if !cargo_bid_price.is_finite() || cargo_bid_price <= 0.0 || cargo.amount <= 0.0 {
            return self.last_dock_action;
        }

        let cargo_value_budget = cargo.amount * cargo_bid_price;
        if cargo_value_budget <= 0.0 {
            return self.last_dock_action;
        }

        let utility_context = UtilityContext {
            island_positions: context.island_positions,
            current_tick: context.current_tick,
            tuning: context.tuning,
            outbound_recent_departures: context.outbound_recent_departures,
        };

        let mut best_choice: Option<(Resource, usize, f32, f32)> = None;
        let mut best_utility = f32::NEG_INFINITY;

        for resource in Resource::iter() {
            if resource == cargo.resource {
                continue;
            }

            let ask_price = island.ask_price(resource);
            if !ask_price.is_finite() || ask_price <= 0.0 {
                continue;
            }

            let available = island.inventory[resource.idx()].max(0.0);
            let required_amount_for_full_value = cargo_value_budget / ask_price;
            if required_amount_for_full_value <= 0.0 {
                continue;
            }

            if available < required_amount_for_full_value {
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
                    required_amount_for_full_value,
                    &utility_context,
                );

                if utility > best_utility {
                    best_utility = utility;
                    best_choice = Some((
                        resource,
                        target_id,
                        ask_price,
                        required_amount_for_full_value,
                    ));
                }
            }
        }

        let Some((resource, target_id, ask_price, acquired_amount)) = best_choice else {
            return self.last_dock_action;
        };

        if best_utility <= 0.0 || acquired_amount <= 0.0 {
            return self.last_dock_action;
        }

        island.inventory[cargo.resource.idx()] += cargo.amount;
        island.inventory[resource.idx()] =
            (island.inventory[resource.idx()] - acquired_amount).max(0.0);

        self.cargo = Some(Cargo {
            resource,
            amount: acquired_amount,
        });
        self.last_purchase = Some(PurchaseRecord {
            unit_price: ask_price,
            resource,
        });
        self.planned_target_after_load = Some(target_id);
        self.cargo_distance_accrued = 0.0;
        self.just_sold_resource = Some(cargo.resource);
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
        if self.cargo.is_some() {
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
            let projected_amount = TRADE_LOT_SIZE.min(affordable).min(available);
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
        let requested = TRADE_LOT_SIZE.min(affordable);
        if requested <= 0.0 {
            return self.last_dock_action;
        }

        let (filled, total_cost) = island.buy_from_island(chosen_resource, requested);
        if filled <= 0.0 {
            return self.last_dock_action;
        }
        self.cash -= total_cost;
        self.cargo = Some(Cargo {
            resource: chosen_resource,
            amount: filled,
        });
        self.last_purchase = Some(PurchaseRecord {
            unit_price: total_cost / filled,
            resource: chosen_resource,
        });
        self.planned_target_after_load = Some(chosen_target);
        self.cargo_distance_accrued = 0.0;
        self.last_dock_action = DockAction::Bought;
        self.last_dock_action
    }

    pub fn sync_ledgers_with_island(&mut self, island: &mut Island) {
        island.merge_ledger(&self.ledger);
        island.copy_ledger_to_ship(&mut self.ledger);
    }

    pub fn plan_next_island(
        &self,
        current_island_id: usize,
        island_positions: &[Vec2],
        current_tick: u64,
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
    ) -> Option<usize> {
        if let Some(cargo) = self.cargo {
            if let Some(preselected_target) = self.planned_target_after_load {
                if preselected_target != current_island_id && preselected_target < self.ledger.len()
                {
                    return Some(preselected_target);
                }
            }

            let resource_idx = cargo.resource.idx();
            let fallback_buy_price = self.median_price_for_resource(cargo.resource);
            let reference_buy_price = self
                .last_purchase
                .filter(|purchase| purchase.resource == cargo.resource)
                .map(|purchase| purchase.unit_price)
                .unwrap_or(fallback_buy_price);

            let mut demand_order: Vec<usize> = self
                .ledger
                .iter()
                .enumerate()
                .filter(|(target_id, _)| *target_id != current_island_id)
                .map(|(target_id, _)| target_id)
                .collect();
            demand_order.sort_by(|a, b| {
                let lhs = self.ledger[*a].prices[resource_idx];
                let rhs = self.ledger[*b].prices[resource_idx];
                rhs.partial_cmp(&lhs).unwrap_or(std::cmp::Ordering::Equal)
            });

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

            for target_id in demand_order {
                let (utility, confidence) = self.calculate_utility(
                    cargo.resource,
                    target_id,
                    reference_buy_price,
                    cargo.amount,
                    &utility_context,
                );
                candidates.push((target_id, utility, confidence));

                if utility > best_utility {
                    best_utility = utility;
                    best_target = Some(target_id);
                    best_confidence = confidence;
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
                let (utility, _confidence) = self.calculate_utility(
                    resource,
                    target_id,
                    buy_price,
                    TRADE_LOT_SIZE,
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
        let expected_profit = real_expected_profit * confidence;
        let fuel_cost = distance * context.tuning.transport_cost_per_distance;

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

        let average_base_cost = BASE_COSTS.iter().copied().sum::<f32>() / RESOURCE_COUNT as f32;
        let lot_scale = (effective_lot_size / TRADE_LOT_SIZE).clamp(0.25, 3.0);
        let luxury_signal = (buy_price * lot_scale) - average_base_cost;
        let luxury_bonus = context.tuning.luxury_weight * luxury_signal;

        let utility = expected_profit - fuel_cost + neglect_bonus + luxury_bonus - broke_penalty;

        (utility, confidence)
    }

    /// Move toward target. Returns the island id when docking this tick.
    pub fn update(&mut self, dt: f32) -> Option<usize> {
        let to_target = self.target - self.pos;
        let dist = to_target.length();
        if dist < 1.0 {
            self.docked_at = self.target_island_id;
            return self.docked_at;
        }
        let step = self.speed * dt;
        if step >= dist {
            if let Some(cargo) = self.cargo {
                let _ = cargo;
                self.cargo_distance_accrued += dist;
            }
            self.pos = self.target;
            self.docked_at = self.target_island_id;
            self.docked_at
        } else {
            if let Some(cargo) = self.cargo {
                let _ = cargo;
                self.cargo_distance_accrued += step;
            }
            self.pos += to_target.normalize() * step;
            None
        }
    }

    pub fn draw(&self) {
        let fill = match self.cargo {
            Some(cargo) => match cargo.resource {
                Resource::Grain => YELLOW,
                Resource::Timber => GREEN,
                Resource::Iron => DARKGRAY,
                Resource::Tools => RED,
            },
            None => WHITE,
        };

        draw_circle(self.pos.x, self.pos.y, 8.0, fill);
        draw_circle_lines(self.pos.x, self.pos.y, 8.0, 2.0, LIGHTGRAY);
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
