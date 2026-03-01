use ::rand::Rng;
use macroquad::prelude::*;
use rayon::prelude::*;

use crate::island::Island;
use crate::ship::{DockAction, LoadPlanningContext, PlanningTuning, Ship, STARTING_CASH};

pub const WORLD_SIZE: f32 = 5000.0;
const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;
const SCUTTLE_THRESHOLD_MULTIPLIER: f32 = 0.50;
const BIRTH_THRESHOLD_MULTIPLIER: f32 = 5.0;
const LIFECYCLE_CHECK_INTERVAL_TICKS: u64 = 30;
const MUTATION_STRENGTH: f32 = 0.05;
const SHIP_MAINTENANCE_PER_TICK: f32 = 0.01;

pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Ship>,
    recent_route_departures: Vec<Vec<f32>>,
    route_departure_history: Vec<Vec<Vec<u16>>>,
    route_history_cursor: usize,
    planning_tuning: PlanningTuning,
    tick: u64,
}

impl World {
    pub fn new(num_islands: usize, num_ships: usize) -> Self {
        let mut rng = ::rand::thread_rng();

        let islands: Vec<Island> = (0..num_islands)
            .map(|id| {
                let pos = vec2(
                    rng.gen_range(200.0..WORLD_SIZE - 200.0),
                    rng.gen_range(200.0..WORLD_SIZE - 200.0),
                );
                Island::new(id, pos, num_islands, &mut rng)
            })
            .collect();

        // Ships start docked at a random island with randomised speeds.
        let ships: Vec<Ship> = (0..num_ships)
            .map(|i| {
                let speed = rng.gen_range(200.0_f32..500.0);
                let start_island_id = i % islands.len();
                Ship::new(
                    islands[start_island_id].pos,
                    speed,
                    num_islands,
                    start_island_id,
                )
            })
            .collect();

        // Ships start docked and will load/plan before first departure.
        Self {
            islands,
            ships,
            recent_route_departures: vec![vec![0.0; num_islands]; num_islands],
            route_departure_history: vec![
                vec![vec![0; num_islands]; num_islands];
                ROUTE_HISTORY_WINDOW_TICKS
            ],
            route_history_cursor: 0,
            planning_tuning: PlanningTuning::default(),
            tick: 0,
        }
    }

    pub fn set_planning_tuning(&mut self, planning_tuning: PlanningTuning) {
        self.planning_tuning = planning_tuning;
    }

    pub fn update(&mut self, dt: f32) {
        self.tick = self.tick.saturating_add(1);
        self.begin_route_history_tick();
        self.update_island_economy(dt);
        self.move_ships(dt);
        self.process_docked_ships();
        self.apply_ship_maintenance();
        self.route_history_cursor =
            (self.route_history_cursor + 1) % ROUTE_HISTORY_WINDOW_TICKS;
        if self.tick.is_multiple_of(LIFECYCLE_CHECK_INTERVAL_TICKS) {
            self.evolve_fleet();
        }
    }

