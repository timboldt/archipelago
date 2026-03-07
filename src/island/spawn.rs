//! Island entity spawning.

use bevy::prelude::*;
use rand::Rng;

use crate::components::{IslandId, IslandMarker, MarketLedger, Position, PriceLedger};
use crate::island::IslandEconomy;
use crate::resources::{IslandEntityMap, IslandPositions, RouteHistory};

/// Width/height of the square simulation space in world units.
pub const WORLD_SIZE: f32 = 5000.0;
const ISLAND_SPAWN_MARGIN: f32 = 200.0;
const MIN_ISLAND_SPAWN_DISTANCE: f32 = 140.0;
const ISLAND_POSITION_ATTEMPTS: usize = 40;

pub const NUM_ISLANDS: usize = 50;

pub const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;

/// Generate island positions along a Caribbean-style arc with scatter.
///
/// The arc sweeps a randomised portion of an ellipse. Islands are placed
/// along the arc with uniform perpendicular scatter and a minimum-distance
/// check.
pub fn generate_arc_positions(rng: &mut impl Rng) -> Vec<Vec2> {
    let center = Vec2::new(WORLD_SIZE * 0.5, WORLD_SIZE * 0.5);
    // Ellipse radii: one axis is fixed, the other varies for eccentricity.
    let radius_a = WORLD_SIZE * 0.38;
    let radius_b = WORLD_SIZE * rng.gen_range(0.2..0.38);
    // Randomize arc length (150–240 degrees) and starting position.
    let arc_length: f32 = rng.gen_range(std::f32::consts::PI * 0.83..std::f32::consts::PI * 1.33);
    let arc_start: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let arc_end: f32 = arc_start + arc_length;
    let arc_spread = 1200.0; // perpendicular scatter from the arc spine

    let mut positions: Vec<Vec2> = Vec::with_capacity(NUM_ISLANDS);

    for _ in 0..NUM_ISLANDS {
        let mut best = Vec2::ZERO;
        let mut best_min_dist = -1.0_f32;

        for _ in 0..ISLAND_POSITION_ATTEMPTS {
            // Pick a random angle along the arc.
            let t: f32 = rng.gen_range(arc_start..arc_end);
            // Base point on the arc.
            let spine = center + Vec2::new(t.cos() * radius_a, t.sin() * radius_b);
            // Uniform scatter perpendicular to the arc.
            let offset = rng.gen_range(-arc_spread..arc_spread);
            let tangent = Vec2::new(-t.sin(), t.cos());
            let candidate = spine + tangent * offset;

            // Clamp inside world margins.
            let candidate = Vec2::new(
                candidate
                    .x
                    .clamp(ISLAND_SPAWN_MARGIN, WORLD_SIZE - ISLAND_SPAWN_MARGIN),
                candidate
                    .y
                    .clamp(ISLAND_SPAWN_MARGIN, WORLD_SIZE - ISLAND_SPAWN_MARGIN),
            );

            let min_dist = positions
                .iter()
                .map(|p| candidate.distance(*p))
                .fold(f32::INFINITY, f32::min);

            if min_dist >= MIN_ISLAND_SPAWN_DISTANCE {
                best = candidate;
                best_min_dist = min_dist;
                break;
            }
            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best = candidate;
            }
        }

        let _ = best_min_dist; // suppress unused warning
        positions.push(best);
    }

    positions
}

/// Spawn island entities and insert shared resources.
///
/// Returns seed data `(position, economy_clone, ledger_clone)` per island,
/// needed by ship spawning to seed initial market views.
pub fn spawn_islands(
    commands: &mut Commands,
    rng: &mut impl Rng,
    island_mesh: Handle<Mesh>,
    island_material: Handle<ColorMaterial>,
) -> Vec<(Vec2, IslandEconomy, PriceLedger)> {
    let island_positions = generate_arc_positions(rng);

    let mut entity_map = Vec::with_capacity(NUM_ISLANDS);
    let mut cached_positions = Vec::with_capacity(NUM_ISLANDS);
    let mut island_seed_data: Vec<(Vec2, IslandEconomy, PriceLedger)> = Vec::new();

    for (id, pos) in island_positions.iter().enumerate() {
        let (economy, ledger) = IslandEconomy::new(id, NUM_ISLANDS, rng);

        island_seed_data.push((
            *pos,
            IslandEconomy::clone_for_seeding(&economy),
            ledger.clone(),
        ));

        let entity = commands
            .spawn((
                IslandMarker,
                IslandId(id),
                economy,
                MarketLedger(ledger),
                Position(*pos),
                Mesh2d(island_mesh.clone()),
                MeshMaterial2d(island_material.clone()),
                Transform::from_translation(pos.extend(0.0)),
            ))
            .id();
        entity_map.push(entity);
        cached_positions.push(*pos);
    }

    commands.insert_resource(IslandEntityMap(entity_map));
    commands.insert_resource(IslandPositions(cached_positions));
    commands.insert_resource(RouteHistory {
        recent_route_departures: vec![vec![0.0; NUM_ISLANDS]; NUM_ISLANDS],
        route_departure_history: vec![
            vec![vec![0; NUM_ISLANDS]; NUM_ISLANDS];
            ROUTE_HISTORY_WINDOW_TICKS
        ],
        cursor: 0,
    });

    island_seed_data
}
