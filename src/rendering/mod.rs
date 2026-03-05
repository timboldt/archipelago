//! Rendering plugin — island/ship visuals, camera setup.

pub mod camera;
pub mod island_ui;
pub mod ship_ui;

use bevy::prelude::*;

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, camera::setup_camera);
        app.add_systems(
            Update,
            (
                island_ui::update_island_visuals,
                ship_ui::update_ship_visuals,
            )
                .after(crate::simulation::SimPhase::FleetEvolution),
        );
    }
}
