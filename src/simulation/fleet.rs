//! Fleet evolution system — cull/spawn ships periodically.

use bevy::prelude::*;

use crate::components::{
    IslandId, IslandMarker, Position, ShipArchetype, ShipLedger, ShipMarker, ShipMovement,
    ShipProfile, ShipTrading,
};
use crate::island::IslandEconomy;
use crate::resources::{PlanningTuningRes, ShipMeshes, SimulationTick};
use crate::ship::{ShipState, STARTING_CASH};

const SCUTTLE_THRESHOLD_MULTIPLIER: f32 = 0.35;
const BIRTH_THRESHOLD_MULTIPLIER: f32 = 5.0;
const BIRTH_FEE_MULTIPLIER: f32 = 1.5;
const TARGET_SHIPS_PER_ISLAND: f32 = 12.0;
const LIFECYCLE_CHECK_INTERVAL_TICKS: u64 = 30;
const MUTATION_STRENGTH: f32 = 0.05;

pub fn evolve_fleet(world: &mut World) {
    let tick = world.resource::<SimulationTick>().0;
    if !tick.is_multiple_of(LIFECYCLE_CHECK_INTERVAL_TICKS) {
        return;
    }

    let num_islands = world
        .query_filtered::<(), With<IslandMarker>>()
        .iter(world)
        .count();
    let num_ships = world
        .query_filtered::<(), With<ShipMarker>>()
        .iter(world)
        .count();

    let base_tuning = world.resource::<PlanningTuningRes>().0;
    let island_count = num_islands.max(1) as f32;
    let target_population = (island_count * TARGET_SHIPS_PER_ISLAND).max(1.0);
    let fleet_pressure = (num_ships as f32 / target_population).max(1.0);
    let crowding_factor = (num_ships as f32 / target_population).max(0.35);
    let cost_factor = (base_tuning.global_friction_mult * crowding_factor).clamp(0.2, 6.0);

    let scuttle_threshold = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER * fleet_pressure;
    let birth_threshold = STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER * cost_factor * fleet_pressure;
    let birth_fee = STARTING_CASH * BIRTH_FEE_MULTIPLIER * cost_factor * fleet_pressure;

    // Collect all ship entities with their state.
    let mut ship_entities: Vec<Entity> = Vec::new();
    {
        let mut query = world.query_filtered::<Entity, With<ShipMarker>>();
        for entity in query.iter(world) {
            ship_entities.push(entity);
        }
    }

    let mut rng = ::rand::thread_rng();
    let mut to_despawn: Vec<Entity> = Vec::new();
    let mut daughters: Vec<ShipState> = Vec::new();
    let mut island_birth_credits: Vec<(usize, f32)> = Vec::new();
    let mut island_scuttle_settlements: Vec<(usize, f32)> = Vec::new();

    for entity in &ship_entities {
        let entity_ref = world.entity(*entity);
        let pos = entity_ref.get::<Position>().unwrap().0;
        let movement = entity_ref.get::<ShipMovement>().unwrap();
        let trading = entity_ref.get::<ShipTrading>().unwrap();
        let profile = entity_ref.get::<ShipProfile>().unwrap();
        let ship_ledger = entity_ref.get::<ShipLedger>().unwrap();

        let mut ship = ShipState::from_components(pos, movement, trading, profile, ship_ledger);

        if ship.estimated_net_worth() < scuttle_threshold {
            if let Some(island_id) = ship.last_docked_island() {
                island_scuttle_settlements.push((island_id, ship.removal_cash_settlement()));
            }
            to_despawn.push(*entity);
            continue;
        }

        if ship.current_cash() >= birth_threshold + birth_fee {
            if let Some(daughter) = ship.spawn_daughter(MUTATION_STRENGTH, &mut rng) {
                ship.deduct_cash(birth_fee);
                if let Some(island_id) = ship.docked_island() {
                    island_birth_credits.push((island_id, birth_fee));
                }
                daughters.push(daughter);

                // Write back parent's cash deduction.
                let mut entity_ref = world.entity_mut(*entity);
                entity_ref.get_mut::<ShipTrading>().unwrap().cash = ship.current_cash();
            }
        }
    }

    // Apply credits/settlements to islands.
    let mut island_entity_map: Vec<Option<Entity>> = vec![None; num_islands];
    {
        let mut query = world.query_filtered::<(Entity, &IslandId), With<IslandMarker>>();
        for (entity, id) in query.iter(world) {
            if id.0 < num_islands {
                island_entity_map[id.0] = Some(entity);
            }
        }
    }

    for (island_id, credit) in island_birth_credits {
        if let Some(Some(entity)) = island_entity_map.get(island_id) {
            if let Some(mut economy) = world.entity_mut(*entity).get_mut::<IslandEconomy>() {
                economy.cash += credit;
            }
        }
    }
    for (island_id, settlement) in island_scuttle_settlements {
        if let Some(Some(entity)) = island_entity_map.get(island_id) {
            if let Some(mut economy) = world.entity_mut(*entity).get_mut::<IslandEconomy>() {
                economy.cash += settlement;
            }
        }
    }

    // Despawn scuttled ships.
    for entity in to_despawn {
        world.despawn(entity);
    }

    // Spawn daughters — each gets its own material for cargo coloring.
    let ship_meshes = world.resource::<ShipMeshes>();
    let clipper_mesh = ship_meshes.clipper.clone();
    let freighter_mesh = ship_meshes.freighter.clone();
    let shorthaul_mesh = ship_meshes.shorthaul.clone();

    for daughter in daughters {
        let daughter_pos = daughter.pos();
        let (movement, trading, profile, ship_ledger) = daughter.into_components();
        let mesh = match profile.archetype {
            ShipArchetype::Clipper => clipper_mesh.clone(),
            ShipArchetype::Freighter => freighter_mesh.clone(),
            ShipArchetype::Shorthaul => shorthaul_mesh.clone(),
        };
        let material = world
            .resource_mut::<Assets<ColorMaterial>>()
            .add(Color::srgb(0.9, 0.9, 0.9));
        world.spawn((
            ShipMarker,
            Position(daughter_pos),
            movement,
            trading,
            profile,
            ship_ledger,
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_translation(daughter_pos.extend(1.0)),
        ));
    }
}
