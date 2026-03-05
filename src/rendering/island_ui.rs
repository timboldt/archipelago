//! Island visual update system.
//!
//! Islands are rendered as sprite-based bar charts. On startup we spawn child
//! entities for the bars; each frame we update their scales and colors.

use bevy::prelude::*;

use crate::components::{IslandMarker, Position};

/// Update island transforms to match their Position component.
/// (Island visuals are just their Transform; actual bar-chart rendering
/// would require sprite children — for now we render islands as simple
/// colored rectangles via Sprite.)
pub fn update_island_visuals(mut query: Query<(&Position, &mut Transform), With<IslandMarker>>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.0.x;
        transform.translation.y = pos.0.y;
    }
}
