use ::rand::Rng;
use macroquad::prelude::*;
use rayon::prelude::*;
use std::time::Instant;

use crate::island::{Island, Resource, RESOURCE_COUNT};
use crate::ship::{
    LoadPlanningContext, PlanningTuning, Ship, ShipArchetype, STARTING_CASH,
};

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

#[derive(Clone, Copy, Default)]
struct FrameTimings {
    economy_ms: f32,
    movement_ms: f32,
    dock_ms: f32,
    friction_ms: f32,
    total_ms: f32,
}

impl FrameTimings {
    fn add_assign(&mut self, other: &FrameTimings) {
        self.economy_ms += other.economy_ms;
        self.movement_ms += other.movement_ms;
        self.dock_ms += other.dock_ms;
        self.friction_ms += other.friction_ms;
        self.total_ms += other.total_ms;
    }

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

        // Ships start docked at a random island with randomised speeds.
        let ships: Vec<Option<Ship>> = (0..num_ships)
            .map(|i| {
                let speed = rng.gen_range(200.0_f32..500.0);
                let start_island_id = i % islands.len();
                Some(Ship::new(
                    islands[start_island_id].pos,
                    speed,
                    num_islands,
                    start_island_id,
                ))
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
            tick: 0,
            frame_timings: FrameTimings::default(),
            frame_timings_accum: FrameTimings::default(),
            frame_timings_samples: 0,
            perf_hud_elapsed_secs: 0.0,
        }
    }

    pub fn set_planning_tuning(&mut self, planning_tuning: PlanningTuning) {
        self.planning_tuning = planning_tuning;
    }

    fn active_ship_count(&self) -> usize {
        self.ships.iter().filter(|ship| ship.is_some()).count()
    }

    fn first_active_ship_index(&self) -> Option<usize> {
        self.ships.iter().position(|ship| ship.is_some())
    }

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

    pub fn select_next_island(&mut self) {
        if self.islands.is_empty() {
            self.selected_island_index = 0;
            return;
        }
        self.selected_island_index = (self.selected_island_index + 1) % self.islands.len();
    }

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

    fn update_island_economy(&mut self, dt: f32) {
        let tick = self.tick;
        self.islands
            .par_iter_mut()
            .for_each(|island| island.produce_consume_and_price(dt, tick));
    }

    fn move_ships(&mut self, dt: f32) {
        let cost_per_mile_factor = self.planning_tuning.cost_per_mile_factor;
        self.ships.par_iter_mut().for_each(|slot| {
            if let Some(ship) = slot.as_mut() {
                let _ = ship.update(dt, cost_per_mile_factor);
            }
        });
    }

    fn apply_maritime_friction(&mut self, dt: f32) {
        let global_crowding_multiplier = (self.active_ship_count() as f32 / 100.0).max(1.0);
        let cost_per_mile_factor = self.planning_tuning.cost_per_mile_factor;
        self.ships.par_iter_mut().for_each(|slot| {
            if let Some(ship) = slot.as_mut() {
                ship.apply_maritime_friction(dt, global_crowding_multiplier, cost_per_mile_factor);
            }
        });
    }

