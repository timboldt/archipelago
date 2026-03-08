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

// --- Island initialization ranges ---
/// Range for starting population.
const INITIAL_POPULATION_MIN: f32 = 45.0;
const INITIAL_POPULATION_MAX: f32 = 140.0;
/// Range for starting cash reserves.
const INITIAL_CASH_MIN: f32 = 900.0;
const INITIAL_CASH_MAX: f32 = 2600.0;
/// Range for starting infrastructure level.
const INITIAL_INFRASTRUCTURE_MIN: f32 = 0.7;
const INITIAL_INFRASTRUCTURE_MAX: f32 = 1.5;
/// Range for starting infrastructure credit (funds earmarked for capital investment).
const INITIAL_INFRA_CREDIT_MIN: f32 = 900.0;
const INITIAL_INFRA_CREDIT_MAX: f32 = 2600.0;
/// Range for island visual/capacity size factor.
const ISLAND_SIZE_FACTOR_MIN: f32 = 0.35;
const ISLAND_SIZE_FACTOR_MAX: f32 = 2.0;

// --- Population dynamics ---
/// Minimum population floor; islands cannot shrink below this.
const MIN_POPULATION: f32 = 8.0;
/// Per-tick fractional population growth when food is adequate.
const POPULATION_GROWTH_RATE: f32 = 0.07;
/// Per-tick fractional population decline when food is scarce.
const POPULATION_STARVATION_RATE: f32 = 0.08;

// --- Production / consumption tuning ---
/// Grain consumed per capita to maintain population stability.
const GRAIN_PER_CAPITA_STABILITY: f32 = 0.07;
/// Base rate of tool fabrication before adaptive adjustments.
const TOOL_FABRICATION_BASE_RATE: f32 = 0.45;
/// Bonus multiplier for grain extraction (farming yield).
const GRAIN_EXTRACTION_BONUS: f32 = 1.35;
/// Bonus multiplier for timber extraction (logging yield).
const TIMBER_EXTRACTION_BONUS: f32 = 1.25;
/// Iron consumed per tool fabrication batch.
const TOOL_IRON_PER_BATCH: f32 = 1.35;
/// Timber consumed per tool fabrication batch.
const TOOL_TIMBER_PER_BATCH: f32 = 1.0;
/// Tools produced per fabrication batch.
const TOOL_OUTPUT_PER_BATCH: f32 = 2.2;
/// Desired tools per 1000 population for adaptive fabrication targeting.
const TARGET_TOOLS_PER_1K_POP: f32 = 50.0;
/// How aggressively fabrication rate adjusts toward the target.
const TOOL_FABRICATOR_ADAPTIVE_GAIN: f32 = 0.7;
/// Maximum adaptive multiplier on tool fabrication rate.
const TOOL_FABRICATOR_ADAPTIVE_CAP: f32 = 1.8;
/// Fractional per-tick tool wear/consumption.
const TOOLS_CONSUMPTION_SCALE: f32 = 0.04;
/// Labor fraction devoted to industry, scaled by infrastructure.
const INDUSTRIAL_LABOR_SCALING: f32 = 0.012;
/// Logarithmic scale factor for scarcity-based pricing.
const SCARCITY_LOG_SCALE: f32 = 2.4;
/// Per-capita passive income per tick (represents internal island economy).
const PER_CAPITA_PASSIVE_INCOME: f32 = 0.15;

// --- Island resource specialization ---
/// Probability an island has zero specialization in a given commodity.
const SPECIALIZATION_ZERO_PROBABILITY: f32 = 0.40;
/// Higher zero-probability for spice specialization (rarer resource).
const SPICE_SPECIALIZATION_ZERO_PROBABILITY: f32 = 0.50;

// --- Focus resource boost ---
/// Production multiplier applied to the island's focus commodity.
const FOCUS_PRODUCTION_BOOST: f32 = 2.8;
/// Production multiplier for non-focus commodities.
const NON_FOCUS_PRODUCTION_SCALE: f32 = 0.45;

/// Maximum productivity multiplier from tool availability.
const TOOLS_PRODUCTIVITY_CAP: f32 = 2.0;
/// How quickly tool availability converts to productivity gain.
const TOOLS_PRODUCTIVITY_SCALE: f32 = 0.22;

