//! Camera setup for the 5000x5000 world.

use bevy::prelude::*;

use crate::island::spawn::WORLD_SIZE;

pub fn setup_camera(mut commands: Commands) {
    let half = WORLD_SIZE / 2.0;
    commands.spawn((
        Camera2d,
        Projection::from(OrthographicProjection {
            near: -1000.0,
            far: 1000.0,
            scaling_mode: bevy::render::camera::ScalingMode::AutoMin {
                min_width: WORLD_SIZE,
                min_height: WORLD_SIZE,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_translation(Vec3::new(half, half, 999.0)),
    ));
}
