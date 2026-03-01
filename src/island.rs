use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

pub const RESOURCE_COUNT: usize = 4;
pub const BASE_COSTS: [f32; RESOURCE_COUNT] = [20.0, 30.0, 45.0, 70.0];
pub const BID_PRICE_MULTIPLIER: f32 = 0.95;
pub const ASK_PRICE_MULTIPLIER: f32 = 1.05;
pub const INVENTORY_CARRYING_CAPACITY: f32 = 180.0;
const INITIAL_POPULATION_MIN: f32 = 45.0;
const INITIAL_POPULATION_MAX: f32 = 140.0;
const INITIAL_CASH_MIN: f32 = 900.0;
const INITIAL_CASH_MAX: f32 = 2600.0;
const INITIAL_INFRASTRUCTURE_MIN: f32 = 0.7;
const INITIAL_INFRASTRUCTURE_MAX: f32 = 1.5;
const MIN_POPULATION: f32 = 8.0;
const POPULATION_GROWTH_RATE: f32 = 0.07;
const POPULATION_STARVATION_RATE: f32 = 0.08;
const GRAIN_PER_CAPITA_STABILITY: f32 = 0.07;
const POPULATION_FLOOR_EPSILON: f32 = 0.05;
const GRAIN_SURVIVAL_PRODUCTION_FLOOR: f32 = 1.8;
const SURVIVAL_NON_GRAIN_TO_GRAIN_RATIO: f32 = 0.55;
const TOOL_FABRICATION_BASE_RATE: f32 = 0.28;
const GRAIN_EXTRACTION_BONUS: f32 = 1.35;
const PER_CAPITA_CASH_GENERATION: f32 = 0.22;
const INDUSTRIAL_CASH_GENERATION: f32 = 0.18;
const SCARCITY_LOG_SCALE: f32 = 2.4;
const SCARCITY_REFERENCE: f32 = 120.0;
const SPECIALIZATION_ZERO_PROBABILITY: f32 = 0.35;
const FOCUS_PRODUCTION_BOOST: f32 = 1.9;
const NON_FOCUS_PRODUCTION_SCALE: f32 = 0.78;
const TOOLS_PRODUCTIVITY_CAP: f32 = 2.0;
const TOOLS_PRODUCTIVITY_SCALE: f32 = 0.22;
const CAPITAL_INVESTMENT_THRESHOLD: f32 = 2200.0;
const CAPITAL_INVESTMENT_RATE: f32 = 0.06;
const INFRASTRUCTURE_INVESTMENT_EFFICIENCY: f32 = 0.00016;
const MAX_INFRASTRUCTURE_LEVEL: f32 = 3.5;
const POPULATION_DISPLAY_SCALE: f32 = 150.0;
const CASH_DISPLAY_SCALE: f32 = 800.0;
const INFRASTRUCTURE_DISPLAY_MAX: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
#[repr(usize)]
pub enum Resource {
    Grain,
    Timber,
    Iron,
    Tools,
}

impl Resource {
    pub fn idx(self) -> usize {
        self as usize
    }
}

pub type Inventory = [f32; RESOURCE_COUNT];

#[derive(Clone, Copy, Debug)]
pub struct PriceEntry {
    pub prices: [f32; RESOURCE_COUNT],
    pub inventories: [f32; RESOURCE_COUNT],
    pub cash: f32,
    pub tick_updated: u64,
    pub last_seen_tick: u64,
}

pub type PriceLedger = Vec<PriceEntry>;

pub struct Island {
    #[allow(dead_code)]
    pub id: usize,
    pub pos: Vec2,
    pub inventory: Inventory,
    pub production_rates: Inventory,
    pub consumption_rates: Inventory,
    pub population: f32,
    pub cash: f32,
    pub infrastructure_level: f32,
    pub local_prices: [f32; RESOURCE_COUNT],
    pub ledger: PriceLedger,
}

