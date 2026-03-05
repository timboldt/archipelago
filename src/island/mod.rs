//! Island economy logic.
//!
//! All simulation math lives here as methods on `IslandEconomy`, preserving
//! testability independent of Bevy.

pub mod spawn;

use ::rand::Rng;
use bevy::prelude::*;
use strum::IntoEnumIterator;

use crate::components::{
    ask_multiplier, bid_multiplier, Commodity, Inventory, PriceEntry, PriceLedger, BASE_COSTS,
    COMMODITY_COUNT, INVENTORY_CARRYING_CAPACITY,
};

const INITIAL_POPULATION_MIN: f32 = 45.0;
const INITIAL_POPULATION_MAX: f32 = 140.0;
const INITIAL_CASH_MIN: f32 = 900.0;
const INITIAL_CASH_MAX: f32 = 2600.0;
const INITIAL_INFRASTRUCTURE_MIN: f32 = 0.7;
const INITIAL_INFRASTRUCTURE_MAX: f32 = 1.5;
const INITIAL_INFRA_CREDIT_MIN: f32 = 900.0;
const INITIAL_INFRA_CREDIT_MAX: f32 = 2600.0;
const ISLAND_SIZE_FACTOR_MIN: f32 = 0.75;
const ISLAND_SIZE_FACTOR_MAX: f32 = 1.40;
const MIN_POPULATION: f32 = 8.0;
const POPULATION_GROWTH_RATE: f32 = 0.07;
const POPULATION_STARVATION_RATE: f32 = 0.08;
const GRAIN_PER_CAPITA_STABILITY: f32 = 0.07;
const POPULATION_FLOOR_EPSILON: f32 = 0.05;
const GRAIN_SURVIVAL_PRODUCTION_FLOOR: f32 = 1.8;
const SURVIVAL_NON_GRAIN_TO_GRAIN_RATIO: f32 = 0.55;
const TOOL_FABRICATION_BASE_RATE: f32 = 0.45;
const GRAIN_EXTRACTION_BONUS: f32 = 1.35;
const TIMBER_EXTRACTION_BONUS: f32 = 1.25;
const TOOL_IRON_PER_BATCH: f32 = 1.35;
const TOOL_TIMBER_PER_BATCH: f32 = 1.0;
const TOOL_OUTPUT_PER_BATCH: f32 = 2.2;
const TARGET_TOOLS_PER_1K_POP: f32 = 50.0;
const TOOL_FABRICATOR_ADAPTIVE_GAIN: f32 = 0.7;
const TOOL_FABRICATOR_ADAPTIVE_CAP: f32 = 1.8;
const TOOLS_CONSUMPTION_SCALE: f32 = 0.04;
const INDUSTRIAL_LABOR_SCALING: f32 = 0.012;
const SCARCITY_LOG_SCALE: f32 = 2.4;
const SCARCITY_REFERENCE: f32 = 120.0;
const SPECIALIZATION_ZERO_PROBABILITY: f32 = 0.20;
const SPICE_SPECIALIZATION_ZERO_PROBABILITY: f32 = 0.65;
const FOCUS_PRODUCTION_BOOST: f32 = 1.9;
const NON_FOCUS_PRODUCTION_SCALE: f32 = 0.78;
const TOOLS_PRODUCTIVITY_CAP: f32 = 2.0;
const TOOLS_PRODUCTIVITY_SCALE: f32 = 0.22;
const PER_CAPITA_INFRA_CREDIT_GENERATION: f32 = 0.05;
const INDUSTRIAL_INFRA_CREDIT_GENERATION: f32 = 0.30;
const CAPITAL_INVESTMENT_THRESHOLD: f32 = 1600.0;
const CAPITAL_INVESTMENT_RATE: f32 = 0.06;
const INFRASTRUCTURE_INVESTMENT_EFFICIENCY: f32 = 0.00032;
const MAX_INFRASTRUCTURE_LEVEL: f32 = 3.5;

