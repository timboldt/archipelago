//! Fleet evolution system — cull/spawn ships periodically.
//!
//! Runs every `LIFECYCLE_CHECK_INTERVAL_TICKS` ticks and performs:
//! 1. **Scuttle** — remove ships below the net-worth threshold
//! 2. **Birth** — wealthy ships spawn daughter ships with mutated traits
//! 3. **Isolated restocking** — islands without recent trade spawn a new ship

use bevy::prelude::*;

use crate::components::{
    IslandId, IslandMarker, Position, ShipArchetype, ShipLedger, ShipMarker, ShipMovement,
    ShipProfile, ShipTrading,
};
use crate::island::IslandEconomy;
use crate::resources::{IslandPositions, PlanningTuningRes, ShipMeshes, SimulationTick};
use crate::ship::{ShipState, STARTING_CASH, TARGET_SHIPS_PER_ISLAND};

const SCUTTLE_THRESHOLD_MULTIPLIER: f32 = 0.35;
const BIRTH_THRESHOLD_MULTIPLIER: f32 = 1.8;
const BIRTH_FEE_MULTIPLIER: f32 = 0.5;
const LIFECYCLE_CHECK_INTERVAL_TICKS: u64 = 30;
const MUTATION_STRENGTH: f32 = 0.05;
const ISLAND_SPAWN_DROUGHT_TICKS: u64 = 600;
const ISLAND_SPAWN_SHIP_COST: f32 = 150.0;

/// Compute lifecycle thresholds based on fleet saturation and friction.
fn lifecycle_thresholds(
    num_islands: usize,
    num_ships: usize,
    global_friction_mult: f32,
) -> (f32, f32, f32) {
    let island_count = num_islands.max(1) as f32;
    let target_population = (island_count * TARGET_SHIPS_PER_ISLAND).max(1.0);
    let fleet_pressure = (num_ships as f32 / target_population).max(1.0);
    let crowding_factor = (num_ships as f32 / target_population).max(0.10);
    let cost_factor = (global_friction_mult * crowding_factor).clamp(0.2, 6.0);

    let scuttle = STARTING_CASH * SCUTTLE_THRESHOLD_MULTIPLIER * fleet_pressure;
    let birth = STARTING_CASH * BIRTH_THRESHOLD_MULTIPLIER * cost_factor * fleet_pressure;
    let fee = STARTING_CASH * BIRTH_FEE_MULTIPLIER * cost_factor * fleet_pressure;
    (scuttle, birth, fee)
}

/// Evaluate each ship for scuttling (too poor) or birthing (wealthy enough).
#[allow(clippy::type_complexity)]
fn evaluate_lifecycle(
    world: &mut World,
    ship_entities: &[Entity],
    scuttle_threshold: f32,
    birth_threshold: f32,
    birth_fee: f32,
) -> (
    Vec<Entity>,       // to_despawn
    Vec<ShipState>,    // daughters
    Vec<(usize, f32)>, // island_birth_credits
    Vec<(usize, f32)>, // island_scuttle_settlements
) {
    let mut rng = ::rand::thread_rng();
    let mut to_despawn: Vec<Entity> = Vec::new();
    let mut daughters: Vec<ShipState> = Vec::new();
    let mut island_birth_credits: Vec<(usize, f32)> = Vec::new();
    let mut island_scuttle_settlements: Vec<(usize, f32)> = Vec::new();

    for entity in ship_entities {
        let Some(entity_ref) = world.get_entity(*entity).ok() else {
            warn!(
                "Ship entity {:?} not found during lifecycle evaluation",
                entity
            );
            continue;
        };

        let (Some(pos), Some(movement), Some(trading), Some(profile), Some(ship_ledger)) = (
            entity_ref.get::<Position>(),
            entity_ref.get::<ShipMovement>(),
            entity_ref.get::<ShipTrading>(),
            entity_ref.get::<ShipProfile>(),
            entity_ref.get::<ShipLedger>(),
        ) else {
            warn!("Ship entity {:?} missing required components", entity);
            continue;
        };

        let mut ship = ShipState::from_components(pos.0, movement, trading, profile, ship_ledger);

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

                if let Some(mut trading_mut) = world.get_mut::<ShipTrading>(*entity) {
                    trading_mut.cash = ship.current_cash();
                }
            }
        }
    }

    (
        to_despawn,
        daughters,
        island_birth_credits,
        island_scuttle_settlements,
    )
}

