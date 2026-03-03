//! Core world state and simulation orchestration.
//!
//! `World` owns islands/ships, advances the simulation in fixed phases,
//! and delegates rendering to specialized submodules.

use ::rand::Rng;
use macroquad::prelude::*;
use rayon::prelude::*;
use std::time::Instant;

use crate::island::Island;
use crate::ship::{PlanningTuning, Ship, STARTING_CASH};

mod docking;
mod hud;
mod ui;
mod view_model;

/// Width/height of the square simulation space in world units.
pub const WORLD_SIZE: f32 = 5000.0;
const ISLAND_SPAWN_MARGIN: f32 = 200.0;
const MIN_ISLAND_SPAWN_DISTANCE: f32 = 140.0;
const ISLAND_POSITION_ATTEMPTS: usize = 40;
const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;
const SCUTTLE_THRESHOLD_MULTIPLIER: f32 = 0.35;
const BIRTH_THRESHOLD_MULTIPLIER: f32 = 5.0;
const BIRTH_FEE_MULTIPLIER: f32 = 1.5;
const TARGET_SHIPS_PER_ISLAND: f32 = 12.0;
const LIFECYCLE_CHECK_INTERVAL_TICKS: u64 = 30;
const MUTATION_STRENGTH: f32 = 0.05;
const MAX_DOCK_SETTLEMENT_STEPS: usize = 3;
const PERF_HUD_UPDATE_INTERVAL_SECS: f32 = 1.0;
const STARTING_SIM_TICK: u64 = 500;

#[derive(Clone, Copy, Default)]
/// Aggregated phase timings used by the performance HUD.
struct FrameTimings {
    economy_ms: f32,
    movement_ms: f32,
    dock_ms: f32,
    friction_ms: f32,
    total_ms: f32,
}

impl FrameTimings {
    /// Accumulates timing values from another sample.
    fn add_assign(&mut self, other: &FrameTimings) {
        self.economy_ms += other.economy_ms;
        self.movement_ms += other.movement_ms;
        self.dock_ms += other.dock_ms;
        self.friction_ms += other.friction_ms;
        self.total_ms += other.total_ms;
    }

    /// Returns a copy with each timing field multiplied by `scale`.
    fn scaled(&self, scale: f32) -> Self {
        Self {
            economy_ms: self.economy_ms * scale,
            movement_ms: self.movement_ms * scale,
            dock_ms: self.dock_ms * scale,
            friction_ms: self.friction_ms * scale,
            total_ms: self.total_ms * scale,
        }
    }
}

/// Central simulation container for islands, ships, selection state, and timing.
pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Option<Ship>>,
    selected_ship_index: usize,
    selected_island_index: usize,
    recent_route_departures: Vec<Vec<f32>>,
    route_departure_history: Vec<Vec<Vec<u16>>>,
    route_history_cursor: usize,
    planning_tuning: PlanningTuning,
    tick: u64,
    frame_timings: FrameTimings,
    frame_timings_accum: FrameTimings,
    frame_timings_samples: u32,
    perf_hud_elapsed_secs: f32,
}

impl World {
    /// Creates a new world with randomly placed islands and seeded ships.
    pub fn new(num_islands: usize, num_ships: usize) -> Self {
        let mut rng = ::rand::thread_rng();

        let mut island_positions: Vec<Vec2> = Vec::with_capacity(num_islands);
        for _ in 0..num_islands {
            let mut best_candidate = vec2(
                rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
                rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
            );
            let mut best_min_distance = island_positions
                .iter()
                .map(|existing| best_candidate.distance(*existing))
                .fold(f32::INFINITY, f32::min);

            for _ in 0..ISLAND_POSITION_ATTEMPTS {
                let candidate = vec2(
                    rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
                    rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
                );
                let min_distance = island_positions
                    .iter()
                    .map(|existing| candidate.distance(*existing))
                    .fold(f32::INFINITY, f32::min);

                if min_distance >= MIN_ISLAND_SPAWN_DISTANCE {
                    best_candidate = candidate;
                    break;
                }

                if min_distance > best_min_distance {
                    best_min_distance = min_distance;
                    best_candidate = candidate;
                }
            }

            island_positions.push(best_candidate);
        }

        let islands: Vec<Island> = island_positions
            .into_iter()
            .enumerate()
            .map(|(id, pos)| Island::new(id, pos, num_islands, &mut rng))
            .collect();

        // Ships start docked at a random island with randomized speeds and
        // noisy/stale beliefs about all islands.
        let ships: Vec<Option<Ship>> = (0..num_ships)
            .map(|i| {
                let speed = rng.gen_range(200.0_f32..500.0);
                let start_island_id = i % islands.len();
                let mut ship = Ship::new(
                    islands[start_island_id].pos,
                    speed,
                    num_islands,
                    start_island_id,
                );
                ship.seed_initial_market_view(
                    &islands,
                    STARTING_SIM_TICK,
                    start_island_id,
                    &mut rng,
                );
                Some(ship)
            })
            .collect();

        // Ships start docked and will load/plan before first departure.
        Self {
            islands,
            ships,
            selected_ship_index: 0,
            selected_island_index: 0,
            recent_route_departures: vec![vec![0.0; num_islands]; num_islands],
            route_departure_history: vec![
                vec![vec![0; num_islands]; num_islands];
                ROUTE_HISTORY_WINDOW_TICKS
            ],
            route_history_cursor: 0,
            planning_tuning: PlanningTuning::default(),
            tick: STARTING_SIM_TICK,
            frame_timings: FrameTimings::default(),
            frame_timings_accum: FrameTimings::default(),
            frame_timings_samples: 0,
            perf_hud_elapsed_secs: 0.0,
        }
    }

