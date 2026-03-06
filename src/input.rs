//! Input handling — keyboard controls for ship/island selection, time scale,
//! camera pan (WASD / arrow keys) and zoom (scroll wheel / Q/E keys).

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::components::{IslandMarker, SelectedIsland, SelectedShip, ShipMarker};
use crate::island::spawn::WORLD_SIZE;
use crate::resources::TimeScale;

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
                ensure_selection_exists,
                handle_selection_input,
                handle_time_scale_input,
                handle_camera_input,
            )
                .chain()
                .before(crate::simulation::SimPhase::TickAdvance),
        );
    }
}

/// If no ship/island is selected (e.g. the selected entity was despawned),
/// automatically select the first one.
fn ensure_selection_exists(
    mut commands: Commands,
    selected_ships: Query<(), With<SelectedShip>>,
    all_ships: Query<Entity, With<ShipMarker>>,
    selected_islands: Query<(), With<SelectedIsland>>,
    all_islands: Query<Entity, With<IslandMarker>>,
) {
    if selected_ships.is_empty() {
        let mut sorted: Vec<_> = all_ships.iter().collect();
        sorted.sort();
        if let Some(&entity) = sorted.first() {
            commands.entity(entity).insert(SelectedShip);
        }
    }
    if selected_islands.is_empty() {
        let mut sorted: Vec<_> = all_islands.iter().collect();
        sorted.sort();
        if let Some(&entity) = sorted.first() {
            commands.entity(entity).insert(SelectedIsland);
        }
    }
}

fn handle_selection_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    selected_ship: Query<Entity, With<SelectedShip>>,
    all_ships: Query<Entity, With<ShipMarker>>,
    selected_island: Query<Entity, With<SelectedIsland>>,
    all_islands: Query<Entity, With<IslandMarker>>,
) {
    let shift_down = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if keys.just_pressed(KeyCode::BracketLeft) {
        if shift_down {
            cycle_selection::<IslandMarker, SelectedIsland>(
                &mut commands,
                &selected_island,
                &all_islands,
                -1,
            );
        } else {
            cycle_selection::<ShipMarker, SelectedShip>(
                &mut commands,
                &selected_ship,
                &all_ships,
                -1,
            );
        }
    }

    if keys.just_pressed(KeyCode::BracketRight) {
        if shift_down {
            cycle_selection::<IslandMarker, SelectedIsland>(
                &mut commands,
                &selected_island,
                &all_islands,
                1,
            );
        } else {
            cycle_selection::<ShipMarker, SelectedShip>(
                &mut commands,
                &selected_ship,
                &all_ships,
                1,
            );
        }
    }
}

fn cycle_selection<M: Component, S: Component + Default>(
    commands: &mut Commands,
    selected: &Query<Entity, With<S>>,
    all: &Query<Entity, With<M>>,
    direction: i32,
) {
    let mut sorted: Vec<Entity> = all.iter().collect();
    sorted.sort();
    if sorted.is_empty() {
        return;
    }

    let current = selected.single().ok();
    let current_idx = current
        .and_then(|e| sorted.iter().position(|&s| s == e))
        .unwrap_or(0);

    let new_idx = if direction > 0 {
        (current_idx + 1) % sorted.len()
    } else if current_idx == 0 {
        sorted.len() - 1
    } else {
        current_idx - 1
    };

    if let Some(old) = current {
        commands.entity(old).remove::<S>();
    }
    commands.entity(sorted[new_idx]).insert(S::default());
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
