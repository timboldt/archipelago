use macroquad::prelude::*;

mod island;
mod ship;
mod world;

use ship::PlanningTuning;
use world::{World, WORLD_SIZE};

#[macroquad::main("Archipelago")]
async fn main() {
    const NUM_ISLANDS: usize = 50;
    const NUM_SHIPS: usize = 500;

    const CONFIDENCE_DECAY_K: f32 = 0.003;
    const SPECULATION_FLOOR: f32 = 0.40;
    const SPECULATION_STALENESS_SCALE: f32 = 0.50;
    const SPECULATION_UNCERTAINTY_BONUS: f32 = 14.0;
    const LEARNING_RATE: f32 = 0.20;
    const LEARNING_DECAY: f32 = 0.98;
    const LEARNING_WEIGHT: f32 = 14.0;
    const TRANSPORT_COST_PER_DISTANCE: f32 = 0.00012;
    const COST_PER_MILE_FACTOR: f32 = 1.0;
    const COST_PER_MILE_FACTOR_STEP: f32 = 0.05;
    const CAPITAL_CARRY_COST_PER_TIME: f32 = 0.0020;
    const ISLAND_NEGLECT_BONUS_PER_TICK: f32 = 0.008;
    const ISLAND_NEGLECT_BONUS_CAP: f32 = 22.0;

    let mut planning_tuning = PlanningTuning {
        confidence_decay_k: CONFIDENCE_DECAY_K,
        speculation_floor: SPECULATION_FLOOR,
        speculation_staleness_scale: SPECULATION_STALENESS_SCALE,
        speculation_uncertainty_bonus: SPECULATION_UNCERTAINTY_BONUS,
        learning_rate: LEARNING_RATE,
        learning_decay: LEARNING_DECAY,
        learning_weight: LEARNING_WEIGHT,
        transport_cost_per_distance: TRANSPORT_COST_PER_DISTANCE,
        cost_per_mile_factor: COST_PER_MILE_FACTOR,
        capital_carry_cost_per_time: CAPITAL_CARRY_COST_PER_TIME,
        island_neglect_bonus_per_tick: ISLAND_NEGLECT_BONUS_PER_TICK,
        island_neglect_bonus_cap: ISLAND_NEGLECT_BONUS_CAP,
    };

    let mut world = World::new(NUM_ISLANDS, NUM_SHIPS);
    world.set_planning_tuning(planning_tuning);

    loop {
        let shift_down = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let mut tuning_changed = false;
        if is_key_pressed(KeyCode::LeftBracket) {
            if shift_down {
                world.select_previous_island();
            } else {
                world.select_previous_ship();
            }
        }
        if is_key_pressed(KeyCode::RightBracket) {
            if shift_down {
                world.select_next_island();
            } else {
                world.select_next_ship();
            }
        }
        if is_key_pressed(KeyCode::Minus) {
            planning_tuning.cost_per_mile_factor =
                (planning_tuning.cost_per_mile_factor - COST_PER_MILE_FACTOR_STEP)
                    .clamp(0.2, 5.0);
            tuning_changed = true;
        }
        if is_key_pressed(KeyCode::Equal) {
            planning_tuning.cost_per_mile_factor =
                (planning_tuning.cost_per_mile_factor + COST_PER_MILE_FACTOR_STEP)
                    .clamp(0.2, 5.0);
            tuning_changed = true;
        }
        if tuning_changed {
            world.set_planning_tuning(planning_tuning);
        }

        // Camera maps simulation space (WORLD_SIZE x WORLD_SIZE) to the screen,
        // with inverted world Y so world-space icons render upright.
        let camera = Camera2D::from_display_rect(Rect::new(0.0, WORLD_SIZE, WORLD_SIZE, -WORLD_SIZE));
        set_camera(&camera);

        world.update(get_frame_time());

        clear_background(Color::from_rgba(10, 30, 60, 255));
        world.draw();

        set_default_camera();
        world.draw_ui();
        next_frame().await;
    }
}
