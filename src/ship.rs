use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;

use crate::island::{Island, PriceEntry, PriceLedger, Resource, RESOURCE_COUNT};

const TRADE_LOT_SIZE: f32 = 16.0;
const CONFIDENCE_DECAY_K: f32 = 0.003;
const SPECULATION_FLOOR: f32 = 0.08;
const SPECULATION_STALENESS_SCALE: f32 = 0.35;
const SPECULATION_UNCERTAINTY_BONUS: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub struct Cargo {
    pub resource: Resource,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockAction {
    None,
    Sold,
    Bought,
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
            cash: 200.0,
            ledger: vec![
                PriceEntry {
                    prices: [0.0; RESOURCE_COUNT],
                    tick_updated: 0,
                };
                num_islands
            ],
            last_dock_action: DockAction::None,
        }
    }

    pub fn set_target(&mut self, target_island_id: usize, target: Vec2) {
        self.target = target;
        self.target_island_id = Some(target_island_id);
        self.docked_at = None;
    }

    pub fn docked_island(&self) -> Option<usize> {
        self.docked_at
    }

    pub fn begin_dock_tick(&mut self) {
        self.last_dock_action = DockAction::None;
    }

    pub fn trade_unload_if_carrying(&mut self, island: &mut Island) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }
        if let Some(cargo) = self.cargo.take() {
            let revenue = island.sell_to_island(cargo.resource, cargo.amount);
            self.cash += revenue;
            self.last_dock_action = DockAction::Sold;
        }
        self.last_dock_action
    }

    pub fn trade_load_if_empty(&mut self, island: &mut Island) -> DockAction {
        if self.last_dock_action != DockAction::None {
            return self.last_dock_action;
        }
        if self.cargo.is_some() {
            return self.last_dock_action;
        }

        let mut chosen_resource: Option<Resource> = None;
        let mut lowest_price = f32::INFINITY;
        for resource in Resource::iter() {
            let idx = resource.idx();
            if island.local_prices[idx] < lowest_price {
                lowest_price = island.local_prices[idx];
                chosen_resource = Some(resource);
            }
        }

        if !lowest_price.is_finite() || lowest_price <= 0.0 {
            return self.last_dock_action;
        }

        let Some(chosen_resource) = chosen_resource else {
            return self.last_dock_action;
        };

        let affordable = (self.cash / lowest_price).max(0.0);
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
    ) -> Option<usize> {
        let mut candidates: Vec<(usize, f32, f32)> = Vec::new();
        let mut best_target = None;
        let mut best_utility = f32::NEG_INFINITY;
        let mut best_confidence = 0.0;
        let baseline_price = if let Some(cargo) = self.cargo {
            self.ledger
                .get(current_island_id)
                .map(|entry| entry.prices[cargo.resource.idx()])
                .unwrap_or(0.0)
        } else {
            0.0
        };

        for target_id in 0..self.ledger.len() {
            if target_id == current_island_id {
                continue;
            }
            let distance = if target_id < island_positions.len() {
                (island_positions[target_id] - self.pos).length()
            } else {
                0.0
            };
            let distance_cost = distance * 0.01;
            let transit_time = distance / self.speed.max(1.0);
            let data_age = current_tick.saturating_sub(self.ledger[target_id].tick_updated) as f32;
            let confidence = (-CONFIDENCE_DECAY_K * (data_age + transit_time))
                .exp()
                .clamp(0.05, 1.0);

            let utility = if let Some(cargo) = self.cargo {
                let target_price = self.ledger[target_id].prices[cargo.resource.idx()];
                ((target_price - baseline_price) * cargo.amount * confidence) - distance_cost
            } else {
                let target_best_buy_price = self.ledger[target_id]
                    .prices
                    .iter()
                    .copied()
                    .fold(f32::INFINITY, f32::min)
                    .max(0.0);
                -(target_best_buy_price * confidence * 2.0) - distance_cost
            };

            candidates.push((target_id, utility, confidence));

            if utility > best_utility {
                best_utility = utility;
                best_target = Some(target_id);
                best_confidence = confidence;
            }
        }

        if candidates.is_empty() {
            return best_target;
        }

        let speculation_chance = (SPECULATION_FLOOR
            + (1.0 - best_confidence) * SPECULATION_STALENESS_SCALE)
            .clamp(SPECULATION_FLOOR, 0.60);

        let mut rng = ::rand::thread_rng();
        if rng.gen_bool(speculation_chance as f64) {
            let mut speculative_target = best_target;
            let mut speculative_score = f32::NEG_INFINITY;

            for (target_id, utility, confidence) in candidates {
                let noise = rng.gen_range(-2.0..2.0);
                let score = utility + (1.0 - confidence) * SPECULATION_UNCERTAINTY_BONUS + noise;
                if score > speculative_score {
                    speculative_score = score;
                    speculative_target = Some(target_id);
                }
            }

            return speculative_target;
        }

        best_target
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
            self.pos = self.target;
            self.docked_at = self.target_island_id;
            self.docked_at
        } else {
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