/// Credit birth fees and scuttle settlements to respective islands.
fn apply_island_payments(
    world: &mut World,
    island_entity_map: &[Option<Entity>],
    birth_credits: Vec<(usize, f32)>,
    scuttle_settlements: Vec<(usize, f32)>,
) {
    for (island_id, credit) in birth_credits {
        if let Some(Some(entity)) = island_entity_map.get(island_id) {
            if let Some(mut economy) = world.entity_mut(*entity).get_mut::<IslandEconomy>() {
                economy.cash += credit;
            }
        }
    }
    for (island_id, settlement) in scuttle_settlements {
        if let Some(Some(entity)) = island_entity_map.get(island_id) {
            if let Some(mut economy) = world.entity_mut(*entity).get_mut::<IslandEconomy>() {
                economy.cash += settlement;
            }
        }
    }
}

/// Spawn daughter ships and ships at isolated (trade-starved) islands.
fn spawn_new_ships(world: &mut World, daughters: Vec<ShipState>, num_islands: usize, tick: u64) {
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

    // Spawn ships at isolated islands that haven't seen trade in a long time.
    let island_positions = world.resource::<IslandPositions>().0.clone();
    let mut isolated_spawns: Vec<(Entity, usize, Vec2)> = Vec::new();
    {
        let mut query =
            world.query_filtered::<(Entity, &IslandId, &IslandEconomy), With<IslandMarker>>();
        for (entity, id, economy) in query.iter(world) {
            let drought = tick.saturating_sub(economy.last_trade_tick);
            if drought >= ISLAND_SPAWN_DROUGHT_TICKS
                && economy.cash >= ISLAND_SPAWN_SHIP_COST * 2.0
                && id.0 < island_positions.len()
            {
                isolated_spawns.push((entity, id.0, island_positions[id.0]));
            }
        }
    }

    for &(entity, _, _) in &isolated_spawns {
        if let Some(mut economy) = world.get_mut::<IslandEconomy>(entity) {
            economy.cash -= ISLAND_SPAWN_SHIP_COST;
            economy.last_trade_tick = tick;
        } else {
            warn!(
                "Island entity {:?} missing IslandEconomy during isolated restocking spawn",
                entity
            );
        }
    }

    for (_, island_id, pos) in isolated_spawns {
        let mut ship = ShipState::new(pos, 350.0, num_islands, island_id);
        ship.set_cash(ISLAND_SPAWN_SHIP_COST);
        ship.set_home_island(island_id);
        let (movement, trading, profile, ship_ledger) = ship.into_components();
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
            Position(pos),
            movement,
            trading,
            profile,
            ship_ledger,
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_translation(pos.extend(1.0)),
        ));
    }
}

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
    let (scuttle_threshold, birth_threshold, birth_fee) =
        lifecycle_thresholds(num_islands, num_ships, base_tuning.global_friction_mult);

    // Collect all ship entities.
    let ship_entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<ShipMarker>>();
        query.iter(world).collect()
    };

    let (to_despawn, daughters, birth_credits, scuttle_settlements) = evaluate_lifecycle(
        world,
        &ship_entities,
        scuttle_threshold,
        birth_threshold,
        birth_fee,
    );

    // Build island entity map for crediting payments.
    let mut island_entity_map: Vec<Option<Entity>> = vec![None; num_islands];
    {
        let mut query = world.query_filtered::<(Entity, &IslandId), With<IslandMarker>>();
        for (entity, id) in query.iter(world) {
            if id.0 < num_islands {
                island_entity_map[id.0] = Some(entity);
            }
        }
    }

    apply_island_payments(
        world,
        &island_entity_map,
        birth_credits,
        scuttle_settlements,
    );

    for entity in to_despawn {
        world.despawn(entity);
    }

    spawn_new_ships(world, daughters, num_islands, tick);
}