/// Core island economy state and market operations — used as a Bevy Component.
#[derive(Component)]
pub struct IslandEconomy {
    pub id: usize,
    pub inventory: Inventory,
    pub production_rates: Inventory,
    pub consumption_rates: Inventory,
    pub population: f32,
    pub cash: f32,
    pub infrastructure_level: f32,
    pub infra_credit: f32,
    pub resource_capacity: Inventory,
    pub population_capacity: f32,
    pub infrastructure_capacity: f32,
    pub local_prices: [f32; COMMODITY_COUNT],
}

impl IslandEconomy {
    /// Creates a new island economy with randomized capacities, production, and initial state.
    pub fn new(id: usize, num_islands: usize, rng: &mut impl Rng) -> (Self, PriceLedger) {
        let mut inventory = [0.0_f32; COMMODITY_COUNT];
        let mut production_rates = [0.0_f32; COMMODITY_COUNT];
        let mut consumption_rates = [0.0_f32; COMMODITY_COUNT];

        for resource in Commodity::iter() {
            let index = resource.idx();
            inventory[index] = rng.gen_range(25.0..125.0);
            production_rates[index] = match resource {
                Commodity::Tools => 0.0,
                Commodity::Grain => rng.gen_range(0.8..2.6),
                Commodity::Timber => {
                    if rng.gen_bool(SPECIALIZATION_ZERO_PROBABILITY as f64) {
                        0.0
                    } else {
                        rng.gen_range(0.7..2.4)
                    }
                }
                Commodity::Iron => {
                    if rng.gen_bool(SPECIALIZATION_ZERO_PROBABILITY as f64) {
                        0.0
                    } else {
                        rng.gen_range(0.35..1.6)
                    }
                }
                Commodity::Spices => {
                    if rng.gen_bool(SPICE_SPECIALIZATION_ZERO_PROBABILITY as f64) {
                        0.0
                    } else {
                        rng.gen_range(0.08..0.45)
                    }
                }
            };
            consumption_rates[index] = match resource {
                Commodity::Grain => rng.gen_range(0.8..2.2),
                Commodity::Tools => rng.gen_range(0.1..0.5),
                Commodity::Spices => rng.gen_range(0.02..0.16),
                Commodity::Timber | Commodity::Iron => rng.gen_range(0.05..0.4),
            };
        }

        if production_rates[Commodity::Timber.idx()] <= 0.0
            && production_rates[Commodity::Iron.idx()] <= 0.0
        {
            if rng.gen_bool(0.7) {
                production_rates[Commodity::Timber.idx()] = rng.gen_range(0.5..1.4);
            } else {
                production_rates[Commodity::Iron.idx()] = rng.gen_range(0.5..1.4);
            }
        }

        let size_factor = rng.gen_range(ISLAND_SIZE_FACTOR_MIN..ISLAND_SIZE_FACTOR_MAX);
        let mut resource_capacity = [0.0_f32; COMMODITY_COUNT];
        for resource in Commodity::iter() {
            let idx = resource.idx();
            let specialization_roll = rng.gen_range(0.80..1.25);
            resource_capacity[idx] =
                (INVENTORY_CARRYING_CAPACITY * size_factor * specialization_roll).max(40.0);
            inventory[idx] = inventory[idx].min(resource_capacity[idx] * 0.8_f32);
        }
        let population_capacity = (160.0 * size_factor).max(MIN_POPULATION + 12.0);
        let infrastructure_capacity = (MAX_INFRASTRUCTURE_LEVEL * (0.72 + 0.35 * size_factor))
            .clamp(0.9, MAX_INFRASTRUCTURE_LEVEL);

        let focus_resource = match rng.gen_range(0..4) {
            0 => Commodity::Grain,
            1 => Commodity::Timber,
            2 => Commodity::Iron,
            _ => Commodity::Spices,
        };
        for resource in [
            Commodity::Grain,
            Commodity::Timber,
            Commodity::Iron,
            Commodity::Spices,
        ] {
            let index = resource.idx();
            if resource == focus_resource {
                production_rates[index] *= FOCUS_PRODUCTION_BOOST;
            } else {
                production_rates[index] *= NON_FOCUS_PRODUCTION_SCALE;
            }
        }

        let mut economy = Self {
            id,
            inventory,
            production_rates,
            consumption_rates,
            population: rng.gen_range(INITIAL_POPULATION_MIN..INITIAL_POPULATION_MAX),
            cash: rng.gen_range(INITIAL_CASH_MIN..INITIAL_CASH_MAX),
            infrastructure_level: rng
                .gen_range(INITIAL_INFRASTRUCTURE_MIN..INITIAL_INFRASTRUCTURE_MAX),
            infra_credit: rng.gen_range(INITIAL_INFRA_CREDIT_MIN..INITIAL_INFRA_CREDIT_MAX),
            resource_capacity,
            population_capacity,
            infrastructure_capacity,
            local_prices: [0.0; COMMODITY_COUNT],
        };

        let mut ledger = vec![
            PriceEntry {
                prices: [0.0; COMMODITY_COUNT],
                inventories: [0.0; COMMODITY_COUNT],
                cash: 0.0,
                infrastructure_level: 0.0,
                tick_updated: 0,
                last_seen_tick: 0,
            };
            num_islands
        ];

        economy.recompute_local_prices_with_ledger(0, &mut ledger);
        (economy, ledger)
    }

