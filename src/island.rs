use macroquad::prelude::*;

pub struct Island {
    #[allow(dead_code)]
    pub id: usize,
    pub pos: Vec2,
}

impl Island {
    pub fn new(id: usize, pos: Vec2) -> Self {
        Self { id, pos }
    }

    pub fn draw(&self) {
        draw_circle(self.pos.x, self.pos.y, 20.0, DARKGREEN);
        draw_circle_lines(self.pos.x, self.pos.y, 20.0, 3.0, GREEN);
    }
}
