//! UI view-model builders for HUD and inspector panels.
//!
//! These types/functions convert live simulation state into display-ready values
//! so rendering modules stay presentation-oriented.

use crate::island::{Resource, RESOURCE_COUNT};
use crate::ship::ShipArchetype;

use super::World;

/// Aggregated top-level metrics shown in the left HUD panel.
pub(super) struct HudSummary {
    pub total_inventory: [f32; RESOURCE_COUNT],
    pub total_population: f32,
    pub total_cash: f32,
    pub avg_infrastructure: f32,
    pub friction_mult: f32,
    pub active_ship_count: usize,
    pub runner_count: usize,
    pub freighter_count: usize,
    pub coaster_count: usize,
    pub perf_economy_ms: f32,
    pub perf_movement_ms: f32,
    pub perf_dock_ms: f32,
    pub perf_friction_ms: f32,
    pub perf_total_ms: f32,
}

/// Display-ready fields for the selected-ship inspector panel.
pub(super) struct ShipInspectorView {
    pub has_ship: bool,
    pub ship_id_text: String,
    pub archetype_text: String,
    pub status_text: String,
    pub speed_text: String,
    pub cargo_text: String,
    pub upkeep_text: String,
    pub cash_text: String,
    pub cargo_mix_text: String,
    pub dominant_cargo_text: String,
}

/// Display-ready fields for the selected-island inspector panel.
pub(super) struct IslandInspectorView {
    pub has_island: bool,
    pub island_id_text: String,
    pub island_pop_text: String,
    pub island_cash_text: String,
    pub island_infra_text: String,
    pub inv_text: String,
    pub price_text: String,
}

/// Builds aggregate HUD metrics from the current world state.
pub(super) fn hud_summary(world: &World) -> HudSummary {
    let mut total_inventory = [0.0_f32; RESOURCE_COUNT];
    let mut total_population = 0.0_f32;
    let mut total_cash = 0.0_f32;
    let mut total_infrastructure = 0.0_f32;
    for island in &world.islands {
        for (idx, slot) in total_inventory.iter_mut().enumerate() {
            *slot += island.inventory[idx].max(0.0);
        }
        total_population += island.population.max(0.0);
        total_cash += island.cash.max(0.0);
        total_infrastructure += island.infrastructure_level.max(0.0);
    }

    let avg_infrastructure = if world.islands.is_empty() {
        0.0
    } else {
        total_infrastructure / world.islands.len() as f32
    };

    let mut runner_count = 0_usize;
    let mut freighter_count = 0_usize;
    let mut coaster_count = 0_usize;
    for ship in world.ships.iter().flatten() {
        match ship.archetype() {
            ShipArchetype::Runner => runner_count += 1,
            ShipArchetype::Freighter => freighter_count += 1,
            ShipArchetype::Coaster => coaster_count += 1,
        }
    }

    HudSummary {
        total_inventory,
        total_population,
        total_cash,
        avg_infrastructure,
        friction_mult: world.environmental_tuning().global_friction_mult,
        active_ship_count: world.active_ship_count(),
        runner_count,
        freighter_count,
        coaster_count,
        perf_economy_ms: world.frame_timings.economy_ms,
        perf_movement_ms: world.frame_timings.movement_ms,
        perf_dock_ms: world.frame_timings.dock_ms,
        perf_friction_ms: world.frame_timings.friction_ms,
        perf_total_ms: world.frame_timings.total_ms,
    }
}

