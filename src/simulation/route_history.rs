//! Tick advancement and route history management.

use bevy::prelude::*;

use crate::components::{IslandId, IslandMarker, Position};
use crate::resources::{IslandPositions, RouteHistory, SimulationTick};

pub fn advance_tick(mut tick: ResMut<SimulationTick>, mut route_history: ResMut<RouteHistory>) {
    tick.0 = tick.0.saturating_add(1);

    // Expire route-history contributions that rolled out of the window.
    let cursor = route_history.cursor;
    for origin_id in 0..route_history.recent_route_departures.len() {
        for target_id in 0..route_history.recent_route_departures[origin_id].len() {
            let stale_count =
                route_history.route_departure_history[cursor][origin_id][target_id] as f32;
            if stale_count > 0.0 {
                route_history.recent_route_departures[origin_id][target_id] =
                    (route_history.recent_route_departures[origin_id][target_id] - stale_count)
                        .max(0.0);
                route_history.route_departure_history[cursor][origin_id][target_id] = 0;
            }
        }
    }
}

pub fn rebuild_island_positions(
    mut island_positions: ResMut<IslandPositions>,
    query: Query<(&IslandId, &Position), With<IslandMarker>>,
) {
    for (id, pos) in query.iter() {
        if id.0 < island_positions.0.len() {
            island_positions.0[id.0] = pos.0;
        }
    }
}