// --- Governor: labor allocation ---
/// EMA smoothing factor for governor labor share adjustments.
const GOVERNOR_SMOOTHING: f32 = 0.15;
/// Sigmoid scale for commodity urgency calculation.
const URGENCY_SCALE: f32 = 10.0;
/// Sigmoid offset for commodity urgency calculation.
const URGENCY_OFFSET: f32 = 1.0;
/// Relative labor priority weights per commodity.
const GRAIN_PRIORITY_WEIGHT: f32 = 3.0;
const TIMBER_PRIORITY_WEIGHT: f32 = 1.0;
const IRON_PRIORITY_WEIGHT: f32 = 1.0;
const SPICES_PRIORITY_WEIGHT: f32 = 0.4;
/// Minimum fraction of labor always allocated to grain.
const GRAIN_LABOR_FLOOR: f32 = 0.15;
/// Scale factor for tool-chain labor demand (iron + timber for tools).
const TOOL_CHAIN_SCALE: f32 = 4.0;
/// Iron share of tool-chain labor weight.
const IRON_FOR_TOOLS_WEIGHT: f32 = 0.6;
/// Timber share of tool-chain labor weight.
const TIMBER_FOR_TOOLS_WEIGHT: f32 = 0.4;
/// Inventory reference level for demand destruction dampening.
const DEMAND_DESTRUCTION_REFERENCE: f32 = 20.0;
/// Spice productivity scaling from spice specialization level.
const SPICE_PRODUCTIVITY_SCALE: f32 = 0.12;
/// Cap on spice productivity multiplier.
const SPICE_PRODUCTIVITY_CAP: f32 = 1.20;

// --- Infrastructure / capital investment ---
/// Per-capita infrastructure credit generation per tick.
const PER_CAPITA_INFRA_CREDIT_GENERATION: f32 = 0.05;
/// Extra infra credit generation from industrial activity.
const INDUSTRIAL_INFRA_CREDIT_GENERATION: f32 = 0.30;
/// Minimum infra credit balance before capital investment triggers.
const CAPITAL_INVESTMENT_THRESHOLD: f32 = 1600.0;
/// Fraction of excess credits invested per tick.
const CAPITAL_INVESTMENT_RATE: f32 = 0.06;
/// Credits-to-infrastructure conversion efficiency.
const INFRASTRUCTURE_INVESTMENT_EFFICIENCY: f32 = 0.00032;
/// Hard cap on infrastructure level.
const MAX_INFRASTRUCTURE_LEVEL: f32 = 3.5;