impl Island {
    pub fn new(id: usize, pos: Vec2, num_islands: usize, rng: &mut impl Rng) -> Self {
        let mut inventory = [0.0; RESOURCE_COUNT];
        let mut production_rates = [0.0; RESOURCE_COUNT];
        let mut consumption_rates = [0.0; RESOURCE_COUNT];

        for resource in Resource::iter() {
            let index = resource.idx();
            inventory[index] = rng.gen_range(25.0..125.0);
            production_rates[index] = match resource {
                Resource::Tools => 0.0,
                Resource::Grain => rng.gen_range(0.8..2.6),
                Resource::Timber | Resource::Iron => {
                    if rng.gen_bool(SPECIALIZATION_ZERO_PROBABILITY as f64) {
                        0.0
                    } else {
                        rng.gen_range(0.4..2.0)
                    }
                }
            };
            consumption_rates[index] = rng.gen_range(0.4..1.9);
        }

        if production_rates[Resource::Timber.idx()] <= 0.0
            && production_rates[Resource::Iron.idx()] <= 0.0
        {
            if rng.gen_bool(0.5) {
                production_rates[Resource::Timber.idx()] = rng.gen_range(0.5..1.4);
            } else {
                production_rates[Resource::Iron.idx()] = rng.gen_range(0.5..1.4);
            }
        }

        let focus_resource = match rng.gen_range(0..3) {
            0 => Resource::Grain,
            1 => Resource::Timber,
            _ => Resource::Iron,
        };
        for resource in [Resource::Grain, Resource::Timber, Resource::Iron] {
            let index = resource.idx();
            if resource == focus_resource {
                production_rates[index] *= FOCUS_PRODUCTION_BOOST;
            } else {
                production_rates[index] *= NON_FOCUS_PRODUCTION_SCALE;
            }
        }

        let mut island = Self {
            id,
            pos,
            inventory,
            production_rates,
            consumption_rates,
            population: rng.gen_range(INITIAL_POPULATION_MIN..INITIAL_POPULATION_MAX),
            cash: rng.gen_range(INITIAL_CASH_MIN..INITIAL_CASH_MAX),
            infrastructure_level: rng
                .gen_range(INITIAL_INFRASTRUCTURE_MIN..INITIAL_INFRASTRUCTURE_MAX),
            local_prices: [0.0; RESOURCE_COUNT],
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
        };
        island.recompute_local_prices(0);
        island
    }

    pub fn produce_consume_and_price(&mut self, dt: f32, tick: u64) {
        let grain_idx = Resource::Grain.idx();
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
        self.population = self.population.max(MIN_POPULATION);

        for resource in Resource::iter() {
            let index = resource.idx();
            let inventory = self.inventory[index];
            let logistic_factor =
                (1.0 - (inventory / INVENTORY_CARRYING_CAPACITY)).clamp(0.0, 1.0);

            if resource != Resource::Tools {
                let tools_boost =
                    (1.0 + self.inventory[Resource::Tools.idx()] * TOOLS_PRODUCTIVITY_SCALE)
                        .clamp(1.0, TOOLS_PRODUCTIVITY_CAP);
                let mut extraction =
                    self.production_rates[index] * self.population * logistic_factor * dt;
                if resource == Resource::Grain {
                    extraction *= GRAIN_EXTRACTION_BONUS;
                }
                extraction *= tools_boost;
                self.inventory[index] += extraction;
            }

            let demand = self.consumption_rates[index] * self.population * dt;
            self.inventory[index] -= demand;
            self.inventory[index] = self.inventory[index].max(0.0);
        }

        let iron_idx = Resource::Iron.idx();
        let timber_idx = Resource::Timber.idx();
        let tools_idx = Resource::Tools.idx();

        let industrial_rate = TOOL_FABRICATION_BASE_RATE * self.infrastructure_level * dt;
        let feasible_batch = self.inventory[iron_idx]
            .min(self.inventory[timber_idx])
            .min(industrial_rate)
            .max(0.0);
        if feasible_batch > 0.0 {
            self.inventory[iron_idx] -= feasible_batch;
            self.inventory[timber_idx] -= feasible_batch;
            self.inventory[tools_idx] += feasible_batch * 1.5;
        }

        let local_economic_income =
            (self.population * PER_CAPITA_CASH_GENERATION + feasible_batch * INDUSTRIAL_CASH_GENERATION)
                * dt;
        self.cash += local_economic_income.max(0.0);

        if self.cash > CAPITAL_INVESTMENT_THRESHOLD {
            let excess_capital = self.cash - CAPITAL_INVESTMENT_THRESHOLD;
            let investment = (excess_capital * CAPITAL_INVESTMENT_RATE * dt).min(self.cash);
            self.cash -= investment;
            self.infrastructure_level = (self.infrastructure_level
                + investment * INFRASTRUCTURE_INVESTMENT_EFFICIENCY)
                .min(MAX_INFRASTRUCTURE_LEVEL);
        }

        self.recompute_local_prices(tick);
    }