    fn begin_route_history_tick(&mut self) {
        let cursor = self.route_history_cursor;
        for origin_id in 0..self.recent_route_departures.len() {
            for target_id in 0..self.recent_route_departures[origin_id].len() {
                let stale_count = self.route_departure_history[cursor][origin_id][target_id] as f32;
                if stale_count > 0.0 {
                    self.recent_route_departures[origin_id][target_id] =
                        (self.recent_route_departures[origin_id][target_id] - stale_count)
                            .max(0.0);
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
        for ship in &mut self.ships {
            let _ = ship.update(dt);
        }
    }

    fn apply_ship_maintenance(&mut self) {
        for ship in &mut self.ships {
            ship.cash = (ship.cash - SHIP_MAINTENANCE_PER_TICK).max(0.0);
        }
    }

    fn process_docked_ships(&mut self) {
        let mut ships_by_island = vec![Vec::new(); self.islands.len()];
        let mut sold_this_tick = vec![false; self.ships.len()];
        for (ship_idx, ship) in self.ships.iter().enumerate() {
            if let Some(island_id) = ship.docked_island() {
                if island_id < self.islands.len() {
                    ships_by_island[island_id].push(ship_idx);
                }
            }
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|island| island.pos).collect();
        let mut departure_orders: Vec<(usize, usize)> = Vec::new();

        for (island_id, ship_indices) in ships_by_island.iter().enumerate() {
            if ship_indices.is_empty() {
                continue;
            }

            let mut outbound_recent_departures = self
                .recent_route_departures
                .get(island_id)
                .cloned()
                .unwrap_or_else(|| vec![0.0; self.islands.len()]);

            {
                let island = &mut self.islands[island_id];
                island.mark_seen(self.tick);

                for &ship_idx in ship_indices {
                    let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                    self.ships[ship_idx].begin_dock_tick(&ship_tuning);
                    let unload_action = self.ships[ship_idx].trade_unload_if_carrying(
                        island_id,
                        island,
                        &ship_tuning,
                    );
                    if unload_action == DockAction::Sold {
                        sold_this_tick[ship_idx] = true;
                    }
                }

                island.recompute_local_prices(self.tick);

                for &ship_idx in ship_indices {
                    let exclude = self.ships[ship_idx].just_sold_resource();
                    let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                    let load_context = LoadPlanningContext {
                        current_island_id: island_id,
                        island_positions: &island_positions,
                        current_tick: self.tick,
                        tuning: &ship_tuning,
                        outbound_recent_departures: &outbound_recent_departures,
                    };
                    let _ = self.ships[ship_idx].trade_load_if_empty(
                        island,
                        exclude,
                        &load_context,
                    );
                }

                for &ship_idx in ship_indices {
                    if sold_this_tick[ship_idx] {
                        continue;
                    }
                    let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                    self.ships[ship_idx].sync_ledgers_with_island(island);
                    if let Some(target_island_id) = self.ships[ship_idx].plan_next_island(
                        island_id,
                        &island_positions,
                        self.tick,
                        &ship_tuning,
                        &outbound_recent_departures,
                    ) {
                        if target_island_id != island_id {
                            departure_orders.push((ship_idx, target_island_id));
                            if let Some(slot) = outbound_recent_departures.get_mut(target_island_id)
                            {
                                *slot += 1.0;
                            }
                            if island_id < self.route_departure_history[self.route_history_cursor].len()
                                && target_island_id
                                    < self.route_departure_history[self.route_history_cursor]
                                        [island_id]
                                        .len()
                            {
                                let slot = &mut self.route_departure_history[self.route_history_cursor]
                                    [island_id][target_island_id];
                                *slot = slot.saturating_add(1);
                            }
                        }
                    }
                }
            }

            if island_id < self.recent_route_departures.len() {
                self.recent_route_departures[island_id] = outbound_recent_departures;
            }
        }

        for (ship_idx, target_island_id) in departure_orders {
            let target_pos = self.islands[target_island_id].pos;
            self.ships[ship_idx].set_target(target_island_id, target_pos);
        }
    }

    fn evolve_fleet(&mut self) {
        let scuttle_threshold = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER;
        let birth_threshold = STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER;
        let mut rng = ::rand::thread_rng();

        let mut scuttle_mask = vec![false; self.ships.len()];
        let mut daughters: Vec<Ship> = Vec::new();

        for (idx, ship) in self.ships.iter_mut().enumerate() {
            if ship.cash < scuttle_threshold {
                scuttle_mask[idx] = true;
                continue;
            }

            if ship.cash >= birth_threshold {
                if let Some(daughter) = ship.spawn_daughter(MUTATION_STRENGTH, &mut rng) {
                    daughters.push(daughter);
                }
            }
        }

        let old_ships = std::mem::take(&mut self.ships);
        self.ships = old_ships
            .into_iter()
            .enumerate()
            .filter_map(|(idx, ship)| (!scuttle_mask[idx]).then_some(ship))
            .collect();
        self.ships.extend(daughters);
    }

    pub fn draw(&self) {
        let world_units_per_pixel_x = WORLD_SIZE / screen_width().max(1.0);
        let world_units_per_pixel_y = WORLD_SIZE / screen_height().max(1.0);
        let world_units_per_pixel = world_units_per_pixel_x.max(world_units_per_pixel_y);

        for island in &self.islands {
            island.draw(world_units_per_pixel);
        }
        for ship in &self.ships {
            ship.draw();
        }
    }

    pub fn draw_ui(&self) {
        let panel_x = 14.0;
        let panel_y = 14.0;
        let panel_w = 180.0;
        let panel_h = 166.0;

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
            ("Empty ship", WHITE),
        ];

        for (i, (label, color)) in entries.iter().enumerate() {
            let y = panel_y + 42.0 + i as f32 * 16.0;
            draw_rectangle(panel_x + 10.0, y - 10.0, 10.0, 10.0, *color);
            draw_rectangle_lines(panel_x + 10.0, y - 10.0, 10.0, 10.0, 1.0, GRAY);
            draw_text(label, panel_x + 28.0, y, 18.0, WHITE);
        }

        let tuning_text = format!("Spec floor: {:.2}", self.planning_tuning.speculation_floor);
        draw_text(
            &tuning_text,
            panel_x + 10.0,
            panel_y + panel_h - 28.0,
            18.0,
            WHITE,
        );

        let ship_count_text = format!("Ships: {}", self.ships.len());
        draw_text(
            &ship_count_text,
            panel_x + 10.0,
            panel_y + panel_h - 10.0,
            18.0,
            WHITE,
        );
    }
}
