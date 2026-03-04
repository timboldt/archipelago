//! World-space ship rendering helpers.
//!
//! This module keeps macroquad drawing concerns out of `Ship` simulation logic.

use macroquad::prelude::*;

use crate::island::Resource;
use crate::ship::{Ship, ShipArchetype};

/// Draws a ship marker using archetype-specific shape and dominant-cargo color.
pub(super) fn draw_ship(ship: &Ship) {
    let fill = match ship.current_cargo().map(|(resource, _)| resource) {
        Some(resource) => match resource {
            Resource::Grain => YELLOW,
            Resource::Timber => GREEN,
            Resource::Iron => DARKGRAY,
            Resource::Tools => RED,
            Resource::Spices => PURPLE,
        },
        None => WHITE,
    };

    match ship.archetype() {
        ShipArchetype::Freighter => {
            let half_size = 7.0;
            draw_rectangle(
                ship.pos.x - half_size,
                ship.pos.y - half_size,
                half_size * 2.0,
                half_size * 2.0,
                fill,
            );
            draw_rectangle_lines(
                ship.pos.x - half_size,
                ship.pos.y - half_size,
                half_size * 2.0,
                half_size * 2.0,
                2.0,
                LIGHTGRAY,
            );
        }
        ShipArchetype::Clipper => {
            let top = vec2(ship.pos.x, ship.pos.y - 8.0);
            let left = vec2(ship.pos.x - 7.0, ship.pos.y + 6.0);
            let right = vec2(ship.pos.x + 7.0, ship.pos.y + 6.0);
            draw_triangle(top, left, right, fill);
            draw_triangle_lines(top, left, right, 2.0, LIGHTGRAY);
        }
        ShipArchetype::Shorthaul => {
            draw_circle(ship.pos.x, ship.pos.y, 8.0, fill);
            draw_circle_lines(ship.pos.x, ship.pos.y, 8.0, 2.0, LIGHTGRAY);
        }
    }
}
