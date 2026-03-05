//! Ship visual update system.

use bevy::prelude::*;

use crate::components::{Position, ShipMarker};

/// Update ship transforms to match their Position component.
pub fn update_ship_visuals(mut query: Query<(&Position, &mut Transform), With<ShipMarker>>) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.0.x;
        transform.translation.y = pos.0.y;
    }
}
