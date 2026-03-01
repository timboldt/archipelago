use ::rand::Rng;
use macroquad::prelude::*;

use crate::island::Island;
use crate::ship::Ship;

pub const WORLD_SIZE: f32 = 5000.0;

pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Ship>,
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
                Ship::new(islands[start_island_id].pos, speed, num_islands, start_island_id)
            })
            .collect();

        let mut world = Self {
            islands,
            ships,
            tick: 0,
        };
        // Give each ship an initial destination.
        for i in 0..world.ships.len() {
            let start_id = i % world.islands.len();
            let mut idx = rng.gen_range(0..world.islands.len());
            if idx == start_id {
                idx = (idx + 1) % world.islands.len();
            }
            world.ships[i].set_target(idx, world.islands[idx].pos);
        }
        world
    }

    pub fn update(&mut self, dt: f32) {
        self.tick = self.tick.saturating_add(1);
        self.update_island_economy(dt);
        self.move_ships(dt);
        self.process_docked_ships();
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
        for (ship_idx, ship) in self.ships.iter().enumerate() {
            if let Some(island_id) = ship.docked_island() {
                if island_id < self.islands.len() {
                    ships_by_island[island_id].push(ship_idx);
                }
            }
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|island| island.pos).collect();
        let mut departure_orders: Vec<(usize, usize)> = Vec::new();

        for island_id in 0..self.islands.len() {
            if ships_by_island[island_id].is_empty() {
                continue;
            }

            {
                let island = &mut self.islands[island_id];

                for &ship_idx in &ships_by_island[island_id] {
                    self.ships[ship_idx].begin_dock_tick();
                    let _ = self.ships[ship_idx].trade_unload_if_carrying(island);
                }

                island.recompute_local_prices(self.tick);

                for &ship_idx in &ships_by_island[island_id] {
                    let _ = self.ships[ship_idx].trade_load_if_empty(island);
                }

                for &ship_idx in &ships_by_island[island_id] {
                    self.ships[ship_idx].sync_ledgers_with_island(island);
                    if let Some(target_island_id) =
                        self.ships[ship_idx].plan_next_island(island_id, &island_positions)
                    {
                        if target_island_id != island_id {
                            departure_orders.push((ship_idx, target_island_id));
                        }
                    }
                }
            }
        }

        for (ship_idx, target_island_id) in departure_orders {
            let target_pos = self.islands[target_island_id].pos;
            self.ships[ship_idx].set_target(target_island_id, target_pos);
        }
    }

    pub fn draw(&self) {
        for island in &self.islands {
            island.draw();
        }
        for ship in &self.ships {
            ship.draw();
        }
    }
}
