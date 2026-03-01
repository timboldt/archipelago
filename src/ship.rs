use ::rand::Rng;
use macroquad::prelude::*;
use strum::IntoEnumIterator;

use crate::island::{Island, PriceEntry, PriceLedger, Resource, RESOURCE_COUNT};

const TRADE_LOT_SIZE: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
pub struct PlanningTuning {
    pub confidence_decay_k: f32,
    pub speculation_floor: f32,
    pub speculation_staleness_scale: f32,
    pub speculation_uncertainty_bonus: f32,
    pub learning_rate: f32,
    pub learning_decay: f32,
    pub learning_weight: f32,
    pub congestion_penalty: f32,
    pub congestion_exponent: f32,
    pub route_congestion_decay: f32,
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
            congestion_penalty: 4.0,
            congestion_exponent: 1.15,
            route_congestion_decay: 0.94,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PurchaseRecord {
    unit_price: f32,
    resource: Resource,
}

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
    route_memory: Vec<f32>,
    last_purchase: Option<PurchaseRecord>,
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
            route_memory: vec![0.0; num_islands],
            last_purchase: None,
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

    pub fn begin_dock_tick(&mut self, tuning: &PlanningTuning) {
        self.last_dock_action = DockAction::None;
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
        if let Some(cargo) = self.cargo.take() {
            let revenue = island.sell_to_island(cargo.resource, cargo.amount);
            self.cash += revenue;

            if let Some(purchase) = self.last_purchase.take() {
                if purchase.resource == cargo.resource
                    && cargo.amount > 0.0
                    && island_id < self.route_memory.len()
                {
                    let sale_unit_price = revenue / cargo.amount;
                    let normalized_margin =
                        (sale_unit_price - purchase.unit_price) / (purchase.unit_price + 1.0);
                    self.route_memory[island_id] += normalized_margin * tuning.learning_rate;
                    self.route_memory[island_id] = self.route_memory[island_id].clamp(-1.5, 1.5);
                }
            }

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
        self.last_purchase = Some(PurchaseRecord {
            unit_price: lowest_price,
            resource: chosen_resource,
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
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
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
            let confidence = (-tuning.confidence_decay_k * (data_age + transit_time))
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

            let learned_bias =
                self.route_memory.get(target_id).copied().unwrap_or(0.0) * tuning.learning_weight;
            let crowd_count = outbound_recent_departures
                .get(target_id)
                .copied()
                .unwrap_or(0.0)
                .max(0.0);
            let congestion_cost = if crowd_count > 0.0 {
                tuning.congestion_penalty * crowd_count.powf(tuning.congestion_exponent)
            } else {
                0.0
            };

            let utility = utility + learned_bias - congestion_cost;

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

        let best_congestion = best_target
            .and_then(|target_id| outbound_recent_departures.get(target_id).copied())
            .unwrap_or(0.0)
            .max(0.0);
        let congestion_boost = (best_congestion * 0.03).min(0.20);

        let speculation_chance = (tuning.speculation_floor
            + (1.0 - best_confidence) * tuning.speculation_staleness_scale
            + congestion_boost)
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
