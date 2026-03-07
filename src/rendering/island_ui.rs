//! Island visual update system.
//!
//! Islands are rendered as sprite-based bar charts. On startup we spawn child
//! entities for the bars; each frame we update their scales and colors.

use bevy::prelude::*;

use crate::components::{IslandBaseColor, IslandMarker, MainlandMarker, Position};
use crate::island::IslandEconomy;
use crate::resources::{HeatmapMode, HeatmapOverlay};

/// Map a 0.0–1.0 fill ratio to a red→yellow→green gradient.
pub fn heatmap_color(ratio: f32) -> Color {
    let t = ratio.clamp(0.0, 1.0);
    if t < 0.5 {
        // red (0.0) → yellow (0.5)
        let s = t * 2.0;
        Color::srgb(1.0, s, 0.0)
    } else {
        // yellow (0.5) → green (1.0)
        let s = (t - 0.5) * 2.0;
        Color::srgb(1.0 - s, 1.0, 0.0)
    }
}

/// Extract the raw heatmap value for a given mode from an island economy.
fn heatmap_value(economy: &IslandEconomy, mode: HeatmapMode) -> f32 {
    match mode {
        HeatmapMode::Commodity(c) => {
            let idx = c.idx();
            let cap = economy.resource_capacity[idx];
            if cap > 0.0 {
                economy.inventory[idx] / cap
            } else {
                0.0
            }
        }
        HeatmapMode::CashPerCapita => {
            if economy.population > 0.0 {
                economy.cash / economy.population
            } else {
                0.0
            }
        }
        HeatmapMode::Population => economy.population,
        HeatmapMode::Infrastructure => economy.infrastructure_level,
        HeatmapMode::ShipWealth => 0.0,
    }
}

#[allow(clippy::type_complexity)]
pub fn update_island_visuals(
    mut query: Query<
        (
            &Position,
            &mut Transform,
            &IslandBaseColor,
            &IslandEconomy,
            &MeshMaterial2d<ColorMaterial>,
            Has<MainlandMarker>,
        ),
        With<IslandMarker>,
    >,
    overlay: Res<HeatmapOverlay>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // For commodity overlays the ratio is already 0–1 (inventory/capacity).
    // For other overlays we normalize relative to the max across archipelago islands
    // (excluding mainland so it doesn't skew the scale).
    let needs_normalize = matches!(
        overlay.0,
        Some(HeatmapMode::CashPerCapita | HeatmapMode::Population | HeatmapMode::Infrastructure)
    );

    let (min_val, max_val) = if needs_normalize {
        let mode = overlay.0.unwrap();
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for (_, _, _, eco, _, is_mainland) in query.iter() {
            if is_mainland {
                continue;
            }
            let v = heatmap_value(eco, mode);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if lo >= hi {
            (0.0, 1.0)
        } else {
            (lo, hi)
        }
    } else {
        (0.0, 1.0)
    };

    for (pos, mut transform, base_color, economy, material_handle, _) in query.iter_mut() {
        transform.translation.x = pos.0.x;
        transform.translation.y = pos.0.y;

        if let Some(mat) = materials.get_mut(&material_handle.0) {
            match overlay.0 {
                Some(HeatmapMode::ShipWealth) | None => {
                    mat.color = base_color.0;
                }
                Some(mode) => {
                    let ratio = (heatmap_value(economy, mode) - min_val) / (max_val - min_val);
                    mat.color = heatmap_color(ratio);
                }
            }
        }
    }
}
