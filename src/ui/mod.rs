//! UI plugin — HUD panels, inspectors, and legend using Bevy UI.

pub mod hud;
pub mod inspector;
pub mod legend;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<legend::LegendVisible>();
        app.add_systems(Startup, (hud::setup_hud, legend::setup_legend));
        app.add_systems(
            Update,
            (
                hud::update_hud,
                hud::update_overlay_label,
                inspector::update_ship_inspector,
                inspector::update_island_inspector,
                legend::toggle_legend,
                legend::update_legend,
            )
                .after(crate::simulation::SimPhase::FleetEvolution),
        );
    }
}