    /// Updates environmental planning parameters used by ship logic.
    pub fn set_planning_tuning(&mut self, planning_tuning: PlanningTuning) {
        self.planning_tuning = planning_tuning;
    }

    /// Handles runtime selection input for ships and islands.
    pub fn handle_input(&mut self) {
        let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if is_key_pressed(KeyCode::LeftBracket) {
            if shift_down {
                self.select_previous_island();
            } else {
                self.select_previous_ship();
            }
        }
        if is_key_pressed(KeyCode::RightBracket) {
            if shift_down {
                self.select_next_island();
            } else {
                self.select_next_ship();
            }
        }
    }

    /// Computes effective tuning after applying crowding-dependent friction.
    fn environmental_tuning(&self) -> PlanningTuning {
        let island_count = self.islands.len().max(1) as f32;
        let target_population = (island_count * TARGET_SHIPS_PER_ISLAND).max(1.0);
        let crowding_factor = (self.active_ship_count() as f32 / target_population).max(0.35);

        let mut tuning = self.planning_tuning;
        tuning.global_friction_mult *= crowding_factor;
        tuning
    }

    /// Returns the number of non-empty ship slots.
    fn active_ship_count(&self) -> usize {
        self.ships.iter().filter(|ship| ship.is_some()).count()
    }

    /// Returns the first active ship slot index, if any.
    fn first_active_ship_index(&self) -> Option<usize> {
        self.ships.iter().position(|ship| ship.is_some())
    }

    /// Finds the next active ship index from `from`, wrapping around the fleet.
    fn find_next_active_ship_index(&self, from: usize, forward: bool) -> Option<usize> {
        if self.ships.is_empty() {
            return None;
        }

        let len = self.ships.len();
        let mut idx = from % len;
        for _ in 0..len {
            idx = if forward {
                (idx + 1) % len
            } else {
                (idx + len - 1) % len
            };
            if self.ships[idx].is_some() {
                return Some(idx);
            }
        }
        None
    }

    /// Repairs the selected ship index after culls/spawns/slot invalidation.
    fn ensure_selected_ship_valid(&mut self) {
        if self.ships.is_empty() {
            self.selected_ship_index = 0;
            return;
        }

        if self.selected_ship_index >= self.ships.len()
            || self.ships[self.selected_ship_index].is_none()
        {
            self.selected_ship_index = self.first_active_ship_index().unwrap_or(0);
        }
    }

    /// Selects the next active ship, wrapping around.
    pub fn select_next_ship(&mut self) {
        if self.active_ship_count() == 0 {
            self.selected_ship_index = 0;
            return;
        }
        self.ensure_selected_ship_valid();
        self.selected_ship_index = self
            .find_next_active_ship_index(self.selected_ship_index, true)
            .unwrap_or(self.selected_ship_index);
    }

    /// Selects the previous active ship, wrapping around.
    pub fn select_previous_ship(&mut self) {
        if self.active_ship_count() == 0 {
            self.selected_ship_index = 0;
            return;
        }
        self.ensure_selected_ship_valid();
        self.selected_ship_index = self
            .find_next_active_ship_index(self.selected_ship_index, false)
            .unwrap_or(self.selected_ship_index);
    }

    /// Selects the next island, wrapping around.
    pub fn select_next_island(&mut self) {
        if self.islands.is_empty() {
            self.selected_island_index = 0;
            return;
        }
        self.selected_island_index = (self.selected_island_index + 1) % self.islands.len();
    }

    /// Selects the previous island, wrapping around.
    pub fn select_previous_island(&mut self) {
        if self.islands.is_empty() {
            self.selected_island_index = 0;
            return;
        }
        if self.selected_island_index == 0 {
            self.selected_island_index = self.islands.len() - 1;
        } else {
            self.selected_island_index -= 1;
        }
    }

