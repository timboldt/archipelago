//! Input handling — keyboard controls for ship/island selection, time scale,
//! camera pan (WASD / arrow keys) and zoom (scroll wheel / Q/E keys).

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::components::{IslandMarker, Position, SelectedIsland, SelectedShip, ShipMarker};
use crate::island::spawn::WORLD_SIZE;
use crate::resources::TimeScale;

/// Tracks mouse drag state for panning vs click detection.
#[derive(Resource, Default)]
struct DragState {
    /// Last cursor position in screen pixels while dragging.
    last_pos: Option<Vec2>,
    /// Starting cursor position when button was first pressed.
    start_pos: Option<Vec2>,
    /// Total distance dragged (screen pixels).
    drag_distance: f32,
}

/// Compute world-units-per-screen-pixel for the current camera setup.
///
/// With `ScalingMode::AutoMin`, the projection maps `min_width`/`min_height`
/// world units to the window, then `ortho.scale` further scales that.
fn world_units_per_pixel(ortho: &OrthographicProjection, window: &Window) -> f32 {
    let window_size = Vec2::new(window.width(), window.height());
    // AutoMin picks whichever axis is tighter.
    let base = match ortho.scaling_mode {
        bevy::render::camera::ScalingMode::AutoMin {
            min_width,
            min_height,
        } => {
            let scale_x = min_width / window_size.x;
            let scale_y = min_height / window_size.y;
            scale_x.max(scale_y)
        }
        _ => 1.0,
    };
    base * ortho.scale
}

/// Convert screen cursor position to world coordinates.
fn screen_to_world(
    cursor_pos: Vec2,
    cam_transform: &Transform,
    ortho: &OrthographicProjection,
    window: &Window,
) -> Vec2 {
    let wpp = world_units_per_pixel(ortho, window);
    let window_size = Vec2::new(window.width(), window.height());
    let offset = cursor_pos - window_size * 0.5;
    Vec2::new(
        cam_transform.translation.x + offset.x * wpp,
        cam_transform.translation.y - offset.y * wpp,
    )
}

/// Maximum screen-pixel movement to count as a click (not a drag).
const CLICK_THRESHOLD: f32 = 5.0;
/// How close (in world units) a click must be to select a ship.
const SHIP_CLICK_RADIUS: f32 = 15.0;
/// How close (in world units) a click must be to select an island.
const ISLAND_CLICK_RADIUS: f32 = 30.0;

const TIME_SCALE_MIN: f32 = 0.25;
const TIME_SCALE_MAX: f32 = 6.0;
const TIME_SCALE_STEP: f32 = 0.25;

const PAN_SPEED: f32 = 2000.0;
const ZOOM_SPEED_KEY: f32 = 2.0;
const ZOOM_SPEED_SCROLL: f32 = 0.08;
const ZOOM_SPEED_SCROLL_PIXEL: f32 = 0.005;
const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 5.0;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragState>().add_systems(
            Update,
            (
                ensure_selection_exists,
                handle_selection_input,
                handle_click_selection,
                handle_time_scale_input,
                handle_camera_input,
            )
                .chain()
                .before(crate::simulation::SimPhase::TickAdvance),
        );
    }
}

