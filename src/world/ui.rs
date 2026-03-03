//! World-space rendering helpers for islands, ships, and selections.

use macroquad::prelude::*;

use super::island_ui;
use super::ship_ui;
use super::World;

/// Draws simulation entities and active selection highlights in world space.
pub(super) fn draw_world(world: &World, world_units_per_pixel: f32) {
    for island in &world.islands {
        island_ui::draw_island(island, world_units_per_pixel);
    }

    if !world.islands.is_empty() {
        let selected_island_idx = world.selected_island_index.min(world.islands.len() - 1);
        island_ui::draw_island_selection_border(
            &world.islands[selected_island_idx],
            world_units_per_pixel,
        );
    }

    for ship in world.ships.iter().flatten() {
        ship_ui::draw_ship(ship);
    }

    if let Some(ship) = world
        .ships
        .get(world.selected_ship_index)
        .and_then(|slot| slot.as_ref())
    {
        let ring_radius = 14.0 * world_units_per_pixel;
        let ring_thickness = 3.0 * world_units_per_pixel;
        draw_circle_lines(ship.pos.x, ship.pos.y, ring_radius, ring_thickness, RED);
    }
}
