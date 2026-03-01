use macroquad::prelude::*;

mod island;
mod ship;
mod world;

use world::{World, WORLD_SIZE};

#[macroquad::main("Archipelago")]
async fn main() {
    let mut world = World::new(30, 50);

    loop {
        // Camera maps simulation space (WORLD_SIZE x WORLD_SIZE) to the screen.
        let camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, WORLD_SIZE, WORLD_SIZE));
        set_camera(&camera);

        world.update(get_frame_time());

        clear_background(Color::from_rgba(10, 30, 60, 255));
        world.draw();

        set_default_camera();
        world.draw_ui();
        next_frame().await;
    }
}