    pub fn produce_consume_and_price(&mut self, dt: f32, tick: u64, ledger: &mut PriceLedger) {
        let grain_idx = Commodity::Grain.idx();
        let grain_stability_target = self.population * GRAIN_PER_CAPITA_STABILITY;
        let grain_balance =
            (self.inventory[grain_idx] - grain_stability_target) / (grain_stability_target + 1.0);
        let smooth_balance = grain_balance / (1.0 + grain_balance.abs());
        let growth_component = smooth_balance.max(0.0) * POPULATION_GROWTH_RATE;
        let starvation_component = (-smooth_balance).max(0.0) * POPULATION_STARVATION_RATE;
        self.population += self.population * (growth_component - starvation_component) * dt;

        if self.population <= MIN_POPULATION + POPULATION_FLOOR_EPSILON
            && self.inventory[grain_idx] < grain_stability_target
        {
            self.reset_survival_focus();
        }
        self.population = self
            .population
            .clamp(MIN_POPULATION, self.population_capacity.max(MIN_POPULATION));

        for resource in Commodity::iter() {
            let index = resource.idx();
            let inventory = self.inventory[index];
            let capacity = self.resource_capacity[index].max(1.0);
            let logistic_factor = (1.0 - (inventory / capacity)).clamp(0.0, 1.0);

            if resource != Commodity::Tools {
                let tools_boost = (1.0
                    + self.inventory[Commodity::Tools.idx()] * TOOLS_PRODUCTIVITY_SCALE)
                    .clamp(1.0, TOOLS_PRODUCTIVITY_CAP);
                let mut extraction =
                    self.production_rates[index] * self.population * logistic_factor * dt;
                if resource == Commodity::Grain {
                    extraction *= GRAIN_EXTRACTION_BONUS;
                } else if resource == Commodity::Timber {
                    extraction *= TIMBER_EXTRACTION_BONUS;
                }
                extraction *= tools_boost;
                self.inventory[index] += extraction;
                self.inventory[index] = self.inventory[index].min(capacity);
            }

            let demand = self.consumption_rates[index] * self.population * dt;
            let effective_demand = if resource == Commodity::Tools {
                demand * TOOLS_CONSUMPTION_SCALE
            } else {
                demand
            };
            self.inventory[index] -= effective_demand;
            self.inventory[index] = self.inventory[index].max(0.0);
        }

        let iron_idx = Commodity::Iron.idx();
        let timber_idx = Commodity::Timber.idx();
        let tools_idx = Commodity::Tools.idx();

        let labor_multiplier = (self.population * INDUSTRIAL_LABOR_SCALING).clamp(0.25, 8.0);
        let local_tools_per_1k_pop = if self.population > 0.0 {
            self.inventory[tools_idx] * 1000.0 / self.population
        } else {
            TARGET_TOOLS_PER_1K_POP
        };
        let tool_shortfall_ratio =
            ((TARGET_TOOLS_PER_1K_POP - local_tools_per_1k_pop) / TARGET_TOOLS_PER_1K_POP).max(0.0);
        let adaptive_tool_boost = (1.0 + tool_shortfall_ratio * TOOL_FABRICATOR_ADAPTIVE_GAIN)
            .clamp(1.0, TOOL_FABRICATOR_ADAPTIVE_CAP);

        let industrial_rate = TOOL_FABRICATION_BASE_RATE
            * self.infrastructure_level
            * labor_multiplier
            * adaptive_tool_boost
            * dt;
        let tool_headroom =
            (self.resource_capacity[tools_idx] - self.inventory[tools_idx]).max(0.0);
        let feasible_batch = (self.inventory[iron_idx] / TOOL_IRON_PER_BATCH)
            .min(self.inventory[timber_idx] / TOOL_TIMBER_PER_BATCH)
            .min(industrial_rate)
            .min(tool_headroom / TOOL_OUTPUT_PER_BATCH)
            .max(0.0);
        if feasible_batch > 0.0 {
            self.inventory[iron_idx] -= feasible_batch * TOOL_IRON_PER_BATCH;
            self.inventory[timber_idx] -= feasible_batch * TOOL_TIMBER_PER_BATCH;
            self.inventory[tools_idx] += feasible_batch * TOOL_OUTPUT_PER_BATCH;
        }

        let infra_headroom_ratio = ((self.infrastructure_capacity - self.infrastructure_level)
            / self.infrastructure_capacity.max(0.01))
        .clamp(0.0, 1.0);
        let infra_credit_income = (self.population * PER_CAPITA_INFRA_CREDIT_GENERATION
            + feasible_batch * INDUSTRIAL_INFRA_CREDIT_GENERATION)
            * dt
            * infra_headroom_ratio;
        self.infra_credit += infra_credit_income.max(0.0);

        if self.infra_credit > CAPITAL_INVESTMENT_THRESHOLD {
            let excess_credit = self.infra_credit - CAPITAL_INVESTMENT_THRESHOLD;
            let investment = (excess_credit * CAPITAL_INVESTMENT_RATE * dt).min(self.infra_credit);
            self.infra_credit -= investment;
            self.infrastructure_level = (self.infrastructure_level
                + investment * INFRASTRUCTURE_INVESTMENT_EFFICIENCY)
                .min(self.infrastructure_capacity);
        }

        self.recompute_local_prices_with_ledger(tick, ledger);
    }

