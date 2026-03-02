use ::rand::Rng;
use macroquad::prelude::*;
use rayon::prelude::*;

use crate::island::{Island, Resource, RESOURCE_COUNT};
use crate::ship::{
    DockAction, LoadPlanningContext, PlanningTuning, Ship, ShipArchetype, STARTING_CASH,
};

pub const WORLD_SIZE: f32 = 5000.0;
const ISLAND_SPAWN_MARGIN: f32 = 200.0;
const MIN_ISLAND_SPAWN_DISTANCE: f32 = 140.0;
const ISLAND_POSITION_ATTEMPTS: usize = 40;
const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;
const SCUTTLE_THRESHOLD_MULTIPLIER: f32 = 0.35;
const BIRTH_THRESHOLD_MULTIPLIER: f32 = 5.0;
const LIFECYCLE_CHECK_INTERVAL_TICKS: u64 = 30;
const MUTATION_STRENGTH: f32 = 0.05;

pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Ship>,
    selected_ship_index: usize,
    selected_island_index: usize,
    recent_route_departures: Vec<Vec<f32>>,
    route_departure_history: Vec<Vec<Vec<u16>>>,
    route_history_cursor: usize,
    planning_tuning: PlanningTuning,
    tick: u64,
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
        }
    }

    pub fn set_planning_tuning(&mut self, planning_tuning: PlanningTuning) {
        self.planning_tuning = planning_tuning;
    }

    pub fn select_next_ship(&mut self) {
        if self.ships.is_empty() {
            self.selected_ship_index = 0;
            return;
        }
        self.selected_ship_index = (self.selected_ship_index + 1) % self.ships.len();
    }

    pub fn select_previous_ship(&mut self) {
        if self.ships.is_empty() {
            self.selected_ship_index = 0;
            return;
        }
        if self.selected_ship_index == 0 {
            self.selected_ship_index = self.ships.len() - 1;
        } else {
            self.selected_ship_index -= 1;
        }
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
        if self.ships.is_empty() {
            self.selected_ship_index = 0;
        } else if self.selected_ship_index >= self.ships.len() {
            self.selected_ship_index = self.ships.len() - 1;
        }
        if self.islands.is_empty() {
            self.selected_island_index = 0;
        } else if self.selected_island_index >= self.islands.len() {
            self.selected_island_index = self.islands.len() - 1;
        }

        self.tick = self.tick.saturating_add(1);
        self.begin_route_history_tick();
        self.update_island_economy(dt);
        self.move_ships(dt);
        self.process_docked_ships();
        self.apply_ship_maintenance(dt);
        self.route_history_cursor = (self.route_history_cursor + 1) % ROUTE_HISTORY_WINDOW_TICKS;
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
        for ship in &mut self.ships {
            let _ = ship.update(dt, self.planning_tuning.cost_per_mile_factor);
        }
    }

    fn apply_ship_maintenance(&mut self, dt: f32) {
        for ship in &mut self.ships {
            ship.apply_maintenance(dt);
        }
    }

    fn process_docked_ships(&mut self) {
        let mut ships_by_island = vec![Vec::new(); self.islands.len()];
        let mut sold_this_tick = vec![false; self.ships.len()];
        let mut bankrupt_this_tick = vec![false; self.ships.len()];
        for (ship_idx, ship) in self.ships.iter().enumerate() {
            if let Some(island_id) = ship.docked_island() {
                if island_id < self.islands.len() {
                    ships_by_island[island_id].push(ship_idx);
                }
            }
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|island| island.pos).collect();
        let mut departure_orders: Vec<(usize, usize)> = Vec::new();
        let mut outbound_recent_departures_by_island = self.recent_route_departures.clone();

        // Phase 1: execute sell/buy actions without cross-island ledger mutation.
        for (island_id, ship_indices) in ships_by_island.iter().enumerate() {
            if ship_indices.is_empty() {
                continue;
            }

            let outbound_recent_departures = &outbound_recent_departures_by_island[island_id];

            {
                let island = &mut self.islands[island_id];
                island.mark_seen(self.tick);

                for &ship_idx in ship_indices {
                    if bankrupt_this_tick[ship_idx] {
                        continue;
                    }
                    let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                    self.ships[ship_idx].begin_dock_tick(&ship_tuning);
                    let load_context = LoadPlanningContext {
                        current_island_id: island_id,
                        island_positions: &island_positions,
                        current_tick: self.tick,
                        tuning: &ship_tuning,
                        outbound_recent_departures,
                    };
                    let barter_action = self.ships[ship_idx].trade_barter_if_carrying(
                        island_id,
                        island,
                        &load_context,
                    );
                    let unload_action = if barter_action == DockAction::Bartered {
                        barter_action
                    } else {
                        self.ships[ship_idx].trade_unload_if_carrying(
                            island_id,
                            island,
                            &ship_tuning,
                        )
                    };
                    if unload_action == DockAction::Sold {
                        // Only hold ships that fully unloaded and are now empty.
                        // If cargo remains after a partial sale, allow immediate redeparture.
                        sold_this_tick[ship_idx] = self.ships[ship_idx].has_no_cargo();
                    }
                    if self.ships[ship_idx].is_bankrupt() {
                        bankrupt_this_tick[ship_idx] = true;
                    }
                }

                island.recompute_local_prices(self.tick);

                for &ship_idx in ship_indices {
                    if bankrupt_this_tick[ship_idx] {
                        continue;
                    }
                    let exclude = self.ships[ship_idx].just_sold_resource();
                    let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                    let load_context = LoadPlanningContext {
                        current_island_id: island_id,
                        island_positions: &island_positions,
                        current_tick: self.tick,
                        tuning: &ship_tuning,
                        outbound_recent_departures,
                    };
                    let _ =
                        self.ships[ship_idx].trade_load_if_empty(island, exclude, &load_context);
                }
            }
        }

        // Phase 2: merge ship ledgers into each island buffer in parallel.
        let merged_island_ledgers = ships_by_island
            .par_iter()
            .enumerate()
            .map(|(island_id, ship_indices)| {
                if ship_indices.is_empty() {
                    return None;
                }

                let mut island_ledger_buffer = self.islands[island_id].ledger.clone();
                for &ship_idx in ship_indices {
                    if sold_this_tick[ship_idx] || bankrupt_this_tick[ship_idx] {
                        continue;
                    }
                    self.ships[ship_idx]
                        .contribute_ledger_to_island_buffer(island_id, &mut island_ledger_buffer);
                }

                Some(island_ledger_buffer)
            })
            .collect::<Vec<_>>();

        for (island_id, maybe_merged_ledger) in merged_island_ledgers.into_iter().enumerate() {
            if let Some(merged_ledger) = maybe_merged_ledger {
                self.islands[island_id].ledger = merged_ledger;
            }
        }

        // Phase 3: use the merged island ledger snapshot for planning, then stage departures.
        for (island_id, ship_indices) in ships_by_island.iter().enumerate() {
            if ship_indices.is_empty() {
                continue;
            }

            let island_ledger_snapshot = self.islands[island_id].ledger.clone();
            let outbound_recent_departures = &mut outbound_recent_departures_by_island[island_id];

            for &ship_idx in ship_indices {
                if sold_this_tick[ship_idx] || bankrupt_this_tick[ship_idx] {
                    continue;
                }
                let ship_tuning = self.ships[ship_idx].effective_tuning(&self.planning_tuning);
                self.ships[ship_idx].sync_ledger_from_snapshot(&island_ledger_snapshot);
                if let Some(target_island_id) = self.ships[ship_idx].plan_next_island(
                    island_id,
                    &island_positions,
                    self.tick,
                    &ship_tuning,
                    outbound_recent_departures,
                ) {
                    if target_island_id != island_id {
                        departure_orders.push((ship_idx, target_island_id));
                        if let Some(slot) = outbound_recent_departures.get_mut(target_island_id) {
                            *slot += 1.0;
                        }
                        if island_id < self.route_departure_history[self.route_history_cursor].len()
                            && target_island_id
                                < self.route_departure_history[self.route_history_cursor][island_id]
                                    .len()
                        {
                            let slot = &mut self.route_departure_history[self.route_history_cursor]
                                [island_id][target_island_id];
                            *slot = slot.saturating_add(1);
                        }
                    }
                }
            }

            if island_id < self.recent_route_departures.len() {
                self.recent_route_departures[island_id] =
                    outbound_recent_departures_by_island[island_id].clone();
            }
        }

        for (ship_idx, target_island_id) in departure_orders {
            let target_pos = self.islands[target_island_id].pos;
            self.ships[ship_idx].set_target(target_island_id, target_pos);
        }

        if bankrupt_this_tick.iter().any(|is_bankrupt| *is_bankrupt) {
            let old_ships = std::mem::take(&mut self.ships);
            self.ships = old_ships
                .into_iter()
                .enumerate()
                .filter_map(|(idx, ship)| (!bankrupt_this_tick[idx]).then_some(ship))
                .collect();
        }
    }

    fn evolve_fleet(&mut self) {
        let scuttle_threshold = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER;
        let birth_threshold = STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER;
        let mut rng = ::rand::thread_rng();

        let mut scuttle_mask = vec![false; self.ships.len()];
        let mut daughters: Vec<Ship> = Vec::new();

        for (idx, ship) in self.ships.iter_mut().enumerate() {
            if ship.estimated_net_worth() < scuttle_threshold {
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

        if !self.ships.is_empty() {
            let selected_idx = self.selected_ship_index.min(self.ships.len() - 1);
            let ship = &self.ships[selected_idx];
            draw_circle_lines(ship.pos.x, ship.pos.y, 12.0, 2.5, RED);
        }
    }

    pub fn draw_ui(&self) {
        let panel_x = 14.0;
        let panel_y = 14.0;
        let panel_w = 260.0;
        let panel_h = 310.0;

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
        let tools_per_1k_pop = if total_population > 0.0 {
            total_inventory[Resource::Tools.idx()] * 1000.0 / total_population
        } else {
            0.0
        };
        let mut runner_count = 0_usize;
        let mut freighter_count = 0_usize;
        let mut coaster_count = 0_usize;
        for ship in &self.ships {
            match ship.archetype() {
                ShipArchetype::Runner => runner_count += 1,
                ShipArchetype::Freighter => freighter_count += 1,
                ShipArchetype::Coaster => coaster_count += 1,
            }
        }

        let pop_text = format!("Population: {:.0}", total_population);
        let cash_text = format!("Cash: {:.0}", total_cash);
        let infra_text = format!("Industry: {:.2}", avg_infrastructure);
        let tools_pop_text = format!("Tools / 1k pop: {:.2}", tools_per_1k_pop);
        let carry_cost_text = format!(
            "Carry cost: {:.4}",
            self.planning_tuning.capital_carry_cost_per_time
        );
        let mile_cost_text = format!(
            "Mile cost x: {:.2}",
            self.planning_tuning.cost_per_mile_factor
        );
        let ship_count_text = format!("Ships: {}", self.ships.len());
        let archetype_text = format!(
            "R/F/C: {}/{}/{}",
            runner_count, freighter_count, coaster_count
        );
        draw_text(&pop_text, panel_x + 10.0, panel_y + 172.0, 18.0, WHITE);
        draw_text(&cash_text, panel_x + 10.0, panel_y + 190.0, 18.0, WHITE);
        draw_text(&infra_text, panel_x + 10.0, panel_y + 208.0, 18.0, WHITE);
        draw_text(
            &tools_pop_text,
            panel_x + 10.0,
            panel_y + 226.0,
            18.0,
            WHITE,
        );
        draw_text(
            &carry_cost_text,
            panel_x + 10.0,
            panel_y + 244.0,
            18.0,
            WHITE,
        );
        draw_text(
            &ship_count_text,
            panel_x + 10.0,
            panel_y + 280.0,
            18.0,
            WHITE,
        );
        draw_text(
            &archetype_text,
            panel_x + 10.0,
            panel_y + 298.0,
            18.0,
            WHITE,
        );
        draw_text(
            &mile_cost_text,
            panel_x + 10.0,
            panel_y + 262.0,
            18.0,
            WHITE,
        );

        let inspect_w = 320.0;
        let inspect_h = 208.0;
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

        if self.ships.is_empty() {
            draw_text("No ships", inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
            return;
        }

        let selected_idx = self.selected_ship_index.min(self.ships.len() - 1);
        let ship = &self.ships[selected_idx];

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

        let ship_id_text = format!("Ship: {}/{}", selected_idx + 1, self.ships.len());
        let archetype_text = format!("Archetype: {}", archetype_label);
        let speed_text = format!("Speed: {:.1}", ship.speed());
        let cargo_text = format!(
            "Cargo vol: {:.1}/{:.1}",
            ship.cargo_volume_used(),
            ship.max_cargo_volume()
        );
        let upkeep_text = format!(
            "Fuel/Maint: {:.2} / {:.4}",
            ship.fuel_burn_rate(),
            ship.maintenance_rate()
        );
        let cash_text = format!("Cash: {:.1}", ship.cash);
        let controls_text = "[ / ]: Prev / Next ship";

        draw_text(&ship_id_text, inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
        draw_text(&archetype_text, inspect_x + 10.0, inspect_y + 66.0, 18.0, WHITE);
        draw_text(&status_text, inspect_x + 10.0, inspect_y + 84.0, 18.0, WHITE);
        draw_text(&speed_text, inspect_x + 10.0, inspect_y + 102.0, 18.0, WHITE);
        draw_text(&cargo_text, inspect_x + 10.0, inspect_y + 120.0, 18.0, WHITE);
        draw_text(&upkeep_text, inspect_x + 10.0, inspect_y + 138.0, 18.0, WHITE);
        draw_text(&cash_text, inspect_x + 10.0, inspect_y + 156.0, 18.0, WHITE);
        draw_text(&dominant_cargo_text, inspect_x + 10.0, inspect_y + 174.0, 18.0, WHITE);
        draw_text(controls_text, inspect_x + 10.0, inspect_y + 196.0, 16.0, LIGHTGRAY);

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
