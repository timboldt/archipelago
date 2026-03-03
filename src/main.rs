use macroquad::prelude::*;

mod island;
mod ship;
mod world;

use ship::PlanningTuning;
use world::{World, WORLD_SIZE};

const TIME_SCALE_MIN: f32 = 0.25;
const TIME_SCALE_MAX: f32 = 6.0;
const TIME_SCALE_STEP: f32 = 0.25;
const TIME_SCALE_DEFAULT: f32 = 1.0;

fn handle_time_scale_input(time_scale: &mut f32) {
    if is_key_pressed(KeyCode::Minus) {
        *time_scale = (*time_scale - TIME_SCALE_STEP).clamp(TIME_SCALE_MIN, TIME_SCALE_MAX);
    }
    if is_key_pressed(KeyCode::Equal) {
        *time_scale = (*time_scale + TIME_SCALE_STEP).clamp(TIME_SCALE_MIN, TIME_SCALE_MAX);
    }
}

#[macroquad::main("Archipelago")]
async fn main() {
    // Number of islands and ships in the simulation.
    const NUM_ISLANDS: usize = 50;
    const NUM_SHIPS: usize = 500;

    // Overall tuning parameters for ship planning.
    const GLOBAL_FRICTION_MULT: f32 = 1.0;
    const INFO_DECAY_RATE: f32 = 0.003;
    const MARKET_SPREAD: f32 = 0.10;

    let planning_tuning = PlanningTuning {
        global_friction_mult: GLOBAL_FRICTION_MULT,
        info_decay_rate: INFO_DECAY_RATE,
        market_spread: MARKET_SPREAD,
    };

    let mut world = World::new(NUM_ISLANDS, NUM_SHIPS);
    world.set_planning_tuning(planning_tuning);
    let mut time_scale = TIME_SCALE_DEFAULT;

    loop {
        world.handle_input();
        handle_time_scale_input(&mut time_scale);

        // Camera maps simulation space (WORLD_SIZE x WORLD_SIZE) to the screen,
        // with inverted world Y so world-space icons render upright.
        let camera =
            Camera2D::from_display_rect(Rect::new(0.0, WORLD_SIZE, WORLD_SIZE, -WORLD_SIZE));
        set_camera(&camera);

        world.update(get_frame_time() * time_scale);

        clear_background(Color::from_rgba(10, 30, 60, 255));
        world.draw();

        set_default_camera();
        world.draw_ui();
        next_frame().await;
    }
}
