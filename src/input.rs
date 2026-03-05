//! Input handling — keyboard controls for ship/island selection, time scale,
//! camera pan (WASD / arrow keys) and zoom (scroll wheel / Q/E keys).

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::components::ShipMarker;
use crate::island::spawn::WORLD_SIZE;
use crate::resources::{SelectionState, TimeScale};

const TIME_SCALE_MIN: f32 = 0.25;
const TIME_SCALE_MAX: f32 = 6.0;
const TIME_SCALE_STEP: f32 = 0.25;

const PAN_SPEED: f32 = 800.0;
const ZOOM_SPEED_KEY: f32 = 2.0;
const ZOOM_SPEED_SCROLL: f32 = 0.1;
const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 5.0;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_selection_input,
                handle_time_scale_input,
                handle_camera_input,
            )
                .before(crate::simulation::SimPhase::TickAdvance),
        );
    }
}

fn handle_selection_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<SelectionState>,
    ships: Query<(), With<ShipMarker>>,
    islands: Query<(), With<crate::components::IslandMarker>>,
) {
    let ship_count = ships.iter().count();
    let island_count = islands.iter().count();
    let shift_down = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if keys.just_pressed(KeyCode::BracketLeft) {
        if shift_down {
            // Previous island.
            if island_count > 0 {
                if selection.selected_island_index == 0 {
                    selection.selected_island_index = island_count - 1;
                } else {
                    selection.selected_island_index -= 1;
                }
            }
        } else {
            // Previous ship.
            if ship_count > 0 {
                if selection.selected_ship_index == 0 {
                    selection.selected_ship_index = ship_count - 1;
                } else {
                    selection.selected_ship_index -= 1;
                }
            }
        }
    }

    if keys.just_pressed(KeyCode::BracketRight) {
        if shift_down {
            // Next island.
            if island_count > 0 {
                selection.selected_island_index =
                    (selection.selected_island_index + 1) % island_count;
            }
        } else {
            // Next ship.
            if ship_count > 0 {
                selection.selected_ship_index = (selection.selected_ship_index + 1) % ship_count;
            }
        }
    }
}

fn handle_time_scale_input(keys: Res<ButtonInput<KeyCode>>, mut time_scale: ResMut<TimeScale>) {
    if keys.just_pressed(KeyCode::Minus) {
        time_scale.0 = (time_scale.0 - TIME_SCALE_STEP).clamp(TIME_SCALE_MIN, TIME_SCALE_MAX);
    }
    if keys.just_pressed(KeyCode::Equal) {
        time_scale.0 = (time_scale.0 + TIME_SCALE_STEP).clamp(TIME_SCALE_MIN, TIME_SCALE_MAX);
    }
}

fn handle_camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut scroll_events: EventReader<MouseWheel>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Orthographic(ref mut ortho) = *projection else {
        return;
    };

    let dt = time.delta_secs();

    // Pan with WASD or arrow keys (scaled by current zoom so panning feels consistent).
    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan.x += 1.0;
    }
    if pan != Vec2::ZERO {
        let pan_amount = pan.normalize() * PAN_SPEED * ortho.scale * dt;
        transform.translation.x += pan_amount.x;
        transform.translation.y += pan_amount.y;
    }

    // Zoom with Q/E keys.
    if keys.pressed(KeyCode::KeyQ) {
        ortho.scale *= 1.0 + ZOOM_SPEED_KEY * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        ortho.scale *= 1.0 - ZOOM_SPEED_KEY * dt;
    }

    // Zoom with scroll wheel.
    for event in scroll_events.read() {
        ortho.scale *= 1.0 - event.y * ZOOM_SPEED_SCROLL;
    }

    ortho.scale = ortho.scale.clamp(MIN_ZOOM, MAX_ZOOM);

    // Clamp camera position so the world stays in view.
    // Allow panning to edges: camera center can go from 0 to WORLD_SIZE.
    transform.translation.x = transform.translation.x.clamp(0.0, WORLD_SIZE);
    transform.translation.y = transform.translation.y.clamp(0.0, WORLD_SIZE);
}