    fn reset_survival_focus(&mut self) {
        let grain_idx = Commodity::Grain.idx();
        let timber_idx = Commodity::Timber.idx();
        let iron_idx = Commodity::Iron.idx();
        let spices_idx = Commodity::Spices.idx();

        self.production_rates[grain_idx] =
            self.production_rates[grain_idx].max(GRAIN_SURVIVAL_PRODUCTION_FLOOR);

        let non_grain_ceiling =
            self.production_rates[grain_idx] * SURVIVAL_NON_GRAIN_TO_GRAIN_RATIO;
        self.production_rates[timber_idx] =
            self.production_rates[timber_idx].min(non_grain_ceiling);
        self.production_rates[iron_idx] = self.production_rates[iron_idx].min(non_grain_ceiling);
        self.production_rates[spices_idx] =
            self.production_rates[spices_idx].min(non_grain_ceiling);
    }

    /// Recomputes local scarcity-adjusted prices and updates this island's self ledger entry.
    pub fn recompute_local_prices_with_ledger(&mut self, tick: u64, ledger: &mut PriceLedger) {
        for resource in Commodity::iter() {
            let index = resource.idx();
            let inventory = self.inventory[index].max(0.0);
            let scarcity_pressure = (SCARCITY_REFERENCE / (inventory + 1.0)).ln_1p();
            self.local_prices[index] =
                BASE_COSTS[index] * (1.0 + SCARCITY_LOG_SCALE * scarcity_pressure);
        }
        if let Some(entry) = ledger.get_mut(self.id) {
            entry.prices = self.local_prices;
            entry.inventories = self.inventory;
            entry.cash = self.cash;
            entry.infrastructure_level = self.infrastructure_level;
            entry.tick_updated = tick;
        }
    }

