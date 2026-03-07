//! Ship movement system.

use bevy::prelude::*;
use std::time::Instant;

use crate::components::{Position, ShipMarker, ShipMovement, ShipTrading};
use crate::resources::{FrameTimingsRes, TimeScale};

pub fn move_ships(
    mut query: Query<
        (
            &mut Position,
            &mut ShipMovement,
            &mut ShipTrading,
            &mut Transform,
        ),
        With<ShipMarker>,
    >,
    time_scale: Res<TimeScale>,
    time: Res<Time>,
    mut frame_timings: ResMut<FrameTimingsRes>,
) {
    let phase_start = Instant::now();
    let dt = time.delta_secs() * time_scale.0;

    query
        .par_iter_mut()
        .for_each(|(mut pos, mut movement, mut trading, mut transform)| {
            let to_target = movement.target - pos.0;
            let dist = to_target.length();
            movement.last_step_distance = 0.0;
            if dist < 1.0 {
                trading.docked_at = movement.target_island_id;
                trading.last_docked_island_id = trading.docked_at;
            } else {
                let step = movement.speed * dt;
                movement.last_step_distance = step.min(dist);
                if step >= dist {
                    pos.0 = movement.target;
                    trading.docked_at = movement.target_island_id;
                    trading.last_docked_island_id = trading.docked_at;
                } else {
                    pos.0 += to_target.normalize() * step;
                }
            }
            transform.translation = pos.0.extend(transform.translation.z);
        });

    frame_timings.accum_movement_ms += phase_start.elapsed().as_secs_f32() * 1000.0;
}
