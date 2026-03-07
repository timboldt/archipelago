//! Ship entity spawning.

use bevy::prelude::*;
use rand::Rng;

use crate::components::{Position, PriceLedger, ShipArchetype, ShipMarker};
use crate::island::IslandEconomy;
use crate::resources::WorldConfig;
use crate::ship::ShipState;

pub const STARTING_SIM_TICK: u64 = 500;

/// Spawns ship entities. Must be called after islands are spawned.
///
/// Each ship gets its own material instance so colors can be updated
/// independently at runtime (e.g. based on cargo).
#[allow(clippy::too_many_arguments)]
pub fn spawn_ships(
    commands: &mut Commands,
    materials: &mut Assets<ColorMaterial>,
    rng: &mut impl Rng,
    island_seed_data: &[(Vec2, IslandEconomy, PriceLedger)],
    clipper_mesh: Handle<Mesh>,
    freighter_mesh: Handle<Mesh>,
    shorthaul_mesh: Handle<Mesh>,
    config: &WorldConfig,
) {
    let base_color = Color::srgb(0.9, 0.9, 0.9);
    let num_ships = config.num_ships;
    let total_islands = config.total_islands;

    // Mainland extra ships: 5 clippers + 5 freighters stationed at the mainland.
    let mainland_extras: Vec<(ShipArchetype, usize)> = if let Some(mid) = config.mainland_island_id
    {
        vec![
            (ShipArchetype::Clipper, mid),
            (ShipArchetype::Clipper, mid),
            (ShipArchetype::Clipper, mid),
            (ShipArchetype::Clipper, mid),
            (ShipArchetype::Clipper, mid),
            (ShipArchetype::Freighter, mid),
            (ShipArchetype::Freighter, mid),
            (ShipArchetype::Freighter, mid),
            (ShipArchetype::Freighter, mid),
            (ShipArchetype::Freighter, mid),
        ]
    } else {
        vec![]
    };

    for i in 0..(num_ships + mainland_extras.len()) {
        let speed = rng.gen_range(200.0_f32..500.0);
        let (forced_archetype, start_island_id) = if i < num_ships {
            (None, i % total_islands)
        } else {
            let (arch, island) = mainland_extras[i - num_ships];
            (Some(arch), island)
        };
        let start_pos = island_seed_data[start_island_id].0;
        let mut ship = ShipState::new(start_pos, speed, total_islands, start_island_id);
        if let Some(arch) = forced_archetype {
            ship.set_archetype(arch);
        }
        ship.seed_initial_market_view(island_seed_data, STARTING_SIM_TICK, start_island_id, rng);

        let (movement, mut trading, mut profile, ship_ledger) = ship.into_components();
        profile.home_island_id = Some(start_island_id);
        if forced_archetype.is_some() {
            trading.cash *= 10.0;
        }
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
