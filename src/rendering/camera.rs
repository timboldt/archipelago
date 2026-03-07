//! Camera setup — computes view bounds from actual island positions.

use bevy::prelude::*;

use crate::resources::IslandPositions;

pub fn setup_camera(mut commands: Commands, island_positions: Res<IslandPositions>) {
    // Compute bounding box of all islands (including mainland if present).
    let positions = &island_positions.0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for p in positions {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }

    // Add margin around the bounding box.
    let margin = 500.0;
    min_x -= margin;
    min_y -= margin;
    max_x += margin;
    max_y += margin;

    let view_width = max_x - min_x;
    let view_height = max_y - min_y;
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;

    commands.spawn((
        Camera2d,
        Projection::from(OrthographicProjection {
            near: -1000.0,
            far: 1000.0,
            scaling_mode: bevy::render::camera::ScalingMode::AutoMin {
                min_width: view_width,
                min_height: view_height,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_translation(Vec3::new(center_x, center_y, 999.0)),
    ));
}
