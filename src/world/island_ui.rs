//! World-space island rendering helpers.
//!
//! This module keeps macroquad drawing concerns out of `Island` simulation logic.

use macroquad::prelude::*;
use strum::IntoEnumIterator;

use crate::island::{Island, Resource, RESOURCE_COUNT};

const POPULATION_DISPLAY_SCALE: f32 = 150.0;
const CASH_DISPLAY_SCALE: f32 = 800.0;
const INFRASTRUCTURE_DISPLAY_MAX: f32 = 2.0;

/// Draws an island's bar chart and status bars in world space.
pub(super) fn draw_island(island: &Island, world_units_per_pixel: f32) {
    let bar_width = 4.0 * world_units_per_pixel;
    let bar_gap = 1.0 * world_units_per_pixel;
    let chart_width =
        (RESOURCE_COUNT as f32 * bar_width) + ((RESOURCE_COUNT as f32 - 1.0) * bar_gap);
    let chart_height = 14.0 * world_units_per_pixel;
    let panel_padding = 2.0 * world_units_per_pixel;
    let border_thickness = 1.0 * world_units_per_pixel;
    let status_gap = 2.0 * world_units_per_pixel;
    let status_row_height = 2.0 * world_units_per_pixel;
    let status_row_spacing = 1.0 * world_units_per_pixel;
    let origin_x = island.pos.x - chart_width * 0.5;
    let origin_y = island.pos.y - chart_height * 0.5;
    let frame_x = origin_x - panel_padding;
    let frame_y = origin_y - panel_padding;
    let frame_w = chart_width + panel_padding * 2.0;
    let frame_h = chart_height + panel_padding * 2.0;
    let status_panel_h = panel_padding * 2.0 + status_row_height * 3.0 + status_row_spacing * 2.0;
    let status_panel_y = frame_y + frame_h + status_gap;

    draw_rectangle(
        frame_x,
        frame_y,
        frame_w,
        frame_h,
        Color::from_rgba(12, 24, 40, 180),
    );

    draw_rectangle(frame_x, frame_y, frame_w, border_thickness, WHITE);
    draw_rectangle(
        frame_x,
        frame_y + frame_h - border_thickness,
        frame_w,
        border_thickness,
        WHITE,
    );
    draw_rectangle(frame_x, frame_y, border_thickness, frame_h, WHITE);
    draw_rectangle(
        frame_x + frame_w - border_thickness,
        frame_y,
        border_thickness,
        frame_h,
        WHITE,
    );

    let max_inventory = island
        .inventory
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1.0);

    for (bar_index, resource) in Resource::iter().enumerate() {
        let value = island.inventory[resource.idx()].max(0.0);
        let normalized = (value / max_inventory).clamp(0.0, 1.0);
        let mut bar_height = normalized * chart_height;
        if value > 0.0 {
            bar_height = bar_height.max(1.0 * world_units_per_pixel);
        }
        let x = origin_x + bar_index as f32 * (bar_width + bar_gap);
        let y = origin_y + chart_height - bar_height;

        let color = match resource {
            Resource::Grain => YELLOW,
            Resource::Timber => GREEN,
            Resource::Iron => DARKGRAY,
            Resource::Tools => RED,
            Resource::Spices => PURPLE,
        };

        draw_rectangle(x, y, bar_width, bar_height, color);
    }

    draw_rectangle(
        frame_x,
        status_panel_y,
        frame_w,
        status_panel_h,
        Color::from_rgba(12, 24, 40, 180),
    );

    draw_rectangle(frame_x, status_panel_y, frame_w, border_thickness, WHITE);
    draw_rectangle(
        frame_x,
        status_panel_y + status_panel_h - border_thickness,
        frame_w,
        border_thickness,
        WHITE,
    );
    draw_rectangle(
        frame_x,
        status_panel_y,
        border_thickness,
        status_panel_h,
        WHITE,
    );
    draw_rectangle(
        frame_x + frame_w - border_thickness,
        status_panel_y,
        border_thickness,
        status_panel_h,
        WHITE,
    );

    let pop_fill =
        (island.population / (island.population + POPULATION_DISPLAY_SCALE)).clamp(0.0, 1.0);
    let cash_fill = (island.cash / (island.cash + CASH_DISPLAY_SCALE)).clamp(0.0, 1.0);
    let infra_fill = (island.infrastructure_level / INFRASTRUCTURE_DISPLAY_MAX).clamp(0.0, 1.0);

    let status_inner_x = frame_x + panel_padding;
    let status_inner_w = (frame_w - panel_padding * 2.0).max(0.0);
    let row1_y = status_panel_y + panel_padding;
    let row2_y = row1_y + status_row_height + status_row_spacing;
    let row3_y = row2_y + status_row_height + status_row_spacing;

    draw_rectangle(
        status_inner_x,
        row1_y,
        status_inner_w,
        status_row_height,
        DARKGRAY,
    );
    draw_rectangle(
        status_inner_x,
        row1_y,
        status_inner_w * pop_fill,
        status_row_height,
        SKYBLUE,
    );

    draw_rectangle(
        status_inner_x,
        row2_y,
        status_inner_w,
        status_row_height,
        DARKGRAY,
    );
    draw_rectangle(
        status_inner_x,
        row2_y,
        status_inner_w * cash_fill,
        status_row_height,
        GOLD,
    );

    draw_rectangle(
        status_inner_x,
        row3_y,
        status_inner_w,
        status_row_height,
        DARKGRAY,
    );
    draw_rectangle(
        status_inner_x,
        row3_y,
        status_inner_w * infra_fill,
        status_row_height,
        ORANGE,
    );
}

/// Draws the selected-island world-space highlight.
pub(super) fn draw_island_selection_border(island: &Island, world_units_per_pixel: f32) {
    let bar_width = 4.0 * world_units_per_pixel;
    let bar_gap = 1.0 * world_units_per_pixel;
    let chart_width =
        (RESOURCE_COUNT as f32 * bar_width) + ((RESOURCE_COUNT as f32 - 1.0) * bar_gap);
    let chart_height = 14.0 * world_units_per_pixel;
    let panel_padding = 2.0 * world_units_per_pixel;
    let status_gap = 2.0 * world_units_per_pixel;
    let status_row_height = 2.0 * world_units_per_pixel;
    let status_row_spacing = 1.0 * world_units_per_pixel;
    let origin_x = island.pos.x - chart_width * 0.5;
    let origin_y = island.pos.y - chart_height * 0.5;
    let frame_x = origin_x - panel_padding;
    let frame_y = origin_y - panel_padding;
    let frame_w = chart_width + panel_padding * 2.0;
    let frame_h = chart_height + panel_padding * 2.0;
    let status_panel_h = panel_padding * 2.0 + status_row_height * 3.0 + status_row_spacing * 2.0;
    let status_panel_y = frame_y + frame_h + status_gap;
    let highlight_thickness = 3.0 * world_units_per_pixel;

    draw_rectangle_lines(frame_x, frame_y, frame_w, frame_h, highlight_thickness, RED);
    draw_rectangle_lines(
        frame_x,
        status_panel_y,
        frame_w,
        status_panel_h,
        highlight_thickness,
        RED,
    );
}
