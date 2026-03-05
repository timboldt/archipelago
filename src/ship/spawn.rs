//! Ship entity spawning.

use bevy::prelude::*;

use crate::components::{Position, ShipMarker};
use crate::island::IslandEconomy;
use crate::ship::ShipState;

pub const NUM_SHIPS: usize = 100;
pub const STARTING_SIM_TICK: u64 = 500;

/// Spawns ship entities. Must be called after islands are spawned.
#[allow(dead_code)]
pub fn spawn_ships(
    commands: &mut Commands,
    island_data: &[(Vec2, IslandEconomy, crate::components::PriceLedger)],
    num_islands: usize,
) {
    use ::rand::Rng;
    let mut rng = ::rand::thread_rng();

    for i in 0..NUM_SHIPS {
        let speed = rng.gen_range(200.0_f32..500.0);
        let start_island_id = i % num_islands;
        let start_pos = island_data[start_island_id].0;
        let mut ship = ShipState::new(start_pos, speed, num_islands, start_island_id);
        ship.seed_initial_market_view(island_data, STARTING_SIM_TICK, start_island_id, &mut rng);

        let (movement, trading, profile, ship_ledger) = ship.into_components();
        commands.spawn((
            ShipMarker,
            Position(start_pos),
            movement,
            trading,
            profile,
            ship_ledger,
            Transform::from_translation(start_pos.extend(1.0)),
        ));
    }
}
