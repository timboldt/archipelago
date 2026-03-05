//! Input handling — keyboard controls for ship/island selection and time scale.

use bevy::prelude::*;

use crate::components::ShipMarker;
use crate::resources::{SelectionState, TimeScale};

const TIME_SCALE_MIN: f32 = 0.25;
const TIME_SCALE_MAX: f32 = 6.0;
const TIME_SCALE_STEP: f32 = 0.25;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_selection_input, handle_time_scale_input)
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
