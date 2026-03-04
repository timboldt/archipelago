//! Screen-space HUD and inspector panel rendering.
//!
//! Rendering consumes precomputed view models to keep drawing code focused on
//! presentation and layout only.

use macroquad::prelude::*;

use crate::island::RESOURCE_COUNT;

use super::view_model;
use super::World;

const LEFT_PANEL_X: f32 = 14.0;
const LEFT_PANEL_Y: f32 = 14.0;
const LEFT_PANEL_W: f32 = 260.0;
const LEFT_PANEL_H: f32 = 404.0;

const INSPECT_MARGIN: f32 = 14.0;
const INSPECT_W: f32 = 320.0;
const SHIP_INSPECT_H: f32 = 226.0;
const ISLAND_INSPECT_H: f32 = 208.0;
const INSPECT_GAP: f32 = 12.0;

/// Draws the left legend/perf panel and right ship/island inspectors.
pub(super) fn draw_ui(world: &World) {
    let summary = view_model::hud_summary(world);

    let panel_x = LEFT_PANEL_X;
    let panel_y = LEFT_PANEL_Y;
    let panel_w = LEFT_PANEL_W;
    let panel_h = LEFT_PANEL_H;

    draw_rectangle(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        Color::from_rgba(8, 16, 30, 210),
    );
    draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 2.0, LIGHTGRAY);
    draw_text("Legend", panel_x + 10.0, panel_y + 22.0, 24.0, WHITE);

    let entries = [
        ("Grain", YELLOW),
        ("Timber", GREEN),
        ("Iron", DARKGRAY),
        ("Tools", RED),
        ("Spices", PURPLE),
        ("Empty ship", WHITE),
    ];

    for (i, (label, color)) in entries.iter().enumerate() {
        let y = panel_y + 42.0 + i as f32 * 16.0;
        draw_rectangle(panel_x + 10.0, y - 10.0, 10.0, 10.0, *color);
        draw_rectangle_lines(panel_x + 10.0, y - 10.0, 10.0, 10.0, 1.0, GRAY);
        if i < RESOURCE_COUNT {
            let counter = format!("{}: {:.0}", label, summary.total_inventory[i]);
            draw_text(&counter, panel_x + 28.0, y, 18.0, WHITE);
        } else {
            draw_text(label, panel_x + 28.0, y, 18.0, WHITE);
        }
    }

    let shape_legend_y = panel_y + 136.0;
    draw_text("Ship Shapes", panel_x + 10.0, shape_legend_y, 18.0, WHITE);

    let icon_y = shape_legend_y + 14.0;
    let clipper_x = panel_x + 14.0;
    let freighter_x = panel_x + 92.0;
    let shorthaul_x = panel_x + 188.0;

    let clipper_top = vec2(clipper_x, icon_y - 8.0);
    let clipper_left = vec2(clipper_x - 7.0, icon_y + 6.0);
    let clipper_right = vec2(clipper_x + 7.0, icon_y + 6.0);
    draw_triangle(clipper_top, clipper_left, clipper_right, WHITE);
    draw_triangle_lines(clipper_top, clipper_left, clipper_right, 1.5, LIGHTGRAY);
    draw_text("Clipper", clipper_x + 12.0, icon_y + 4.0, 16.0, WHITE);

    draw_rectangle(freighter_x - 7.0, icon_y - 7.0, 14.0, 14.0, WHITE);
    draw_rectangle_lines(freighter_x - 7.0, icon_y - 7.0, 14.0, 14.0, 1.5, LIGHTGRAY);
    draw_text("Freighter", freighter_x + 12.0, icon_y + 4.0, 16.0, WHITE);

    draw_circle(shorthaul_x, icon_y, 7.0, WHITE);
    draw_circle_lines(shorthaul_x, icon_y, 7.0, 1.5, LIGHTGRAY);
    draw_text("Shorthaul", shorthaul_x + 12.0, icon_y + 4.0, 16.0, WHITE);

    let pop_text = format!("Population: {:.0}", summary.total_population);
    let cash_text = format!("Cash: {:.0}", summary.total_cash);
    let infra_text = format!("Industry: {:.2}", summary.avg_infrastructure);
    let mile_cost_text = format!("Friction x: {:.2}", summary.friction_mult);
    let ship_count_text = format!("Ships: {}", summary.active_ship_count);
    let archetype_text = format!(
        "Cl/Fr/Sh: {}/{}/{}",
        summary.clipper_count, summary.freighter_count, summary.shorthaul_count
    );
    let perf_header_text = "Perf (ms)";
    let perf_economy_text = format!("Economy: {:.2}", summary.perf_economy_ms);
    let perf_movement_text = format!("Movement: {:.2}", summary.perf_movement_ms);
    let perf_dock_text = format!("Dock: {:.2}", summary.perf_dock_ms);
    let perf_friction_text = format!("Friction: {:.2}", summary.perf_friction_ms);
    let perf_total_text = format!("Total: {:.2}", summary.perf_total_ms);
    draw_text(&pop_text, panel_x + 10.0, panel_y + 172.0, 18.0, WHITE);
    draw_text(&cash_text, panel_x + 10.0, panel_y + 190.0, 18.0, WHITE);
    draw_text(&infra_text, panel_x + 10.0, panel_y + 208.0, 18.0, WHITE);
    draw_text(
        &mile_cost_text,
        panel_x + 10.0,
        panel_y + 226.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_count_text,
        panel_x + 10.0,
        panel_y + 244.0,
        18.0,
        WHITE,
    );
    draw_text(
        &archetype_text,
        panel_x + 10.0,
        panel_y + 262.0,
        18.0,
        WHITE,
    );
    draw_text(
        perf_header_text,
        panel_x + 10.0,
        panel_y + 288.0,
        18.0,
        WHITE,
    );
    draw_text(
        &perf_economy_text,
        panel_x + 10.0,
        panel_y + 306.0,
        17.0,
        WHITE,
    );
    draw_text(
        &perf_movement_text,
        panel_x + 10.0,
        panel_y + 324.0,
        17.0,
        WHITE,
    );
    draw_text(
        &perf_dock_text,
        panel_x + 10.0,
        panel_y + 342.0,
        17.0,
        WHITE,
    );
    draw_text(
        &perf_friction_text,
        panel_x + 10.0,
        panel_y + 360.0,
        17.0,
        WHITE,
    );
    draw_text(
        &perf_total_text,
        panel_x + 10.0,
        panel_y + 378.0,
        17.0,
        WHITE,
    );
    draw_text(
        "F9: Save debug snapshot",
        panel_x + 10.0,
        panel_y + 398.0,
        16.0,
        LIGHTGRAY,
    );

    let inspect_w = INSPECT_W;
    let inspect_h = SHIP_INSPECT_H;
    let inspect_x = (screen_width() - inspect_w - INSPECT_MARGIN).max(INSPECT_MARGIN);
    let inspect_y = INSPECT_MARGIN;
    draw_rectangle(
        inspect_x,
        inspect_y,
        inspect_w,
        inspect_h,
        Color::from_rgba(8, 16, 30, 210),
    );
    draw_rectangle_lines(inspect_x, inspect_y, inspect_w, inspect_h, 2.0, LIGHTGRAY);
    draw_text(
        "Selected Ship",
        inspect_x + 10.0,
        inspect_y + 22.0,
        24.0,
        WHITE,
    );

    let ship_view = view_model::ship_inspector_view(world, summary.active_ship_count);
    if !ship_view.has_ship {
        draw_text("No ships", inspect_x + 10.0, inspect_y + 48.0, 18.0, WHITE);
        return;
    }

    draw_text(
        &ship_view.ship_id_text,
        inspect_x + 10.0,
        inspect_y + 48.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.archetype_text,
        inspect_x + 10.0,
        inspect_y + 66.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.status_text,
        inspect_x + 10.0,
        inspect_y + 84.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.speed_text,
        inspect_x + 10.0,
        inspect_y + 102.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.cargo_text,
        inspect_x + 10.0,
        inspect_y + 120.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.upkeep_text,
        inspect_x + 10.0,
        inspect_y + 138.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.cash_text,
        inspect_x + 10.0,
        inspect_y + 156.0,
        18.0,
        WHITE,
    );
    draw_text(
        &ship_view.cargo_resource_text,
        inspect_x + 10.0,
        inspect_y + 174.0,
        17.0,
        WHITE,
    );
    draw_text(
        "[ / ]: Prev / Next ship",
        inspect_x + 10.0,
        inspect_y + 214.0,
        16.0,
        LIGHTGRAY,
    );

    let island_hud_y = inspect_y + inspect_h + INSPECT_GAP;
    draw_rectangle(
        inspect_x,
        island_hud_y,
        inspect_w,
        ISLAND_INSPECT_H,
        Color::from_rgba(8, 16, 30, 210),
    );
    draw_rectangle_lines(
        inspect_x,
        island_hud_y,
        inspect_w,
        ISLAND_INSPECT_H,
        2.0,
        LIGHTGRAY,
    );
    draw_text(
        "Selected Island",
        inspect_x + 10.0,
        island_hud_y + 22.0,
        24.0,
        WHITE,
    );

    let island_view = view_model::island_inspector_view(world);
    if !island_view.has_island {
        draw_text(
            "No islands",
            inspect_x + 10.0,
            island_hud_y + 48.0,
            18.0,
            WHITE,
        );
        return;
    }

    draw_text(
        &island_view.island_id_text,
        inspect_x + 10.0,
        island_hud_y + 48.0,
        18.0,
        WHITE,
    );
    draw_text(
        &island_view.island_pop_text,
        inspect_x + 10.0,
        island_hud_y + 66.0,
        18.0,
        WHITE,
    );
    draw_text(
        &island_view.island_cash_text,
        inspect_x + 10.0,
        island_hud_y + 84.0,
        18.0,
        WHITE,
    );
    draw_text(
        &island_view.island_infra_text,
        inspect_x + 10.0,
        island_hud_y + 102.0,
        18.0,
        WHITE,
    );
    draw_text(
        &island_view.inv_text,
        inspect_x + 10.0,
        island_hud_y + 128.0,
        17.0,
        WHITE,
    );
    draw_text(
        &island_view.price_text,
        inspect_x + 10.0,
        island_hud_y + 154.0,
        17.0,
        WHITE,
    );
    draw_text(
        "{ / }: Prev / Next island",
        inspect_x + 10.0,
        island_hud_y + 196.0,
        16.0,
        LIGHTGRAY,
    );
}