    fn reset_survival_focus(&mut self) {
        let grain_idx = Resource::Grain.idx();
        let timber_idx = Resource::Timber.idx();
        let iron_idx = Resource::Iron.idx();

        self.production_rates[grain_idx] =
            self.production_rates[grain_idx].max(GRAIN_SURVIVAL_PRODUCTION_FLOOR);

        let non_grain_ceiling = self.production_rates[grain_idx] * SURVIVAL_NON_GRAIN_TO_GRAIN_RATIO;
        self.production_rates[timber_idx] = self.production_rates[timber_idx].min(non_grain_ceiling);
        self.production_rates[iron_idx] = self.production_rates[iron_idx].min(non_grain_ceiling);
    }

    pub fn recompute_local_prices(&mut self, tick: u64) {
        for resource in Resource::iter() {
            let index = resource.idx();
            let inventory = self.inventory[index].max(0.0);
            let scarcity_pressure = (SCARCITY_REFERENCE / (inventory + 1.0)).ln_1p();
            self.local_prices[index] =
                BASE_COSTS[index] * (1.0 + SCARCITY_LOG_SCALE * scarcity_pressure);
        }
        if let Some(entry) = self.ledger.get_mut(self.id) {
            entry.prices = self.local_prices;
            entry.inventories = self.inventory;
            entry.cash = self.cash;
            entry.tick_updated = tick;
        }
    }

    pub fn mark_seen(&mut self, tick: u64) {
        if let Some(entry) = self.ledger.get_mut(self.id) {
            entry.last_seen_tick = tick;
        }
    }

    pub fn sell_to_island(&mut self, resource: Resource, amount: f32) -> (f32, f32) {
        if amount <= 0.0 {
            return (0.0, 0.0);
        }
        let index = resource.idx();
        let price = self.bid_price(resource);
        if !price.is_finite() || price <= 0.0 || self.cash <= 0.0 {
            return (0.0, 0.0);
        }

        let affordable = (self.cash / price).max(0.0);
        let filled = amount.min(affordable);
        let total_value = filled * price;
        if filled <= 0.0 || total_value <= 0.0 {
            return (0.0, 0.0);
        }

        self.inventory[index] += filled;
        self.cash -= total_value;
        (filled, total_value)
    }

    pub fn buy_from_island(&mut self, resource: Resource, requested_amount: f32) -> (f32, f32) {
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
        let total_cost = filled * self.ask_price(resource);
        self.cash += total_cost;
        (filled, total_cost)
    }

    pub fn bid_price(&self, resource: Resource) -> f32 {
        self.local_prices[resource.idx()] * BID_PRICE_MULTIPLIER
    }

    pub fn ask_price(&self, resource: Resource) -> f32 {
        self.local_prices[resource.idx()] * ASK_PRICE_MULTIPLIER
    }

    pub fn merge_ledger(&mut self, incoming: &PriceLedger) {
        let len = self.ledger.len().min(incoming.len());
        for (i, incoming_entry) in incoming.iter().copied().enumerate().take(len) {
            if i == self.id {
                continue;
            }
            if incoming_entry.tick_updated > self.ledger[i].tick_updated {
                self.ledger[i].prices = incoming_entry.prices;
                self.ledger[i].inventories = incoming_entry.inventories;
                self.ledger[i].cash = incoming_entry.cash;
                self.ledger[i].tick_updated = incoming_entry.tick_updated;
            }
            if incoming_entry.last_seen_tick > self.ledger[i].last_seen_tick {
                self.ledger[i].last_seen_tick = incoming_entry.last_seen_tick;
            }
        }
    }

    pub fn copy_ledger_to_ship(&self, ship_ledger: &mut PriceLedger) {
        let len = ship_ledger.len().min(self.ledger.len());
        for (i, ship_entry) in ship_ledger.iter_mut().enumerate().take(len) {
            if self.ledger[i].tick_updated >= ship_entry.tick_updated {
                ship_entry.prices = self.ledger[i].prices;
                ship_entry.inventories = self.ledger[i].inventories;
                ship_entry.cash = self.ledger[i].cash;
                ship_entry.tick_updated = self.ledger[i].tick_updated;
            }
            if self.ledger[i].last_seen_tick >= ship_entry.last_seen_tick {
                ship_entry.last_seen_tick = self.ledger[i].last_seen_tick;
            }
        }
    }