    fn process_docked_ships(&mut self) {
        struct IslandBatchResult {
            ships: Vec<(usize, Ship)>,
            outbound_recent_departures: Vec<f32>,
            departure_targets: Vec<usize>,
        }

        let island_count = self.islands.len();
        if island_count == 0 {
            return;
        }

        let mut ship_slots = std::mem::take(&mut self.ships);
        let mut ships_by_island: Vec<Vec<(usize, Ship)>> =
            (0..island_count).map(|_| Vec::new()).collect();

        for (ship_id, slot) in ship_slots.iter_mut().enumerate() {
            let Some(ship) = slot.take() else {
                continue;
            };

            if let Some(island_id) = ship.docked_island() {
                if island_id < island_count {
                    ships_by_island[island_id].push((ship_id, ship));
                    continue;
                }
            }

            *slot = Some(ship);
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|island| island.pos).collect();
        let planning_tuning = self.planning_tuning;
        let tick = self.tick;
        let outbound_seed = self.recent_route_departures.clone();

        let island_results: Vec<IslandBatchResult> = self
            .islands
            .par_iter_mut()
            .zip(ships_by_island.into_par_iter())
            .zip(outbound_seed.into_par_iter())
            .enumerate()
            .map(|(island_id, ((island, mut docked_ships), mut outbound_recent_departures))| {
                if docked_ships.is_empty() {
                    return IslandBatchResult {
                        ships: docked_ships,
                        outbound_recent_departures,
                        departure_targets: Vec::new(),
                    };
                }

                island.mark_seen(tick);
                let mut sold_and_empty = vec![false; docked_ships.len()];
                let mut bankrupt = vec![false; docked_ships.len()];

                for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
                    let ship_tuning = ship.effective_tuning(&planning_tuning);
                    ship.begin_dock_tick(&ship_tuning);
                    let load_context = LoadPlanningContext {
                        current_island_id: island_id,
                        island_positions: &island_positions,
                        current_tick: tick,
                        tuning: &ship_tuning,
                        outbound_recent_departures: &outbound_recent_departures,
                    };
                    let settled_any = ship.trade_settle_until_stuck(
                        island_id,
                        island,
                        &load_context,
                        &ship_tuning,
                        MAX_DOCK_SETTLEMENT_STEPS,
                    );
                    if settled_any {
                        sold_and_empty[idx] = ship.has_no_cargo();
                    }
                    if ship.is_bankrupt() {
                        bankrupt[idx] = true;
                    }
                }

                island.recompute_local_prices(tick);

                for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
                    if bankrupt[idx] {
                        continue;
                    }
                    let exclude = ship.just_sold_resource();
                    let ship_tuning = ship.effective_tuning(&planning_tuning);
                    let load_context = LoadPlanningContext {
                        current_island_id: island_id,
                        island_positions: &island_positions,
                        current_tick: tick,
                        tuning: &ship_tuning,
                        outbound_recent_departures: &outbound_recent_departures,
                    };
                    let _ = ship.trade_load_if_empty(island, exclude, &load_context);
                }

                let mut island_ledger_buffer = island.ledger.clone();
                for (idx, (_, ship)) in docked_ships.iter().enumerate() {
                    if sold_and_empty[idx] || bankrupt[idx] {
                        continue;
                    }
                    ship.contribute_ledger_to_island_buffer(island_id, &mut island_ledger_buffer);
                }
                island.ledger = island_ledger_buffer;

                let island_ledger_snapshot = island.ledger.clone();
                let mut departure_targets = Vec::new();

                for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
                    if sold_and_empty[idx] || bankrupt[idx] {
                        continue;
                    }
                    if !ship.has_no_cargo() && !ship.cargo_changed_this_dock() {
                        continue;
                    }

                    let ship_tuning = ship.effective_tuning(&planning_tuning);
                    ship.sync_ledger_from_snapshot(&island_ledger_snapshot);
                    if let Some(target_island_id) = ship.plan_next_island(
                        island_id,
                        &island_positions,
                        tick,
                        &ship_tuning,
                        &outbound_recent_departures,
                    ) {
                        if target_island_id != island_id {
                            ship.set_target(target_island_id, island_positions[target_island_id]);
                            departure_targets.push(target_island_id);
                            if let Some(slot) = outbound_recent_departures.get_mut(target_island_id)
                            {
                                *slot += 1.0;
                            }
                        }
                    }
                }

                let ships = docked_ships
                    .into_iter()
                    .enumerate()
                    .filter_map(|(idx, pair)| (!bankrupt[idx]).then_some(pair))
                    .collect();

