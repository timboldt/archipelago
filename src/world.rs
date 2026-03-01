use ::rand::Rng;
use macroquad::prelude::*;

use crate::island::Island;
use crate::ship::Ship;

pub const WORLD_SIZE: f32 = 5000.0;

pub struct World {
    pub islands: Vec<Island>,
    pub ships: Vec<Ship>,
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
                Island::new(id, pos)
            })
            .collect();

        // Ships start docked at a random island with randomised speeds.
        let ships: Vec<Ship> = (0..num_ships)
            .map(|i| {
                let speed = rng.gen_range(200.0_f32..500.0);
                Ship::new(islands[i % islands.len()].pos, speed)
            })
            .collect();

        let mut world = Self { islands, ships };
        // Give each ship an initial destination.
        for i in 0..world.ships.len() {
            let idx = rng.gen_range(0..world.islands.len());
            world.ships[i].set_target(world.islands[idx].pos);
        }
        world
    }

    pub fn update(&mut self, dt: f32) {
        let mut rng = ::rand::thread_rng();
        for i in 0..self.ships.len() {
            let arrived = self.ships[i].update(dt);
            if arrived {
                let idx = rng.gen_range(0..self.islands.len());
                self.ships[i].set_target(self.islands[idx].pos);
            }
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