    /// Marks that this island was directly observed at `tick`.
    pub fn mark_seen(&self, tick: u64, ledger: &mut PriceLedger) {
        if let Some(entry) = ledger.get_mut(self.id) {
            entry.last_seen_tick = tick;
        }
    }

    /// Buys `resource` from a ship at bid price.
    /// Returns `(filled_amount, total_value_paid)`.
    pub fn sell_to_island(
        &mut self,
        resource: Commodity,
        amount: f32,
        market_spread: f32,
    ) -> (f32, f32) {
        if amount <= 0.0 {
            return (0.0, 0.0);
        }
        let index = resource.idx();
        let price = self.bid_price(resource, market_spread);
        if !price.is_finite() || price <= 0.0 {
            return (0.0, 0.0);
        }

        let total_value = amount * price;
        self.inventory[index] += amount;
        self.cash -= total_value;
        (amount, total_value)
    }

    /// Sells `resource` to a ship at ask price, limited by island inventory.
    /// Returns `(filled_amount, total_cost_charged)`.
    pub fn buy_from_island(
        &mut self,
        resource: Commodity,
        requested_amount: f32,
        market_spread: f32,
    ) -> (f32, f32) {
        if requested_amount <= 0.0 {
            return (0.0, 0.0);
        }
        let index = resource.idx();
        let available = self.inventory[index].max(0.0);
        let filled = requested_amount.min(available);
        if filled <= 0.0 {
            return (0.0, 0.0);
        }
        self.inventory[index] -= filled;
        let total_cost = filled * self.ask_price(resource, market_spread);
        self.cash += total_cost;
        (filled, total_cost)
    }

    /// Credits dock services paid by ships and returns accepted payment.
    pub fn accept_service_payment(&mut self, repair_amount: f32, labor_amount: f32) -> f32 {
        let paid = repair_amount.max(0.0) + labor_amount.max(0.0);
        self.cash += paid;
        paid
    }

    /// Applies one-time ship bankruptcy settlement credit to island cash.
    pub fn apply_ship_bankruptcy_settlement(&mut self, settlement: f32) {
        self.cash += settlement;
    }

    /// Returns the island bid price for `resource` after applying spread.
    pub fn bid_price(&self, resource: Commodity, market_spread: f32) -> f32 {
        self.local_prices[resource.idx()] * bid_multiplier(market_spread)
    }

    /// Returns the island ask price for `resource` after applying spread.
    pub fn ask_price(&self, resource: Commodity, market_spread: f32) -> f32 {
        self.local_prices[resource.idx()] * ask_multiplier(market_spread)
    }

