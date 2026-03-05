//! Left panel HUD — legend, metrics, performance counters.

use bevy::prelude::*;

use crate::components::{IslandMarker, ShipArchetype, ShipMarker, ShipProfile, COMMODITY_COUNT};
use crate::island::IslandEconomy;
use crate::resources::{FrameTimingsRes, PlanningTuningRes, PERF_HUD_UPDATE_INTERVAL_SECS};

const TARGET_SHIPS_PER_ISLAND: f32 = 12.0;

#[derive(Component)]
pub struct HudText;

pub fn setup_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(14.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
    ));
}

pub fn update_hud(
    mut query: Query<&mut Text, With<HudText>>,
    islands: Query<&IslandEconomy, With<IslandMarker>>,
    ships: Query<&ShipProfile, With<ShipMarker>>,
    planning_tuning: Res<PlanningTuningRes>,
    mut frame_timings: ResMut<FrameTimingsRes>,
    time: Res<Time>,
) {
    let Ok(mut text) = query.single_mut() else {
        return;
    };

    // Aggregate island stats.
    let mut total_inventory = [0.0_f32; COMMODITY_COUNT];
    let mut total_population = 0.0_f32;
    let mut total_cash = 0.0_f32;
    let mut total_infrastructure = 0.0_f32;
    let mut island_count = 0_usize;
    for economy in islands.iter() {
        for (idx, slot) in total_inventory.iter_mut().enumerate() {
            *slot += economy.inventory[idx].max(0.0);
        }
        total_population += economy.population.max(0.0);
        total_cash += economy.cash.max(0.0);
        total_infrastructure += economy.infrastructure_level.max(0.0);
        island_count += 1;
    }
    let avg_infrastructure = if island_count == 0 {
        0.0
    } else {
        total_infrastructure / island_count as f32
    };

    let mut clipper_count = 0_usize;
    let mut freighter_count = 0_usize;
    let mut shorthaul_count = 0_usize;
    let mut ship_count = 0_usize;
    for profile in ships.iter() {
        match profile.archetype {
            ShipArchetype::Clipper => clipper_count += 1,
            ShipArchetype::Freighter => freighter_count += 1,
            ShipArchetype::Shorthaul => shorthaul_count += 1,
        }
        ship_count += 1;
    }

    let target_pop = (island_count.max(1) as f32 * TARGET_SHIPS_PER_ISLAND).max(1.0);
    let friction_mult =
        planning_tuning.0.global_friction_mult * (ship_count as f32 / target_pop).max(0.35);

    // Update perf timings.
    frame_timings.samples = frame_timings.samples.saturating_add(1);
    frame_timings.elapsed_secs += time.delta_secs();
    if frame_timings.elapsed_secs >= PERF_HUD_UPDATE_INTERVAL_SECS && frame_timings.samples > 0 {
        let inv = 1.0 / frame_timings.samples as f32;
        frame_timings.economy_ms = frame_timings.accum_economy_ms * inv;
        frame_timings.movement_ms = frame_timings.accum_movement_ms * inv;
        frame_timings.dock_ms = frame_timings.accum_dock_ms * inv;
        frame_timings.friction_ms = frame_timings.accum_friction_ms * inv;
        frame_timings.total_ms = (frame_timings.accum_economy_ms
            + frame_timings.accum_movement_ms
            + frame_timings.accum_dock_ms
            + frame_timings.accum_friction_ms)
            * inv;
        frame_timings.accum_economy_ms = 0.0;
        frame_timings.accum_movement_ms = 0.0;
        frame_timings.accum_dock_ms = 0.0;
        frame_timings.accum_friction_ms = 0.0;
        frame_timings.accum_total_ms = 0.0;
        frame_timings.samples = 0;
        frame_timings.elapsed_secs = 0.0;
    }

    let resource_names = ["Grain", "Timber", "Iron", "Tools", "Spices"];
    let mut hud_text = String::new();
    hud_text.push_str("Legend\n");
    for (i, name) in resource_names.iter().enumerate() {
        hud_text.push_str(&format!("  {}: {:.0}\n", name, total_inventory[i]));
    }
    hud_text.push_str(&format!(
        "\nShips: {} (Cl/Fr/Sh: {}/{}/{})\n",
        ship_count, clipper_count, freighter_count, shorthaul_count
    ));
    hud_text.push_str(&format!("Population: {:.0}\n", total_population));
    hud_text.push_str(&format!("Cash: {:.0}\n", total_cash));
    hud_text.push_str(&format!("Industry: {:.2}\n", avg_infrastructure));
    hud_text.push_str(&format!("Friction x: {:.2}\n", friction_mult));
    hud_text.push_str("\nPerf (ms)\n");
    hud_text.push_str(&format!("  Economy: {:.2}\n", frame_timings.economy_ms));
    hud_text.push_str(&format!("  Movement: {:.2}\n", frame_timings.movement_ms));
    hud_text.push_str(&format!("  Dock: {:.2}\n", frame_timings.dock_ms));
    hud_text.push_str(&format!("  Friction: {:.2}\n", frame_timings.friction_ms));
    hud_text.push_str(&format!("  Total: {:.2}\n", frame_timings.total_ms));

    **text = hud_text;
}
