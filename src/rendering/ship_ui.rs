//! Ship visual update system.

use bevy::prelude::*;

use crate::components::{Commodity, Position, ShipMarker, ShipTrading};

/// Color for each commodity cargo, plus a default for empty ships.
fn cargo_color(cargo: &Option<(Commodity, f32)>) -> Color {
    match cargo {
        Some((Commodity::Grain, _)) => Color::srgb(1.0, 0.85, 0.2), // golden yellow
        Some((Commodity::Timber, _)) => Color::srgb(0.55, 0.35, 0.15), // brown
        Some((Commodity::Iron, _)) => Color::srgb(0.6, 0.6, 0.65),  // steel grey
        Some((Commodity::Tools, _)) => Color::srgb(0.2, 0.6, 1.0),  // blue
        Some((Commodity::Spices, _)) => Color::srgb(0.9, 0.3, 0.2), // red-orange
        None => Color::srgb(0.9, 0.9, 0.9),                         // white (empty)
    }
}

/// Update ship transforms and colors to match their current state.
pub fn update_ship_visuals(
    mut query: Query<
        (
            &Position,
            &ShipTrading,
            &mut Transform,
            &mut MeshMaterial2d<ColorMaterial>,
        ),
        With<ShipMarker>,
    >,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (pos, trading, mut transform, material_handle) in query.iter_mut() {
        transform.translation.x = pos.0.x;
        transform.translation.y = pos.0.y;

        if let Some(mat) = materials.get_mut(material_handle.0.id()) {
            mat.color = cargo_color(&trading.cargo);
        }
    }
}
