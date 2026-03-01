use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

pub const RESOURCE_COUNT: usize = 4;
pub const BASE_COSTS: [f32; RESOURCE_COUNT] = [20.0, 30.0, 45.0, 70.0];

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
            production_rates[index] = rng.gen_range(0.5..2.2);
            consumption_rates[index] = rng.gen_range(0.4..1.9);
        }

        let mut island = Self {
            id,
            pos,
            inventory,
            production_rates,
            consumption_rates,
            local_prices: [0.0; RESOURCE_COUNT],
            ledger: vec![
                PriceEntry {
                    prices: [0.0; RESOURCE_COUNT],
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
        for resource in Resource::iter() {
            let index = resource.idx();
            self.inventory[index] += self.production_rates[index] * dt;
            self.inventory[index] -= self.consumption_rates[index] * dt;
            self.inventory[index] = self.inventory[index].max(0.0);
        }
        self.recompute_local_prices(tick);
    }

    pub fn recompute_local_prices(&mut self, tick: u64) {
        for resource in Resource::iter() {
            let index = resource.idx();
            self.local_prices[index] = BASE_COSTS[index] / (self.inventory[index] + 1.0);
        }
        if let Some(entry) = self.ledger.get_mut(self.id) {
            entry.prices = self.local_prices;
            entry.tick_updated = tick;
        }
    }

    pub fn mark_seen(&mut self, tick: u64) {
        if let Some(entry) = self.ledger.get_mut(self.id) {
            entry.last_seen_tick = tick;
        }
    }

    pub fn sell_to_island(&mut self, resource: Resource, amount: f32) -> f32 {
        if amount <= 0.0 {
            return 0.0;
        }
        let index = resource.idx();
        let price = self.local_prices[index];
        self.inventory[index] += amount;
        amount * price
    }

    pub fn buy_from_island(&mut self, resource: Resource, requested_amount: f32) -> (f32, f32) {
        if requested_amount <= 0.0 {
            return (0.0, 0.0);
        }
        let index = resource.idx();
        let available = self.inventory[index].max(0.0);
        let filled = requested_amount.min(available);
        self.inventory[index] -= filled;
        let total_cost = filled * self.local_prices[index];
        (filled, total_cost)
    }

    pub fn merge_ledger(&mut self, incoming: &PriceLedger) {
        let len = self.ledger.len().min(incoming.len());
        for (i, incoming_entry) in incoming.iter().copied().enumerate().take(len) {
            if incoming_entry.tick_updated > self.ledger[i].tick_updated {
                self.ledger[i].prices = incoming_entry.prices;
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
        let origin_x = self.pos.x - chart_width * 0.5;
        let origin_y = self.pos.y - chart_height * 0.5;
        let frame_x = origin_x - panel_padding;
        let frame_y = origin_y - panel_padding;
        let frame_w = chart_width + panel_padding * 2.0;
        let frame_h = chart_height + panel_padding * 2.0;

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
    }
}
