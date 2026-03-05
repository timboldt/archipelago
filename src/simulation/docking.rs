//! Docking-phase processing — exclusive system for cross-entity mutation.

use bevy::prelude::*;
use std::time::Instant;

use crate::components::{
    IslandId, IslandMarker, MarketLedger, Position, ShipLedger, ShipMarker,
    ShipMovement, ShipProfile, ShipTrading,
};
use crate::island::IslandEconomy;
use crate::resources::{
    FrameTimingsRes, IslandPositions, PlanningTuningRes, RouteHistory, SimulationTick,
};
use crate::ship::{LoadPlanningContext, PlanningTuning, ShipState};

const MAX_DOCK_SETTLEMENT_STEPS: usize = 3;
const TARGET_SHIPS_PER_ISLAND: f32 = 12.0;

fn environmental_tuning(
    base: &PlanningTuning,
    num_islands: usize,
    num_ships: usize,
) -> PlanningTuning {
    let island_count = num_islands.max(1) as f32;
    let target_population = (island_count * TARGET_SHIPS_PER_ISLAND).max(1.0);
    let crowding_factor = (num_ships as f32 / target_population).max(0.35);
    let mut tuning = *base;
    tuning.global_friction_mult *= crowding_factor;
    tuning
}

pub fn process_docked_ships(world: &mut World) {
    let phase_start = Instant::now();

    let num_islands = world
        .query_filtered::<(), With<IslandMarker>>()
        .iter(world)
        .count();
    let num_ships = world
        .query_filtered::<(), With<ShipMarker>>()
        .iter(world)
        .count();

    if num_islands == 0 {
        return;
    }

    let base_tuning = world.resource::<PlanningTuningRes>().0;
    let planning_tuning = environmental_tuning(&base_tuning, num_islands, num_ships);
    let tick = world.resource::<SimulationTick>().0;
    let island_positions = world.resource::<IslandPositions>().0.clone();

    // Collect island entities indexed by IslandId.
    let mut island_entities: Vec<Option<Entity>> = vec![None; num_islands];
    {
        let mut query = world.query_filtered::<(Entity, &IslandId), With<IslandMarker>>();
        for (entity, id) in query.iter(world) {
            if id.0 < num_islands {
                island_entities[id.0] = Some(entity);
            }
        }
    }

    // Collect ship entities and their docked_at status.
    let mut ship_data: Vec<(Entity, Option<usize>)> = Vec::new();
    {
        let mut query = world.query_filtered::<(Entity, &ShipTrading), With<ShipMarker>>();
        for (entity, trading) in query.iter(world) {
            ship_data.push((entity, trading.docked_at));
        }
    }

    // Group ships by island.
    let mut docked_by_island: Vec<Vec<Entity>> = vec![Vec::new(); num_islands];
    for &(entity, docked_at) in &ship_data {
        if let Some(island_id) = docked_at {
            if island_id < num_islands {
                docked_by_island[island_id].push(entity);
            }
        }
    }

    let mut all_departure_targets: Vec<(usize, usize)> = Vec::new();
    let mut bankrupt_entities: Vec<Entity> = Vec::new();

    let route_departures_clone: Vec<Vec<f32>> =
        world.resource::<RouteHistory>().recent_route_departures.clone();

    for island_id in 0..num_islands {
        let ship_entity_list = &docked_by_island[island_id];
        if ship_entity_list.is_empty() {
            continue;
        }

        let island_entity = match island_entities[island_id] {
            Some(e) => e,
            None => continue,
        };

        // Take island economy and ledger out of the ECS temporarily.
        let mut island_economy = world.entity_mut(island_entity).take::<IslandEconomy>().unwrap();
        let mut island_ledger_component = world.entity_mut(island_entity).take::<MarketLedger>().unwrap();
        let island_ledger = &mut island_ledger_component.0;

        island_economy.mark_seen(tick, island_ledger);

        // Extract ship states.
        let mut ships: Vec<(Entity, ShipState)> = Vec::with_capacity(ship_entity_list.len());
        for &ship_entity in ship_entity_list {
            let entity_ref = world.entity(ship_entity);
            let pos = entity_ref.get::<Position>().unwrap().0;
            let movement = entity_ref.get::<ShipMovement>().unwrap();
            let trading = entity_ref.get::<ShipTrading>().unwrap();
            let profile = entity_ref.get::<ShipProfile>().unwrap();
            let ship_ledger_comp = entity_ref.get::<ShipLedger>().unwrap();
            let ship = ShipState::from_components(pos, movement, trading, profile, ship_ledger_comp);
            ships.push((ship_entity, ship));
        }

        let ship_entities_local: Vec<Entity> = ships.iter().map(|(e, _)| *e).collect();
        let mut sold_and_empty = vec![false; ships.len()];
        let mut bankrupt_local = vec![false; ships.len()];

        // Pass 1: trade transactions.
        for (local_idx, (_, ship)) in ships.iter_mut().enumerate() {
            let ship_tuning = ship.effective_tuning(&planning_tuning);
            ship.begin_dock_tick();

            let settled_any = ship.trade_settle_until_stuck(
                island_id,
                &mut island_economy,
                &ship_tuning,
                MAX_DOCK_SETTLEMENT_STEPS,
            );
            if settled_any {
                sold_and_empty[local_idx] = ship.has_no_cargo();
            }

            let _ = ship.settle_service_debt(&mut island_economy);

            let outbound_recent_departures = route_departures_clone
                .get(island_id)
                .cloned()
                .unwrap_or_default();
            let exclude = ship.just_sold_resource();
            let ship_tuning = ship.effective_tuning(&planning_tuning);
            let load_context = LoadPlanningContext {
                current_island_id: island_id,
                island_positions: &island_positions,
                current_tick: tick,
                tuning: &ship_tuning,
                outbound_recent_departures: &outbound_recent_departures,
            };
            let _ = ship.trade_load_if_empty(&mut island_economy, exclude, &load_context);
            if ship.cargo_changed_this_dock() {
                let _ = ship.pay_dynamic_docking_tax(&mut island_economy);
            }

            if island_economy.cash < 0.0 {
                let deficit = (-island_economy.cash).min(ship.current_cash().max(0.0));
                ship.deduct_cash(deficit);
                island_economy.cash += deficit;
            }

            if ship.is_bankrupt() {
                island_economy.apply_ship_bankruptcy_settlement(ship.removal_cash_settlement());
                bankrupt_local[local_idx] = true;
                bankrupt_entities.push(ship_entities_local[local_idx]);
            }
        }

        island_economy.recompute_local_prices_with_ledger(tick, island_ledger);

        // Pass 2: ledger merge.
        let mut island_ledger_buffer = island_ledger.clone();
        for (local_idx, (_, ship)) in ships.iter().enumerate() {
            if sold_and_empty[local_idx] || bankrupt_local[local_idx] {
                continue;
            }
            ship.contribute_ledger_to_island_buffer(island_id, &mut island_ledger_buffer);
        }
        *island_ledger = island_ledger_buffer;

        // Pass 3: departure planning.
        let island_ledger_snapshot = island_ledger.clone();
        let mut outbound_for_island = route_departures_clone
            .get(island_id)
            .cloned()
            .unwrap_or_default();

        for (local_idx, (_, ship)) in ships.iter_mut().enumerate() {
            if sold_and_empty[local_idx] || bankrupt_local[local_idx] {
                continue;
            }
            let has_outbound_target = ship
                .target_island()
                .is_some_and(|t| t != island_id);
            if !ship.has_no_cargo() && !ship.cargo_changed_this_dock() && has_outbound_target {
                continue;
            }

            let ship_tuning = ship.effective_tuning(&planning_tuning);
            ship.sync_ledger_from_snapshot(&island_ledger_snapshot);
            if let Some(target_island_id) = ship.plan_next_island(
                island_id,
                &island_positions,
                tick,
                &ship_tuning,
                &outbound_for_island,
            ) {
                if target_island_id != island_id {
                    ship.set_target(target_island_id, island_positions[target_island_id]);
                    all_departure_targets.push((island_id, target_island_id));
                    if let Some(slot) = outbound_for_island.get_mut(target_island_id) {
                        *slot += 1.0;
                    }
                }
            } else {
                let _ = ship.pay_idle_port_fee(&mut island_economy);
            }
        }

        // Put island components back.
        world.entity_mut(island_entity).insert(island_economy);
        world.entity_mut(island_entity).insert(island_ledger_component);

        // Update route departures.
        {
            let mut route_history = world.resource_mut::<RouteHistory>();
            if island_id < route_history.recent_route_departures.len() {
                route_history.recent_route_departures[island_id] = outbound_for_island;
            }
        }

        // Write back ship states — one component at a time to avoid borrow conflicts.
        for (entity, ship) in ships {
            if bankrupt_entities.contains(&entity) {
                continue;
            }
            let new_pos = ship.pos();
            let (movement, trading, profile, ship_ledger_comp) = ship.into_components();
            // Each get_mut is a separate temporary borrow — no conflict.
            *world.get_mut::<Position>(entity).unwrap() = Position(new_pos);
            *world.get_mut::<ShipMovement>(entity).unwrap() = movement;
            *world.get_mut::<ShipTrading>(entity).unwrap() = trading;
            *world.get_mut::<ShipProfile>(entity).unwrap() = profile;
            *world.get_mut::<ShipLedger>(entity).unwrap() = ship_ledger_comp;
            world.get_mut::<Transform>(entity).unwrap().translation = new_pos.extend(1.0);
        }
    }

    // Record departure targets in route history.
    {
        let mut route_history = world.resource_mut::<RouteHistory>();
        let cursor = route_history.cursor;
        for (from_island, to_island) in &all_departure_targets {
            if *from_island < route_history.route_departure_history[cursor].len()
                && *to_island < route_history.route_departure_history[cursor][*from_island].len()
            {
                let slot =
                    &mut route_history.route_departure_history[cursor][*from_island][*to_island];
                *slot = slot.saturating_add(1);
            }
        }
        let window = route_history.route_departure_history.len();
        route_history.cursor = (cursor + 1) % window;
    }

    // Despawn bankrupt ships.
    for entity in bankrupt_entities {
        world.despawn(entity);
    }

    world.resource_mut::<FrameTimingsRes>().accum_dock_ms +=
        phase_start.elapsed().as_secs_f32() * 1000.0;
}
