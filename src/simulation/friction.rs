//! Maritime friction system.

use bevy::prelude::*;
use std::time::Instant;

use crate::components::{ShipMarker, ShipMovement, ShipProfile, ShipTrading};
use crate::resources::{FrameTimingsRes, PlanningTuningRes, TimeScale};
use crate::ship::{ShipState, TARGET_SHIPS_PER_ISLAND};

pub fn apply_maritime_friction(
    mut query: Query<(&mut ShipMovement, &mut ShipTrading, &ShipProfile), With<ShipMarker>>,
    time_scale: Res<TimeScale>,
    time: Res<Time>,
    planning_tuning: Res<PlanningTuningRes>,
    ship_count: Query<(), With<ShipMarker>>,
    island_count: Query<(), With<crate::components::IslandMarker>>,
    mut frame_timings: ResMut<FrameTimingsRes>,
) {
    let phase_start = Instant::now();
    let dt = time.delta_secs() * time_scale.0;

    let num_islands = island_count.iter().count().max(1) as f32;
    let num_ships = ship_count.iter().count() as f32;
    let target_population = (num_islands * TARGET_SHIPS_PER_ISLAND).max(1.0);
    let crowding_factor = (num_ships / target_population).max(0.10);
    let global_friction_mult = planning_tuning.0.global_friction_mult * crowding_factor;

    query
        .par_iter_mut()
        .for_each(|(mut movement, mut trading, profile)| {
            // Labor cost
            let (_, _, labor_mult, _) = ShipState::profile_multipliers_static(profile.archetype);
            let efficiency_factor_labor =
                (1.20 - 0.35 * profile.efficiency_rating).clamp(0.70, 1.15);
            let labor_rate = crate::ship::BASE_LABOR_RATE * labor_mult * efficiency_factor_labor;

            let mut labor = labor_rate * dt.max(0.0) * global_friction_mult;
            if trading.docked_at.is_some() {
                labor *= 1.5; // DOCKED_PORT_FEE_MULTIPLIER
            }

            // Wear cost
            let (_, _, _, wear_mult) = ShipState::profile_multipliers_static(profile.archetype);
            let efficiency_factor_wear =
                (1.20 - 0.40 * profile.efficiency_rating).clamp(0.65, 1.15);
            let wear_rate = crate::ship::BASE_WEAR_RATE * wear_mult * efficiency_factor_wear;

            let cargo_load_ratio = if let Some((resource, amount)) = trading.cargo {
                (amount.max(0.0) * resource.volume_per_unit() / profile.max_cargo_volume.max(0.01))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let wear_multiplier = 1.0 + cargo_load_ratio * crate::ship::HEAVY_LOAD_WEAR_MULTIPLIER;
            let wear = movement.last_step_distance.max(0.0)
                * wear_rate
                * global_friction_mult
                * wear_multiplier;

            trading.labor_debt += labor.max(0.0);
            trading.wear_debt += wear.max(0.0);
            movement.last_step_distance = 0.0;
        });

    frame_timings.accum_friction_ms += phase_start.elapsed().as_secs_f32() * 1000.0;
}
