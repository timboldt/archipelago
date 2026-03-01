use ::rand::Rng;
use macroquad::prelude::*;

use crate::island::Island;
use crate::ship::{DockAction, PlanningTuning, Ship};

pub const WORLD_SIZE: f32 = 5000.0;

pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Ship>,
    recent_route_departures: Vec<Vec<f32>>,
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
            planning_tuning: PlanningTuning::default(),
            tick: 0,
        }
    }

    pub fn set_planning_tuning(&mut self, planning_tuning: PlanningTuning) {
        self.planning_tuning = planning_tuning;
    }

    pub fn update(&mut self, dt: f32) {
        self.tick = self.tick.saturating_add(1);
        self.decay_route_departure_memory();
        self.update_island_economy(dt);
        self.move_ships(dt);
        self.process_docked_ships();
    }

    fn decay_route_departure_memory(&mut self) {
        let decay = self.planning_tuning.route_congestion_decay.clamp(0.0, 1.0);
        for origin_row in &mut self.recent_route_departures {
            for route_score in origin_row {
                *route_score *= decay;
            }
        }
    }

    fn update_island_economy(&mut self, dt: f32) {
        for island in &mut self.islands {
            island.produce_consume_and_price(dt, self.tick);
        }
    }

    fn move_ships(&mut self, dt: f32) {
        for ship in &mut self.ships {
            let _ = ship.update(dt);
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
                    self.ships[ship_idx].begin_dock_tick(&self.planning_tuning);
                    let unload_action = self.ships[ship_idx].trade_unload_if_carrying(
                        island_id,
                        island,
                        &self.planning_tuning,
                    );
                    if unload_action == DockAction::Sold {
                        sold_this_tick[ship_idx] = true;
                    }
                }

                island.recompute_local_prices(self.tick);

                for &ship_idx in ship_indices {
                    let _ = self.ships[ship_idx].trade_load_if_empty(
                        island_id,
                        &island_positions,
                        self.tick,
                        &self.planning_tuning,
                        island,
                    );
                }

                for &ship_idx in ship_indices {
                    if sold_this_tick[ship_idx] {
                        continue;
                    }
                    self.ships[ship_idx].sync_ledgers_with_island(island);
                    if let Some(target_island_id) = self.ships[ship_idx].plan_next_island(
                        island_id,
                        &island_positions,
                        self.tick,
                        &self.planning_tuning,
                        &outbound_recent_departures,
                    ) {
                        if target_island_id != island_id {
                            departure_orders.push((ship_idx, target_island_id));
                            if let Some(slot) = outbound_recent_departures.get_mut(target_island_id)
                            {
                                *slot += 1.0;
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
        let panel_h = 146.0;

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
            panel_y + panel_h - 10.0,
            18.0,
            WHITE,
        );
    }
}
