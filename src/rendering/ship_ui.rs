//! Ship visual update system.

use bevy::prelude::*;

use super::island_ui::heatmap_color;
use crate::components::{Commodity, Position, ShipMarker, ShipTrading};
use crate::resources::{HeatmapMode, HeatmapOverlay};

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

/// Estimate ship wealth from trading component: cash + cargo book value - debt.
fn ship_wealth(trading: &ShipTrading) -> f32 {
    let cargo_value = if let Some((_, amount)) = trading.cargo {
        (trading.purchase_price.max(0.0) * amount.max(0.0)).max(0.0)
    } else {
        0.0
    };
    let debt = trading.labor_debt.max(0.0) + trading.wear_debt.max(0.0);
    trading.cash + cargo_value - debt
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
    overlay: Res<HeatmapOverlay>,
) {
    // Pre-compute min/max wealth for ship wealth overlay.
    let (wealth_min, wealth_max) = if matches!(overlay.0, Some(HeatmapMode::ShipWealth)) {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for (_, trading, _, _) in query.iter() {
            let w = ship_wealth(trading);
            lo = lo.min(w);
            hi = hi.max(w);
        }
        if lo >= hi {
            (0.0, 1.0)
        } else {
            (lo, hi)
        }
    } else {
        (0.0, 1.0)
    };

    for (pos, trading, mut transform, material_handle) in query.iter_mut() {
        transform.translation.x = pos.0.x;
        transform.translation.y = pos.0.y;

        if let Some(mat) = materials.get_mut(material_handle.0.id()) {
            mat.color = match overlay.0 {
                Some(HeatmapMode::Commodity(overlay_commodity)) => {
                    let carries = trading
                        .cargo
                        .as_ref()
                        .is_some_and(|(c, _)| *c == overlay_commodity);
                    if carries {
                        Color::srgb(1.0, 0.15, 0.15) // red
                    } else {
                        Color::srgb(0.9, 0.9, 0.9) // white
                    }
                }
                Some(HeatmapMode::ShipWealth) => {
                    let ratio = (ship_wealth(trading) - wealth_min) / (wealth_max - wealth_min);
                    heatmap_color(ratio)
                }
                _ => cargo_color(&trading.cargo),
            };
        }
    }
}