    /// Advances the simulation by `dt` seconds using a fixed phase order.
    pub fn update(&mut self, dt: f32) {
        let frame_start = Instant::now();
        let mut current_frame_timings = FrameTimings::default();
        self.ensure_selected_ship_valid();
        if self.islands.is_empty() {
            self.selected_island_index = 0;
        } else if self.selected_island_index >= self.islands.len() {
            self.selected_island_index = self.islands.len() - 1;
        }

        self.tick = self.tick.saturating_add(1);
        self.begin_route_history_tick();

        let phase_start = Instant::now();
        self.update_island_economy(dt);
        current_frame_timings.economy_ms = phase_start.elapsed().as_secs_f32() * 1000.0;

        let phase_start = Instant::now();
        self.move_ships(dt);
        current_frame_timings.movement_ms = phase_start.elapsed().as_secs_f32() * 1000.0;

        let phase_start = Instant::now();
        self.process_docked_ships();
        current_frame_timings.dock_ms = phase_start.elapsed().as_secs_f32() * 1000.0;

        let phase_start = Instant::now();
        self.apply_maritime_friction(dt);
        current_frame_timings.friction_ms = phase_start.elapsed().as_secs_f32() * 1000.0;

        self.route_history_cursor = (self.route_history_cursor + 1) % ROUTE_HISTORY_WINDOW_TICKS;
        if self.tick.is_multiple_of(LIFECYCLE_CHECK_INTERVAL_TICKS) {
            self.evolve_fleet();
        }
        current_frame_timings.total_ms = frame_start.elapsed().as_secs_f32() * 1000.0;

        self.frame_timings_accum.add_assign(&current_frame_timings);
        self.frame_timings_samples = self.frame_timings_samples.saturating_add(1);
        self.perf_hud_elapsed_secs += dt.max(0.0);
        if self.perf_hud_elapsed_secs >= PERF_HUD_UPDATE_INTERVAL_SECS
            && self.frame_timings_samples > 0
        {
            let inv_samples = 1.0 / self.frame_timings_samples as f32;
            self.frame_timings = self.frame_timings_accum.scaled(inv_samples);
            self.frame_timings_accum = FrameTimings::default();
            self.frame_timings_samples = 0;
            self.perf_hud_elapsed_secs = 0.0;
        }
    }

    /// Expires route-history contributions that rolled out of the window.
    fn begin_route_history_tick(&mut self) {
        let cursor = self.route_history_cursor;
        for origin_id in 0..self.recent_route_departures.len() {
            for target_id in 0..self.recent_route_departures[origin_id].len() {
                let stale_count = self.route_departure_history[cursor][origin_id][target_id] as f32;
                if stale_count > 0.0 {
                    self.recent_route_departures[origin_id][target_id] =
                        (self.recent_route_departures[origin_id][target_id] - stale_count).max(0.0);
                    self.route_departure_history[cursor][origin_id][target_id] = 0;
                }
            }
        }
    }

    /// Runs island production/consumption/pricing updates in parallel.
    fn update_island_economy(&mut self, dt: f32) {
        let tick = self.tick;
        self.islands
            .par_iter_mut()
            .for_each(|island| island.produce_consume_and_price(dt, tick));
    }

    /// Advances ship movement in parallel.
    fn move_ships(&mut self, dt: f32) {
        self.ships.par_iter_mut().for_each(|slot| {
            if let Some(ship) = slot.as_mut() {
                let _ = ship.update(dt);
            }
        });
    }

    /// Applies continuous maritime operating friction to ships in parallel.
    fn apply_maritime_friction(&mut self, dt: f32) {
        let global_friction_mult = self.environmental_tuning().global_friction_mult;
        self.ships.par_iter_mut().for_each(|slot| {
            if let Some(ship) = slot.as_mut() {
                ship.apply_maritime_friction(dt, global_friction_mult);
            }
        });
    }

    /// Applies periodic selection pressure: cull weak ships and spawn daughters.
    fn evolve_fleet(&mut self) {
        let scuttle_threshold = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER;
        let island_count = self.islands.len().max(1) as f32;
        let fleet_pressure =
            (self.active_ship_count() as f32 / (island_count * TARGET_SHIPS_PER_ISLAND)).max(1.0);
        let cost_factor = self
            .environmental_tuning()
            .global_friction_mult
            .clamp(0.2, 6.0);
        let birth_threshold =
            STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER * cost_factor * fleet_pressure;
        let birth_fee = STARTING_CASH * BIRTH_FEE_MULTIPLIER * cost_factor * fleet_pressure;
        let mut rng = ::rand::thread_rng();

        let mut daughters: Vec<Option<Ship>> = Vec::new();

        for slot in &mut self.ships {
            let Some(ship) = slot.as_mut() else {
                continue;
            };

            if ship.estimated_net_worth() < scuttle_threshold {
                *slot = None;
                continue;
            }

            if ship.cash >= birth_threshold + birth_fee {
                if let Some(daughter) = ship.spawn_daughter(MUTATION_STRENGTH, &mut rng) {
                    ship.cash -= birth_fee;
                    daughters.push(Some(daughter));
                }
            }
        }

        self.ships.extend(daughters);
        self.ensure_selected_ship_valid();
    }

    /// Draws world-space simulation entities and selection overlays.
    pub fn draw(&self) {
        let world_units_per_pixel_x = WORLD_SIZE / screen_width().max(1.0);
        let world_units_per_pixel_y = WORLD_SIZE / screen_height().max(1.0);
        let world_units_per_pixel = world_units_per_pixel_x.max(world_units_per_pixel_y);
        ui::draw_world(self, world_units_per_pixel);
    }

    /// Draws screen-space HUD and inspector panels.
    pub fn draw_ui(&self) {
        hud::draw_ui(self);
    }
}
