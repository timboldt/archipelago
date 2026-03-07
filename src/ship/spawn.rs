//! Ship entity spawning.

use bevy::prelude::*;
use rand::Rng;

use crate::components::{Position, PriceLedger, ShipArchetype, ShipMarker};
use crate::island::spawn::NUM_ISLANDS;
use crate::island::IslandEconomy;
use crate::ship::ShipState;

pub const NUM_SHIPS: usize = 100;
pub const STARTING_SIM_TICK: u64 = 500;

/// Spawns ship entities. Must be called after islands are spawned.
///
/// Each ship gets its own material instance so colors can be updated
/// independently at runtime (e.g. based on cargo).
pub fn spawn_ships(
    commands: &mut Commands,
    materials: &mut Assets<ColorMaterial>,
    rng: &mut impl Rng,
    island_seed_data: &[(Vec2, IslandEconomy, PriceLedger)],
    clipper_mesh: Handle<Mesh>,
    freighter_mesh: Handle<Mesh>,
    shorthaul_mesh: Handle<Mesh>,
) {
    let base_color = Color::srgb(0.9, 0.9, 0.9);

    for i in 0..NUM_SHIPS {
        let speed = rng.gen_range(200.0_f32..500.0);
        let start_island_id = i % NUM_ISLANDS;
        let start_pos = island_seed_data[start_island_id].0;
        let mut ship = ShipState::new(start_pos, speed, NUM_ISLANDS, start_island_id);
        ship.seed_initial_market_view(island_seed_data, STARTING_SIM_TICK, start_island_id, rng);

        let (movement, trading, mut profile, ship_ledger) = ship.into_components();
        profile.home_island_id = Some(start_island_id);
        let mesh = match profile.archetype {
            ShipArchetype::Clipper => clipper_mesh.clone(),
            ShipArchetype::Freighter => freighter_mesh.clone(),
            ShipArchetype::Shorthaul => shorthaul_mesh.clone(),
        };
        // Each ship gets its own material so cargo color can vary per ship.
        let material = materials.add(base_color);
        commands.spawn((
            ShipMarker,
            Position(start_pos),
            movement,
            trading,
            profile,
            ship_ledger,
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_translation(start_pos.extend(1.0)),
        ));
    }
}
