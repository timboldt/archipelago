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
        for i in 0..len {
            if incoming[i].tick_updated > self.ledger[i].tick_updated {
                self.ledger[i] = incoming[i];
            }
        }
    }

    pub fn copy_ledger_to_ship(&self, ship_ledger: &mut PriceLedger) {
        let len = ship_ledger.len().min(self.ledger.len());
        for i in 0..len {
            if self.ledger[i].tick_updated >= ship_ledger[i].tick_updated {
                ship_ledger[i] = self.ledger[i];
            }
        }
    }

    pub fn draw(&self) {
        draw_circle(self.pos.x, self.pos.y, 20.0, DARKGREEN);
        draw_circle_lines(self.pos.x, self.pos.y, 20.0, 3.0, GREEN);
    }
}
