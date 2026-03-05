//! UI plugin — HUD panels and inspectors using Bevy UI.

pub mod hud;
pub mod inspector;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, hud::setup_hud);
        app.add_systems(
            Update,
            (
                hud::update_hud,
                inspector::update_ship_inspector,
                inspector::update_island_inspector,
            )
                .after(crate::simulation::SimPhase::FleetEvolution),
        );
    }
}