/// Core island economy state and market operations — used as a Bevy Component.
#[derive(Component, Clone)]
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
    pub labor_allocation: [f32; COMMODITY_COUNT],
    pub spice_morale_bonus: f32,
    pub last_trade_tick: u64,
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
                        rng.gen_range(0.15..0.60)
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

        // Ensure at least one raw industrial resource (Timber or Iron) has
        // meaningful production — without this, the island can never fabricate
        // tools locally or contribute industrial inputs to the trade network.
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
            } else if resource != Commodity::Grain {
                // Grain is exempt from non-focus penalty — all islands
                // need baseline food production to sustain their population.
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
            labor_allocation: Self::initial_labor_allocation_for(&production_rates),
            spice_morale_bonus: 1.0,
            last_trade_tick: 0,
        };

        let mut ledger = vec![
            PriceEntry {
                prices: [0.0; COMMODITY_COUNT],
                inventories: [0.0; COMMODITY_COUNT],
                capacities: [0.0; COMMODITY_COUNT],
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

    /// Computes an initial (non-smoothed) labor allocation from production rates.
    pub fn initial_labor_allocation_for(
        production_rates: &[f32; COMMODITY_COUNT],
    ) -> [f32; COMMODITY_COUNT] {
        let mut alloc = [0.0_f32; COMMODITY_COUNT];
        let mut total = 0.0;
        for c in Commodity::iter() {
            if c == Commodity::Tools {
                continue;
            }
            let idx = c.idx();
            if production_rates[idx] > 0.0 {
                alloc[idx] = 1.0; // equal weight initially
                total += 1.0;
            }
        }
        if total > 0.0 {
            for v in &mut alloc {
                *v /= total;
            }
        }
        alloc
    }

    fn priority_weight(c: Commodity) -> f32 {
        match c {
            Commodity::Grain => GRAIN_PRIORITY_WEIGHT,
            Commodity::Timber => TIMBER_PRIORITY_WEIGHT,
            Commodity::Iron => IRON_PRIORITY_WEIGHT,
            Commodity::Spices => SPICES_PRIORITY_WEIGHT,
            Commodity::Tools => 0.0,
        }
    }

    fn update_labor_allocation(&mut self) {
        let pop = self.population.max(1.0);
        let tools_idx = Commodity::Tools.idx();

        // Compute average non-zero production rate (excluding Tools).
        let mut rate_sum = 0.0_f32;
        let mut rate_count = 0_u32;
        for c in Commodity::iter() {
            if c == Commodity::Tools {
                continue;
            }
            let r = self.production_rates[c.idx()];
            if r > 0.0 {
                rate_sum += r;
                rate_count += 1;
            }
        }
        let avg_rate = if rate_count > 0 {
            rate_sum / rate_count as f32
        } else {
            1.0
        };

        // Tool scarcity pressure for tool-chain bonus.
        let tools_per_1k = if pop > 0.0 {
            self.inventory[tools_idx] * 1000.0 / pop
        } else {
            TARGET_TOOLS_PER_1K_POP
        };
        let tool_scarcity =
            ((TARGET_TOOLS_PER_1K_POP - tools_per_1k) / TARGET_TOOLS_PER_1K_POP).clamp(0.0, 1.0);

        let mut urgency = [0.0_f32; COMMODITY_COUNT];
        let mut urgency_total = 0.0_f32;

        for c in Commodity::iter() {
            if c == Commodity::Tools {
                continue;
            }
            let idx = c.idx();
            if self.production_rates[idx] <= 0.0 {
                continue;
            }

            let consumption = self.consumption_rates[idx] * pop;
            let days_of_supply = if consumption > 0.0 {
                self.inventory[idx] / consumption
            } else {
                100.0 // no consumption = no urgency
            };
            let raw = URGENCY_SCALE / (days_of_supply + URGENCY_OFFSET);
            let comparative_advantage = (self.production_rates[idx] / avg_rate).clamp(0.5, 2.0);
            urgency[idx] = raw * Self::priority_weight(c) * comparative_advantage;
        }

        // Tool-chain bonus: when tools are scarce, boost Iron and Timber urgency.
        let iron_idx = Commodity::Iron.idx();
        let timber_idx = Commodity::Timber.idx();
        let tool_chain_extra = tool_scarcity * TOOL_CHAIN_SCALE;
        if self.production_rates[iron_idx] > 0.0 {
            urgency[iron_idx] += tool_chain_extra * IRON_FOR_TOOLS_WEIGHT;
        }
        if self.production_rates[timber_idx] > 0.0 {
            urgency[timber_idx] += tool_chain_extra * TIMBER_FOR_TOOLS_WEIGHT;
        }

        for v in &urgency {
            urgency_total += v;
        }

        // Normalize to target allocation.
        let mut target = [0.0_f32; COMMODITY_COUNT];
        if urgency_total > 0.0 {
            for (t, u) in target.iter_mut().zip(urgency.iter()) {
                *t = u / urgency_total;
            }
        }

        // Enforce grain labor floor.
        let grain_idx = Commodity::Grain.idx();
        if self.production_rates[grain_idx] > 0.0 && target[grain_idx] < GRAIN_LABOR_FLOOR {
            let deficit = GRAIN_LABOR_FLOOR - target[grain_idx];
            target[grain_idx] = GRAIN_LABOR_FLOOR;
            // Proportionally reduce others.
            let others_total: f32 = target
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != grain_idx)
                .map(|(_, v)| *v)
                .sum();
            if others_total > 0.0 {
                for (i, t) in target.iter_mut().enumerate() {
                    if i != grain_idx {
                        *t -= *t * deficit / others_total;
                        *t = t.max(0.0);
                    }
                }
            }
        }

        // EMA smoothing.
        for (alloc, t) in self.labor_allocation.iter_mut().zip(target.iter()) {
            *alloc = GOVERNOR_SMOOTHING * t + (1.0 - GOVERNOR_SMOOTHING) * *alloc;
        }

        // Re-normalize to ensure sum == 1.0.
        let alloc_sum: f32 = self.labor_allocation.iter().sum();
        if alloc_sum > 0.0 {
            for v in &mut self.labor_allocation {
                *v /= alloc_sum;
            }
        }
    }

    fn update_spice_morale(&mut self) {
        let spice_per_capita = self.inventory[Commodity::Spices.idx()] / self.population.max(1.0);
        self.spice_morale_bonus = (1.0 + SPICE_PRODUCTIVITY_SCALE * spice_per_capita.sqrt())
            .clamp(1.0, SPICE_PRODUCTIVITY_CAP);
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

        self.population = self
            .population
            .clamp(MIN_POPULATION, self.population_capacity.max(MIN_POPULATION));

        self.update_labor_allocation();
        self.update_spice_morale();

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
                extraction *= tools_boost * self.labor_allocation[index] * self.spice_morale_bonus;
                self.inventory[index] += extraction;
                self.inventory[index] = self.inventory[index].min(capacity);
            }

            let availability =
                self.inventory[index] / (self.inventory[index] + DEMAND_DESTRUCTION_REFERENCE);
            let demand = self.consumption_rates[index] * self.population * dt * availability;
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

        let labor_multiplier = (self.population * INDUSTRIAL_LABOR_SCALING).clamp(0.25, 120.0);
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

        // Passive per-capita income representing the island's internal economy.
        // This keeps islands liquid enough to buy imports from ships.
        self.cash += self.population * PER_CAPITA_PASSIVE_INCOME * dt;

        self.recompute_local_prices_with_ledger(tick, ledger);
    }

    /// Recomputes local scarcity-adjusted prices and updates this island's self ledger entry.
    pub fn recompute_local_prices_with_ledger(&mut self, tick: u64, ledger: &mut PriceLedger) {
        for resource in Commodity::iter() {
            let index = resource.idx();
            let inventory = self.inventory[index].max(0.0);
            let capacity = self.resource_capacity[index].max(1.0);
            // Scarcity pressure is 0.0 when inventory == capacity.
            let scarcity_pressure = (capacity / (inventory + 1.0)).ln().max(0.0);
            self.local_prices[index] =
                BASE_COSTS[index] * (1.0 + SCARCITY_LOG_SCALE * scarcity_pressure);
        }
        if let Some(entry) = ledger.get_mut(self.id) {
            entry.prices = self.local_prices;
            entry.inventories = self.inventory;
            entry.capacities = self.resource_capacity;
            entry.cash = self.cash;
            entry.infrastructure_level = self.infrastructure_level;
            entry.tick_updated = tick;
        }
    }

    /// Marks that this island was directly observed at `tick`.
    pub fn mark_seen(&self, tick: u64, ledger: &mut PriceLedger) {
        if let Some(entry) = ledger.get_mut(self.id) {
            entry.last_seen_tick = tick;
            entry.capacities = self.resource_capacity;
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

        // Limit amount by island's available cash (allow limited negative cash
        // for "barter-like" trades that resolve within the docking cycle).
        // Overdraft scales with population so larger islands can sustain bigger trades.
        let max_overdraft = 800.0 + self.population * 5.0;
        let affordable_amount = if self.cash > -max_overdraft {
            ((self.cash + max_overdraft) / price).min(amount)
        } else {
            0.0
        };

        if affordable_amount <= 0.0 {
            return (0.0, 0.0);
        }

        let total_value = affordable_amount * price;
        self.inventory[index] += affordable_amount;
        self.cash -= total_value;
        (affordable_amount, total_value)
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
            labor_allocation: source.labor_allocation,
            spice_morale_bonus: source.spice_morale_bonus,
            last_trade_tick: source.last_trade_tick,
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
        assert_eq!(entry.capacities, economy.resource_capacity);
        approx_eq(entry.cash, economy.cash);
        approx_eq(entry.infrastructure_level, economy.infrastructure_level);
        assert_eq!(entry.tick_updated, 42);
        assert!(economy.local_prices[Commodity::Grain.idx()] > BASE_COSTS[Commodity::Grain.idx()]);
    }

    #[test]
    fn sell_to_island_allows_limited_negative_cash() {
        let mut rng = StdRng::seed_from_u64(11);
        let (mut economy, _ledger) = IslandEconomy::new(0, 1, &mut rng);
        economy.cash = 10.0;
        economy.local_prices[Commodity::Tools.idx()] = 120.0;
        let starting_tools = economy.inventory[Commodity::Tools.idx()];
        let expected_price = 120.0 * (1.0 - 0.1 * 0.5);

        // Island has 10.0 cash and allowed overdraft of 800.0.
        let (filled, paid) = economy.sell_to_island(Commodity::Tools, 1.0, 0.1);

        approx_eq(filled, 1.0);
        approx_eq(paid, expected_price);
        approx_eq(economy.cash, 10.0 - expected_price);
        approx_eq(
            economy.inventory[Commodity::Tools.idx()],
            starting_tools + 1.0,
        );
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

    /// Helper to create a deterministic economy with controlled state for governor tests.
    fn make_test_economy() -> (IslandEconomy, PriceLedger) {
        let mut rng = StdRng::seed_from_u64(42);
        let (mut e, ledger) = IslandEconomy::new(0, 1, &mut rng);
        // Reset to known state for predictable tests.
        e.population = 50.0;
        e.inventory = [50.0, 50.0, 50.0, 50.0, 50.0];
        e.production_rates = [1.5, 1.0, 0.8, 0.0, 0.3];
        e.consumption_rates = [1.0, 0.2, 0.2, 0.2, 0.1];
        e.labor_allocation = IslandEconomy::initial_labor_allocation_for(&e.production_rates);
        e.spice_morale_bonus = 1.0;
        (e, ledger)
    }

    #[test]
    fn governor_zeros_nonproduceable() {
        let (mut e, _) = make_test_economy();
        // Tools production_rate is 0, so labor_allocation for Tools must stay 0.
        e.update_labor_allocation();
        approx_eq(e.labor_allocation[Commodity::Tools.idx()], 0.0);
        // Spices has production > 0, so allocation should be > 0.
        assert!(e.labor_allocation[Commodity::Spices.idx()] > 0.0);
    }

    #[test]
    fn governor_prioritizes_grain_when_starving() {
        let (mut e, _) = make_test_economy();
        e.inventory[Commodity::Grain.idx()] = 0.5; // near-zero grain
                                                   // Run governor many times to converge past EMA.
        for _ in 0..100 {
            e.update_labor_allocation();
        }
        let grain_alloc = e.labor_allocation[Commodity::Grain.idx()];
        // Grain should dominate allocation.
        assert!(
            grain_alloc > 0.5,
            "Expected grain > 50%, got {:.1}%",
            grain_alloc * 100.0
        );
    }

    #[test]
    fn governor_tool_chain_boosts_iron_timber() {
        // With plenty of tools.
        let (mut e_tools, _) = make_test_economy();
        e_tools.inventory = [50.0, 50.0, 50.0, 200.0, 50.0];
        for _ in 0..100 {
            e_tools.update_labor_allocation();
        }

        // With zero tools.
        let (mut e_no_tools, _) = make_test_economy();
        e_no_tools.inventory = [50.0, 50.0, 50.0, 0.0, 50.0];
        for _ in 0..100 {
            e_no_tools.update_labor_allocation();
        }

        let iron_with = e_no_tools.labor_allocation[Commodity::Iron.idx()];
        let iron_without = e_tools.labor_allocation[Commodity::Iron.idx()];
        let timber_with = e_no_tools.labor_allocation[Commodity::Timber.idx()];
        let timber_without = e_tools.labor_allocation[Commodity::Timber.idx()];

        // Tool scarcity should boost iron+timber combined allocation.
        assert!(
            (iron_with + timber_with) > (iron_without + timber_without),
            "Tool chain bonus should increase iron+timber: scarce={:.3}+{:.3}, abundant={:.3}+{:.3}",
            iron_with, timber_with, iron_without, timber_without
        );
    }

    #[test]
    fn governor_smoothing() {
        let (mut e, _) = make_test_economy();
        let initial = e.labor_allocation;
        // Drastically change conditions.
        e.inventory[Commodity::Grain.idx()] = 0.1;
        e.update_labor_allocation();
        // After one tick, allocation should have changed but not fully converged.
        let grain_idx = Commodity::Grain.idx();
        assert!(e.labor_allocation[grain_idx] > initial[grain_idx]);
        // But should not yet be at the converged value (EMA smoothing).
        assert!(e.labor_allocation[grain_idx] < 0.9);
    }

    #[test]
    fn spice_morale_boosts_production() {
        let (base, mut ledger) = make_test_economy();
        let mut no_spice = IslandEconomy::clone_for_seeding(&base);
        no_spice.inventory[Commodity::Spices.idx()] = 0.0;
        no_spice.produce_consume_and_price(1.0, 1, &mut ledger);
        let grain_no_spice = no_spice.inventory[Commodity::Grain.idx()];

        let mut with_spice = IslandEconomy::clone_for_seeding(&base);
        let mut ledger2 = ledger.clone();
        with_spice.inventory[Commodity::Spices.idx()] = 200.0;
        with_spice.produce_consume_and_price(1.0, 1, &mut ledger2);
        let grain_with_spice = with_spice.inventory[Commodity::Grain.idx()];

        // With spices, morale bonus should result in more grain produced.
        assert!(
            grain_with_spice > grain_no_spice,
            "Spice morale should boost production: with={grain_with_spice}, without={grain_no_spice}"
        );
    }

    #[test]
    fn labor_allocation_sums_to_one() {
        let (mut e, _) = make_test_economy();
        for _ in 0..50 {
            e.update_labor_allocation();
        }
        let sum: f32 = e.labor_allocation.iter().sum();
        approx_eq(sum, 1.0);
    }
}