    pub fn draw(&self, world_units_per_pixel: f32) {
        let chart_width = 20.0 * world_units_per_pixel;
        let chart_height = 14.0 * world_units_per_pixel;
        let bar_width = 4.0 * world_units_per_pixel;
        let bar_gap = 1.0 * world_units_per_pixel;
        let panel_padding = 2.0 * world_units_per_pixel;
        let border_thickness = 1.0 * world_units_per_pixel;
        let status_gap = 2.0 * world_units_per_pixel;
        let status_row_height = 2.0 * world_units_per_pixel;
        let status_row_spacing = 1.0 * world_units_per_pixel;
        let origin_x = self.pos.x - chart_width * 0.5;
        let origin_y = self.pos.y - chart_height * 0.5;
        let frame_x = origin_x - panel_padding;
        let frame_y = origin_y - panel_padding;
        let frame_w = chart_width + panel_padding * 2.0;
        let frame_h = chart_height + panel_padding * 2.0;
        let status_panel_h = panel_padding * 2.0 + status_row_height * 3.0 + status_row_spacing * 2.0;
        let status_panel_y = frame_y + frame_h + status_gap;

        draw_rectangle(
            frame_x,
            frame_y,
            frame_w,
            frame_h,
            Color::from_rgba(12, 24, 40, 180),
        );

        draw_rectangle(frame_x, frame_y, frame_w, border_thickness, WHITE);
        draw_rectangle(
            frame_x,
            frame_y + frame_h - border_thickness,
            frame_w,
            border_thickness,
            WHITE,
        );
        draw_rectangle(frame_x, frame_y, border_thickness, frame_h, WHITE);
        draw_rectangle(
            frame_x + frame_w - border_thickness,
            frame_y,
            border_thickness,
            frame_h,
            WHITE,
        );

        let max_inventory = self
            .inventory
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .max(1.0);

        for (bar_index, resource) in Resource::iter().enumerate() {
            let value = self.inventory[resource.idx()].max(0.0);
            let normalized = (value / max_inventory).clamp(0.0, 1.0);
            let mut bar_height = normalized * chart_height;
            if value > 0.0 {
                bar_height = bar_height.max(1.0 * world_units_per_pixel);
            }
            let x = origin_x + bar_index as f32 * (bar_width + bar_gap);
            let y = origin_y + chart_height - bar_height;

            let color = match resource {
                Resource::Grain => YELLOW,
                Resource::Timber => GREEN,
                Resource::Iron => DARKGRAY,
                Resource::Tools => RED,
            };

            draw_rectangle(x, y, bar_width, bar_height, color);
        }

        draw_rectangle(
            frame_x,
            status_panel_y,
            frame_w,
            status_panel_h,
            Color::from_rgba(12, 24, 40, 180),
        );

        draw_rectangle(frame_x, status_panel_y, frame_w, border_thickness, WHITE);
        draw_rectangle(
            frame_x,
            status_panel_y + status_panel_h - border_thickness,
            frame_w,
            border_thickness,
            WHITE,
        );
        draw_rectangle(frame_x, status_panel_y, border_thickness, status_panel_h, WHITE);
        draw_rectangle(
            frame_x + frame_w - border_thickness,
            status_panel_y,
            border_thickness,
            status_panel_h,
            WHITE,
        );

        let pop_fill = (self.population / (self.population + POPULATION_DISPLAY_SCALE))
            .clamp(0.0, 1.0);
        let cash_fill = (self.cash / (self.cash + CASH_DISPLAY_SCALE)).clamp(0.0, 1.0);
        let infra_fill = (self.infrastructure_level / INFRASTRUCTURE_DISPLAY_MAX).clamp(0.0, 1.0);

        let status_inner_x = frame_x + panel_padding;
        let status_inner_w = (frame_w - panel_padding * 2.0).max(0.0);
        let row1_y = status_panel_y + panel_padding;
        let row2_y = row1_y + status_row_height + status_row_spacing;
        let row3_y = row2_y + status_row_height + status_row_spacing;

        draw_rectangle(status_inner_x, row1_y, status_inner_w, status_row_height, DARKGRAY);
        draw_rectangle(
            status_inner_x,
            row1_y,
            status_inner_w * pop_fill,
            status_row_height,
            SKYBLUE,
        );

        draw_rectangle(status_inner_x, row2_y, status_inner_w, status_row_height, DARKGRAY);
        draw_rectangle(
            status_inner_x,
            row2_y,
            status_inner_w * cash_fill,
            status_row_height,
            GOLD,
        );

        draw_rectangle(status_inner_x, row3_y, status_inner_w, status_row_height, DARKGRAY);
        draw_rectangle(
            status_inner_x,
            row3_y,
            status_inner_w * infra_fill,
            status_row_height,
            ORANGE,
        );
    }
}