                IslandBatchResult {
                    ships,
                    outbound_recent_departures,
                    departure_targets,
                }
            })
            .collect();

        for (island_id, result) in island_results.into_iter().enumerate() {
            self.recent_route_departures[island_id] = result.outbound_recent_departures;

            if island_id < self.route_departure_history[self.route_history_cursor].len() {
                for target_island_id in result.departure_targets {
                    if target_island_id
                        < self.route_departure_history[self.route_history_cursor][island_id].len()
                    {
                        let slot = &mut self.route_departure_history[self.route_history_cursor]
                            [island_id][target_island_id];
                        *slot = slot.saturating_add(1);
                    }
                }
            }

            for (ship_id, ship) in result.ships {
                ship_slots[ship_id] = Some(ship);
            }
        }

        self.ships = ship_slots;
        self.ensure_selected_ship_valid();
    }

    fn evolve_fleet(&mut self) {
        let scuttle_threshold = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER;
        let island_count = self.islands.len().max(1) as f32;
        let fleet_pressure =
            (self.active_ship_count() as f32 / (island_count * TARGET_SHIPS_PER_ISLAND)).max(1.0);
        let cost_factor = self.planning_tuning.cost_per_mile_factor.clamp(0.2, 5.0);
        let birth_threshold = STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER * cost_factor * fleet_pressure;
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

    pub fn draw(&self) {
        let world_units_per_pixel_x = WORLD_SIZE / screen_width().max(1.0);
        let world_units_per_pixel_y = WORLD_SIZE / screen_height().max(1.0);
        let world_units_per_pixel = world_units_per_pixel_x.max(world_units_per_pixel_y);

        for island in &self.islands {
            island.draw(world_units_per_pixel);
        }

        if !self.islands.is_empty() {
            let selected_island_idx = self.selected_island_index.min(self.islands.len() - 1);
            self.islands[selected_island_idx].draw_selection_border(world_units_per_pixel);
        }

        for ship in self.ships.iter().flatten() {
            ship.draw();
        }

        if let Some(ship) = self
            .ships
            .get(self.selected_ship_index)
            .and_then(|slot| slot.as_ref())
        {
            let ring_radius = 14.0 * world_units_per_pixel;
            let ring_thickness = 3.0 * world_units_per_pixel;
            draw_circle_lines(ship.pos.x, ship.pos.y, ring_radius, ring_thickness, RED);
        }
    }

    pub fn draw_ui(&self) {
        let panel_x = 14.0;
        let panel_y = 14.0;
        let panel_w = 260.0;
        let panel_h = 404.0;

        let mut total_inventory = [0.0_f32; RESOURCE_COUNT];
        let mut total_population = 0.0_f32;
        let mut total_cash = 0.0_f32;
        let mut total_infrastructure = 0.0_f32;
        for island in &self.islands {
            for (idx, slot) in total_inventory.iter_mut().enumerate() {
                *slot += island.inventory[idx].max(0.0);
            }
            total_population += island.population.max(0.0);
            total_cash += island.cash.max(0.0);
            total_infrastructure += island.infrastructure_level.max(0.0);
        }

        draw_rectangle(
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            Color::from_rgba(8, 16, 30, 210),
        );
        draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 2.0, LIGHTGRAY);
        draw_text("Legend", panel_x + 10.0, panel_y + 22.0, 24.0, WHITE);

        let entries = [
            ("Grain", YELLOW),
            ("Timber", GREEN),
            ("Iron", DARKGRAY),
            ("Tools", RED),
            ("Spices", PURPLE),
            ("Empty ship", WHITE),
        ];

        for (i, (label, color)) in entries.iter().enumerate() {
            let y = panel_y + 42.0 + i as f32 * 16.0;
            draw_rectangle(panel_x + 10.0, y - 10.0, 10.0, 10.0, *color);
            draw_rectangle_lines(panel_x + 10.0, y - 10.0, 10.0, 10.0, 1.0, GRAY);
            if i < RESOURCE_COUNT {
                let counter = format!("{}: {:.0}", label, total_inventory[i]);
                draw_text(&counter, panel_x + 28.0, y, 18.0, WHITE);
            } else {
                draw_text(label, panel_x + 28.0, y, 18.0, WHITE);
            }
        }

        let shape_legend_y = panel_y + 136.0;
        draw_text("Ship Shapes", panel_x + 10.0, shape_legend_y, 18.0, WHITE);

        let icon_y = shape_legend_y + 14.0;
        let runner_x = panel_x + 14.0;
        let freighter_x = panel_x + 92.0;
        let coaster_x = panel_x + 188.0;

        let runner_top = vec2(runner_x, icon_y - 8.0);
        let runner_left = vec2(runner_x - 7.0, icon_y + 6.0);
        let runner_right = vec2(runner_x + 7.0, icon_y + 6.0);
        draw_triangle(runner_top, runner_left, runner_right, WHITE);
        draw_triangle_lines(runner_top, runner_left, runner_right, 1.5, LIGHTGRAY);
        draw_text("Runner", runner_x + 12.0, icon_y + 4.0, 16.0, WHITE);

        draw_rectangle(freighter_x - 7.0, icon_y - 7.0, 14.0, 14.0, WHITE);
        draw_rectangle_lines(freighter_x - 7.0, icon_y - 7.0, 14.0, 14.0, 1.5, LIGHTGRAY);
        draw_text("Freighter", freighter_x + 12.0, icon_y + 4.0, 16.0, WHITE);

        draw_circle(coaster_x, icon_y, 7.0, WHITE);
        draw_circle_lines(coaster_x, icon_y, 7.0, 1.5, LIGHTGRAY);
        draw_text("Coaster", coaster_x + 12.0, icon_y + 4.0, 16.0, WHITE);

        let avg_infrastructure = if self.islands.is_empty() {
            0.0
        } else {
            total_infrastructure / self.islands.len() as f32
        };
        let mut runner_count = 0_usize;
        let mut freighter_count = 0_usize;
        let mut coaster_count = 0_usize;
        for ship in self.ships.iter().flatten() {
            match ship.archetype() {
                ShipArchetype::Runner => runner_count += 1,
                ShipArchetype::Freighter => freighter_count += 1,
                ShipArchetype::Coaster => coaster_count += 1,
            }
        }

        let pop_text = format!("Population: {:.0}", total_population);
        let cash_text = format!("Cash: {:.0}", total_cash);
        let infra_text = format!("Industry: {:.2}", avg_infrastructure);
        let mile_cost_text = format!(
            "Mile cost x: {:.2}",
            self.planning_tuning.cost_per_mile_factor
        );
        let active_ship_count = self.active_ship_count();
        let ship_count_text = format!("Ships: {}", active_ship_count);
        let archetype_text = format!(
            "R/F/C: {}/{}/{}",
            runner_count, freighter_count, coaster_count
        );
        let perf_header_text = "Perf (ms)";
        let perf_economy_text = format!("Economy: {:.2}", self.frame_timings.economy_ms);
        let perf_movement_text = format!("Movement: {:.2}", self.frame_timings.movement_ms);
        let perf_dock_text = format!("Dock: {:.2}", self.frame_timings.dock_ms);
        let perf_friction_text = format!("Friction: {:.2}", self.frame_timings.friction_ms);
        let perf_total_text = format!("Total: {:.2}", self.frame_timings.total_ms);
        draw_text(&pop_text, panel_x + 10.0, panel_y + 172.0, 18.0, WHITE);
        draw_text(&cash_text, panel_x + 10.0, panel_y + 190.0, 18.0, WHITE);
        draw_text(&infra_text, panel_x + 10.0, panel_y + 208.0, 18.0, WHITE);
        draw_text(
            &mile_cost_text,
            panel_x + 10.0,
            panel_y + 226.0,
            18.0,
            WHITE,
        );
        draw_text(
            &ship_count_text,
            panel_x + 10.0,
            panel_y + 244.0,
            18.0,
            WHITE,
        );
        draw_text(
            &archetype_text,
            panel_x + 10.0,
            panel_y + 262.0,
            18.0,
            WHITE,
        );
        draw_text(perf_header_text, panel_x + 10.0, panel_y + 288.0, 18.0, WHITE);
        draw_text(
            &perf_economy_text,
            panel_x + 10.0,
            panel_y + 306.0,
            17.0,
            WHITE,
        );
        draw_text(
            &perf_movement_text,
            panel_x + 10.0,
            panel_y + 324.0,
            17.0,
            WHITE,
        );
        draw_text(
            &perf_dock_text,
            panel_x + 10.0,
            panel_y + 342.0,
            17.0,
            WHITE,
        );
        draw_text(
            &perf_friction_text,
            panel_x + 10.0,
            panel_y + 360.0,
            17.0,
            WHITE,
        );
        draw_text(
            &perf_total_text,
            panel_x + 10.0,
            panel_y + 378.0,
            17.0,
            WHITE,
        );

        let inspect_w = 320.0;
        let inspect_h = 226.0;
        let inspect_x = (screen_width() - inspect_w - 14.0).max(14.0);
        let inspect_y = 14.0;
        draw_rectangle(
            inspect_x,
            inspect_y,
            inspect_w,
            inspect_h,
            Color::from_rgba(8, 16, 30, 210),
        );
        draw_rectangle_lines(inspect_x, inspect_y, inspect_w, inspect_h, 2.0, LIGHTGRAY);
        draw_text("Selected Ship", inspect_x + 10.0, inspect_y + 22.0, 24.0, WHITE);

        if active_ship_count == 0 {
            draw_text("No ships", inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
            return;
        }

        let selected_idx = self.selected_ship_index;
        let Some(ship) = self.ships.get(selected_idx).and_then(|slot| slot.as_ref()) else {
            draw_text("No ships", inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
            return;
        };

        let archetype_label = match ship.archetype() {
            ShipArchetype::Runner => "Runner",
            ShipArchetype::Freighter => "Freighter",
            ShipArchetype::Coaster => "Coaster",
        };

        let status_text = if let Some(island_id) = ship.docked_island() {
            format!("Docked at: {}", island_id)
        } else if let Some(target_id) = ship.target_island() {
            format!("En route to: {}", target_id)
        } else {
            "Status: Idle".to_string()
        };

        let dominant_cargo_text = if let Some((resource, value)) = ship.dominant_cargo_by_value() {
            let label = match resource {
                Resource::Grain => "Grain",
                Resource::Timber => "Timber",
                Resource::Iron => "Iron",
                Resource::Tools => "Tools",
                Resource::Spices => "Spices",
            };
            format!("Top cargo value: {} ({:.0})", label, value)
        } else {
            "Top cargo value: Empty".to_string()
        };

        let ship_id_text = format!(
            "Ship ID: {}  Active: {}/{}",
            selected_idx,
            active_ship_count,
            self.ships.len()
        );
        let archetype_text = format!("Archetype: {}", archetype_label);
        let speed_text = format!("Speed: {:.1}", ship.speed());
        let cargo_text = format!(
            "Cargo vol: {:.1}/{:.1}",
            ship.cargo_volume_used(),
            ship.max_cargo_volume()
        );
        let upkeep_text = format!(
            "Rigging/Labor: {:.2} / {:.4}",
            ship.fuel_burn_rate(),
            ship.maintenance_rate()
        );
        let cash_text = format!("Cash: {:.1}", ship.cash);
        let cargo_mix_text = format!(
            "Cargo G/T/I/To/S: {:.1}/{:.1}/{:.1}/{:.1}/{:.1}",
            ship.cargo_amount(Resource::Grain),
            ship.cargo_amount(Resource::Timber),
            ship.cargo_amount(Resource::Iron),
            ship.cargo_amount(Resource::Tools),
            ship.cargo_amount(Resource::Spices),
        );
        let controls_text = "[ / ]: Prev / Next ship";

        draw_text(&ship_id_text, inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
        draw_text(&archetype_text, inspect_x + 10.0, inspect_y + 66.0, 18.0, WHITE);
        draw_text(&status_text, inspect_x + 10.0, inspect_y + 84.0, 18.0, WHITE);
        draw_text(&speed_text, inspect_x + 10.0, inspect_y + 102.0, 18.0, WHITE);
        draw_text(&cargo_text, inspect_x + 10.0, inspect_y + 120.0, 18.0, WHITE);
        draw_text(&upkeep_text, inspect_x + 10.0, inspect_y + 138.0, 18.0, WHITE);
        draw_text(&cash_text, inspect_x + 10.0, inspect_y + 156.0, 18.0, WHITE);
        draw_text(&cargo_mix_text, inspect_x + 10.0, inspect_y + 174.0, 17.0, WHITE);
        draw_text(
            &dominant_cargo_text,
            inspect_x + 10.0,
            inspect_y + 192.0,
            17.0,
            WHITE,
        );
        draw_text(controls_text, inspect_x + 10.0, inspect_y + 214.0, 16.0, LIGHTGRAY);

        let island_hud_y = inspect_y + inspect_h + 12.0;
        let island_hud_h = 208.0;
        draw_rectangle(
            inspect_x,
            island_hud_y,
            inspect_w,
            island_hud_h,
            Color::from_rgba(8, 16, 30, 210),
        );
        draw_rectangle_lines(inspect_x, island_hud_y, inspect_w, island_hud_h, 2.0, LIGHTGRAY);
        draw_text("Selected Island", inspect_x + 10.0, island_hud_y + 22.0, 24.0, WHITE);

        if self.islands.is_empty() {
            draw_text("No islands", inspect_x + 10.0, island_hud_y + 48.0, 18.0, WHITE);
            return;
        }

        let island_idx = self.selected_island_index.min(self.islands.len() - 1);
        let island = &self.islands[island_idx];

        let island_id_text = format!("Island: {}/{}", island_idx + 1, self.islands.len());
        let island_pop_text = format!("Population: {:.0}", island.population.max(0.0));
        let island_cash_text = format!("Cash: {:.0}", island.cash.max(0.0));
        let island_infra_text = format!("Infrastructure: {:.2}", island.infrastructure_level.max(0.0));
        let inv_text = format!(
            "Inv G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}",
            island.inventory[Resource::Grain.idx()].max(0.0),
            island.inventory[Resource::Timber.idx()].max(0.0),
            island.inventory[Resource::Iron.idx()].max(0.0),
            island.inventory[Resource::Tools.idx()].max(0.0),
            island.inventory[Resource::Spices.idx()].max(0.0)
        );
        let price_text = format!(
            "Price G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}",
            island.local_prices[Resource::Grain.idx()].max(0.0),
            island.local_prices[Resource::Timber.idx()].max(0.0),
            island.local_prices[Resource::Iron.idx()].max(0.0),
            island.local_prices[Resource::Tools.idx()].max(0.0),
            island.local_prices[Resource::Spices.idx()].max(0.0)
        );
        let island_controls_text = "{ / }: Prev / Next island";

        draw_text(&island_id_text, inspect_x + 10.0, island_hud_y + 48.0, 18.0, WHITE);
        draw_text(&island_pop_text, inspect_x + 10.0, island_hud_y + 66.0, 18.0, WHITE);
        draw_text(&island_cash_text, inspect_x + 10.0, island_hud_y + 84.0, 18.0, WHITE);
        draw_text(&island_infra_text, inspect_x + 10.0, island_hud_y + 102.0, 18.0, WHITE);
        draw_text(&inv_text, inspect_x + 10.0, island_hud_y + 128.0, 17.0, WHITE);
        draw_text(&price_text, inspect_x + 10.0, island_hud_y + 154.0, 17.0, WHITE);
        draw_text(
            island_controls_text,
            inspect_x + 10.0,
            island_hud_y + 196.0,
            16.0,
            LIGHTGRAY,
        );
    }
}