/// If nothing is selected (e.g. the selected entity was despawned),
/// automatically select the first ship.
fn ensure_selection_exists(
    mut commands: Commands,
    selected_ships: Query<(), With<SelectedShip>>,
    all_ships: Query<Entity, With<ShipMarker>>,
    selected_islands: Query<(), With<SelectedIsland>>,
) {
    if selected_ships.is_empty() && selected_islands.is_empty() {
        let mut sorted: Vec<_> = all_ships.iter().collect();
        sorted.sort();
        if let Some(&entity) = sorted.first() {
            commands.entity(entity).insert(SelectedShip);
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
            // Deselect ship, cycle island.
            for e in selected_ship.iter() {
                commands.entity(e).remove::<SelectedShip>();
            }
            cycle_selection::<IslandMarker, SelectedIsland>(
                &mut commands,
                &selected_island,
                &all_islands,
                -1,
            );
        } else {
            // Deselect island, cycle ship.
            for e in selected_island.iter() {
                commands.entity(e).remove::<SelectedIsland>();
            }
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
            for e in selected_ship.iter() {
                commands.entity(e).remove::<SelectedShip>();
            }
            cycle_selection::<IslandMarker, SelectedIsland>(
                &mut commands,
                &selected_island,
                &all_islands,
                1,
            );
        } else {
            for e in selected_island.iter() {
                commands.entity(e).remove::<SelectedIsland>();
            }
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

/// Select the nearest ship or island when the user clicks (not drags).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn handle_click_selection(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    drag_state: Res<DragState>,
    windows: Query<&Window>,
    camera_query: Query<(&Transform, &Projection), With<Camera2d>>,
    ships: Query<(Entity, &Position), With<ShipMarker>>,
    islands: Query<(Entity, &Position), (With<IslandMarker>, Without<ShipMarker>)>,
    selected_ships: Query<Entity, With<SelectedShip>>,
    selected_islands: Query<Entity, With<SelectedIsland>>,
) {
    // Fire on mouse release, only if it was a click (not a drag).
    if !mouse_buttons.just_released(MouseButton::Left) {
        return;
    }
    if drag_state.drag_distance > CLICK_THRESHOLD {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((cam_transform, projection)) = camera_query.single() else {
        return;
    };
    let Projection::Orthographic(ref ortho) = *projection else {
        return;
    };

    let world_pos = screen_to_world(cursor_pos, cam_transform, ortho, window);
    let wpp = world_units_per_pixel(ortho, window);

    // Check ships first (they're smaller and on top).
    let mut closest_ship: Option<(Entity, f32)> = None;
    for (entity, pos) in ships.iter() {
        let dist = world_pos.distance(pos.0);
        if dist < SHIP_CLICK_RADIUS * wpp
            && (closest_ship.is_none() || dist < closest_ship.unwrap().1)
        {
            closest_ship = Some((entity, dist));
        }
    }
    if let Some((entity, _)) = closest_ship {
        for old in selected_ships.iter() {
            commands.entity(old).remove::<SelectedShip>();
        }
        // Deselect any island — selection is mutually exclusive.
        for old in selected_islands.iter() {
            commands.entity(old).remove::<SelectedIsland>();
        }
        commands.entity(entity).insert(SelectedShip);
        return;
    }

    // Then check islands.
    let mut closest_island: Option<(Entity, f32)> = None;
    for (entity, pos) in islands.iter() {
        let dist = world_pos.distance(pos.0);
        if dist < ISLAND_CLICK_RADIUS * wpp
            && (closest_island.is_none() || dist < closest_island.unwrap().1)
        {
            closest_island = Some((entity, dist));
        }
    }
    if let Some((entity, _)) = closest_island {
        for old in selected_islands.iter() {
            commands.entity(old).remove::<SelectedIsland>();
        }
        // Deselect any ship — selection is mutually exclusive.
        for old in selected_ships.iter() {
            commands.entity(old).remove::<SelectedShip>();
        }
        commands.entity(entity).insert(SelectedIsland);
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
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    mut scroll_events: EventReader<MouseWheel>,
    mut drag_state: ResMut<DragState>,
    windows: Query<&Window>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_query.single_mut() else {
        return;
    };

    let Projection::Orthographic(ref mut ortho) = *projection else {
        return;
    };

    let dt = time.delta_secs();
    let window = windows.single().unwrap();
    let wpp = world_units_per_pixel(ortho, window);

    // ── Mouse drag panning ───────────────────────────────────────────
    if mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if drag_state.start_pos.is_none() {
                drag_state.start_pos = Some(cursor_pos);
                drag_state.drag_distance = 0.0;
            }
            if let Some(last) = drag_state.last_pos {
                let delta = cursor_pos - last;
                drag_state.drag_distance += delta.length();
                // Screen-pixel delta → world units (y is flipped).
                transform.translation.x -= delta.x * wpp;
                transform.translation.y += delta.y * wpp;
            }
            drag_state.last_pos = Some(cursor_pos);
        }
    } else {
        drag_state.last_pos = None;
        drag_state.start_pos = None;
    }

    // ── Keyboard pan (WASD / arrows) ─────────────────────────────────
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

    // ── Keyboard zoom (Q/E) ──────────────────────────────────────────
    if keys.pressed(KeyCode::KeyQ) {
        ortho.scale *= 1.0 + ZOOM_SPEED_KEY * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        ortho.scale *= 1.0 - ZOOM_SPEED_KEY * dt;
    }

    // ── Scroll-wheel zoom toward cursor ──────────────────────────────
    let cursor_world = window
        .cursor_position()
        .map(|cursor_pos| screen_to_world(cursor_pos, &transform, ortho, window));

    for event in scroll_events.read() {
        let zoom_amount = match event.unit {
            MouseScrollUnit::Line => event.y * ZOOM_SPEED_SCROLL,
            MouseScrollUnit::Pixel => event.y * ZOOM_SPEED_SCROLL_PIXEL,
        };
        let zoom_factor = 1.0 - zoom_amount;
        let old_scale = ortho.scale;
        ortho.scale = (ortho.scale * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let actual_factor = ortho.scale / old_scale;

        // Adjust camera so the world point under the cursor stays put.
        if let Some(world_pos) = cursor_world {
            let cam_pos = Vec2::new(transform.translation.x, transform.translation.y);
            let new_cam = world_pos + (cam_pos - world_pos) * actual_factor;
            transform.translation.x = new_cam.x;
            transform.translation.y = new_cam.y;
        }
    }

    // ── Clamp ────────────────────────────────────────────────────────
    ortho.scale = ortho.scale.clamp(MIN_ZOOM, MAX_ZOOM);
    transform.translation.x = transform.translation.x.clamp(0.0, WORLD_SIZE);
    transform.translation.y = transform.translation.y.clamp(0.0, WORLD_SIZE);
}
