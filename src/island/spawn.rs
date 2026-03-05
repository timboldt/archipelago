//! Island entity spawning.


use bevy::prelude::*;

use crate::components::{IslandId, IslandMarker, MarketLedger, Position};
use crate::island::IslandEconomy;
use crate::resources::{IslandEntityMap, IslandPositions, RouteHistory};

/// Width/height of the square simulation space in world units.
pub const WORLD_SIZE: f32 = 5000.0;
const ISLAND_SPAWN_MARGIN: f32 = 200.0;
const MIN_ISLAND_SPAWN_DISTANCE: f32 = 140.0;
const ISLAND_POSITION_ATTEMPTS: usize = 40;

pub const NUM_ISLANDS: usize = 50;

pub const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;

#[allow(dead_code)]
pub fn spawn_islands(commands: &mut Commands) {
    let mut rng = ::rand::thread_rng();

    let mut island_positions: Vec<Vec2> = Vec::with_capacity(NUM_ISLANDS);
    for _ in 0..NUM_ISLANDS {
        let mut best_candidate = Vec2::new(
            rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
            rng.gen_range(ISLAND_SPAWN_MARGIN..WORLD_SIZE - ISLAND_SPAWN_MARGIN),
        );
        let mut best_min_distance = island_positions
            .iter()
            .map(|existing| best_candidate.distance(*existing))
            .fold(f32::INFINITY, f32::min);

        for _ in 0..ISLAND_POSITION_ATTEMPTS {
            let candidate = Vec2::new(
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

    use ::rand::Rng;

    let mut entity_map = Vec::with_capacity(NUM_ISLANDS);
    let mut cached_positions = Vec::with_capacity(NUM_ISLANDS);

    for (id, pos) in island_positions.iter().enumerate() {
        let (economy, ledger) = IslandEconomy::new(id, NUM_ISLANDS, &mut rng);
        let entity = commands
            .spawn((
                IslandMarker,
                IslandId(id),
                economy,
                MarketLedger(ledger),
                Position(*pos),
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
}