/// Builds selected-ship inspector content (or an empty-state payload).
pub(super) fn ship_inspector_view(world: &World, active_ship_count: usize) -> ShipInspectorView {
    let selected_idx = world.selected_ship_index;
    let Some(ship) = world.ships.get(selected_idx).and_then(|slot| slot.as_ref()) else {
        return ShipInspectorView {
            has_ship: false,
            ship_id_text: String::new(),
            archetype_text: String::new(),
            status_text: String::new(),
            speed_text: String::new(),
            cargo_text: String::new(),
            upkeep_text: String::new(),
            cash_text: String::new(),
            cargo_mix_text: String::new(),
            dominant_cargo_text: String::new(),
        };
    };

    let archetype_label = match ship.archetype() {
        ShipArchetype::Runner => "Runner",
        ShipArchetype::Freighter => "Freighter",
        ShipArchetype::Coaster => "Coaster",
    };

    let status_text = if let Some(island_id) = ship.docked_island() {
        format!("Docked at: {}", island_id)
    } else if let Some(target_id) = ship.target_island() {
        format!("En route to: {}", target_id)
    } else {
        "Status: Idle".to_string()
    };

    let dominant_cargo_text = if let Some((resource, value)) = ship.dominant_cargo_by_value() {
        let label = match resource {
            Resource::Grain => "Grain",
            Resource::Timber => "Timber",
            Resource::Iron => "Iron",
            Resource::Tools => "Tools",
            Resource::Spices => "Spices",
        };
        format!("Top cargo value: {} ({:.0})", label, value)
    } else {
        "Top cargo value: Empty".to_string()
    };

    ShipInspectorView {
        has_ship: true,
        ship_id_text: format!(
            "Ship ID: {}  Active: {}/{}",
            selected_idx,
            active_ship_count,
            world.ships.len()
        ),
        archetype_text: format!("Archetype: {}", archetype_label),
        status_text,
        speed_text: format!("Speed: {:.1}", ship.speed()),
        cargo_text: format!(
            "Cargo vol: {:.1}/{:.1}",
            ship.cargo_volume_used(),
            ship.max_cargo_volume()
        ),
        upkeep_text: format!(
            "Distance/Time cost: {:.2} / {:.4}",
            ship.cost_per_distance_rate(),
            ship.maintenance_rate()
        ),
        cash_text: format!("Cash: {:.1}", ship.cash),
        cargo_mix_text: format!(
            "Cargo G/T/I/To/S: {:.1}/{:.1}/{:.1}/{:.1}/{:.1}",
            ship.cargo_amount(Resource::Grain),
            ship.cargo_amount(Resource::Timber),
            ship.cargo_amount(Resource::Iron),
            ship.cargo_amount(Resource::Tools),
            ship.cargo_amount(Resource::Spices),
        ),
        dominant_cargo_text,
    }
}

/// Builds selected-island inspector content (or an empty-state payload).
pub(super) fn island_inspector_view(world: &World) -> IslandInspectorView {
    if world.islands.is_empty() {
        return IslandInspectorView {
            has_island: false,
            island_id_text: String::new(),
            island_pop_text: String::new(),
            island_cash_text: String::new(),
            island_infra_text: String::new(),
            inv_text: String::new(),
            price_text: String::new(),
        };
    }

    let island_idx = world.selected_island_index.min(world.islands.len() - 1);
    let island = &world.islands[island_idx];

    IslandInspectorView {
        has_island: true,
        island_id_text: format!("Island: {}/{}", island_idx + 1, world.islands.len()),
        island_pop_text: format!("Population: {:.0}", island.population.max(0.0)),
        island_cash_text: format!("Cash: {:.0}", island.cash.max(0.0)),
        island_infra_text: format!(
            "Infrastructure: {:.2}",
            island.infrastructure_level.max(0.0)
        ),
        inv_text: format!(
            "Inv G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}",
            island.inventory[Resource::Grain.idx()].max(0.0),
            island.inventory[Resource::Timber.idx()].max(0.0),
            island.inventory[Resource::Iron.idx()].max(0.0),
            island.inventory[Resource::Tools.idx()].max(0.0),
            island.inventory[Resource::Spices.idx()].max(0.0)
        ),
        price_text: format!(
            "Price G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}",
            island.local_prices[Resource::Grain.idx()].max(0.0),
            island.local_prices[Resource::Timber.idx()].max(0.0),
            island.local_prices[Resource::Iron.idx()].max(0.0),
            island.local_prices[Resource::Tools.idx()].max(0.0),
            island.local_prices[Resource::Spices.idx()].max(0.0)
        ),
    }
}