    /// Create a non-Component copy of economy data for ship seeding.
    pub fn clone_for_seeding(source: &IslandEconomy) -> IslandEconomy {
        IslandEconomy {
            id: source.id,
            inventory: source.inventory,
            production_rates: source.production_rates,
            consumption_rates: source.consumption_rates,
            population: source.population,
            cash: source.cash,
            infrastructure_level: source.infrastructure_level,
            infra_credit: source.infra_credit,
            resource_capacity: source.resource_capacity,
            population_capacity: source.population_capacity,
            infrastructure_capacity: source.infrastructure_capacity,
            local_prices: source.local_prices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn approx_eq(left: f32, right: f32) {
        assert!((left - right).abs() <= 1e-4, "left={left}, right={right}");
    }

    #[test]
    fn recompute_prices_updates_self_ledger_entry() {
        let mut rng = StdRng::seed_from_u64(7);
        let (mut economy, mut ledger) = IslandEconomy::new(0, 3, &mut rng);
        economy.inventory = [0.0, 20.0, 30.0, 40.0, 50.0];
        economy.cash = 1234.0;
        economy.infrastructure_level = 1.75;

        economy.recompute_local_prices_with_ledger(42, &mut ledger);

        let entry = ledger[0];
        assert_eq!(entry.prices, economy.local_prices);
        assert_eq!(entry.inventories, economy.inventory);
        approx_eq(entry.cash, economy.cash);
        approx_eq(entry.infrastructure_level, economy.infrastructure_level);
        assert_eq!(entry.tick_updated, 42);
        assert!(economy.local_prices[Commodity::Grain.idx()] > BASE_COSTS[Commodity::Grain.idx()]);
    }

    #[test]
    fn sell_to_island_accepts_goods_beyond_cash() {
        let mut rng = StdRng::seed_from_u64(11);
        let (mut economy, _ledger) = IslandEconomy::new(0, 1, &mut rng);
        economy.cash = 10.0;
        economy.local_prices[Commodity::Tools.idx()] = 120.0;
        let starting_tools = economy.inventory[Commodity::Tools.idx()];
        let expected_price = 120.0 * (1.0 - 0.1 * 0.5);

        let (filled, paid) = economy.sell_to_island(Commodity::Tools, 1.0, 0.1);

        approx_eq(filled, 1.0);
        approx_eq(paid, expected_price);
        approx_eq(
            economy.inventory[Commodity::Tools.idx()],
            starting_tools + 1.0,
        );
        assert!(economy.cash < 0.0);
    }

    #[test]
    fn buy_from_island_is_inventory_limited() {
        let mut rng = StdRng::seed_from_u64(19);
        let (mut economy, _ledger) = IslandEconomy::new(0, 1, &mut rng);
        economy.inventory[Commodity::Grain.idx()] = 2.0;
        economy.local_prices[Commodity::Grain.idx()] = 50.0;
        let starting_cash = economy.cash;

        let (filled, cost) = economy.buy_from_island(Commodity::Grain, 5.0, 0.1);

        approx_eq(filled, 2.0);
        approx_eq(cost, 2.0 * 50.0 * 1.05);
        approx_eq(economy.inventory[Commodity::Grain.idx()], 0.0);
        approx_eq(economy.cash, starting_cash + cost);
    }

    #[test]
    fn produce_consume_and_price_keeps_state_bounded() {
        let mut rng = StdRng::seed_from_u64(23);
        let (mut economy, mut ledger) = IslandEconomy::new(0, 1, &mut rng);
        economy.population = 64.0;
        economy.inventory = [30.0, 30.0, 30.0, 30.0, 30.0];
        economy.production_rates = [1.0, 1.0, 1.0, 0.0, 0.2];
        economy.consumption_rates = [0.5, 0.1, 0.1, 0.1, 0.05];

        economy.produce_consume_and_price(1.0, 99, &mut ledger);

        for amount in economy.inventory {
            assert!(amount.is_finite());
            assert!(amount >= 0.0);
        }
        assert!(economy.population.is_finite());
        assert!(economy.population >= MIN_POPULATION);
        assert!(economy.population <= economy.population_capacity.max(MIN_POPULATION));
        assert_eq!(ledger[0].tick_updated, 99);
    }
}
