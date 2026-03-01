use macroquad::prelude::*;

pub struct Ship {
    pub pos: Vec2,
    target: Vec2,
    speed: f32,
}

impl Ship {
    pub fn new(pos: Vec2, speed: f32) -> Self {
        Self {
            pos,
            target: pos,
            speed,
        }
    }

    pub fn set_target(&mut self, target: Vec2) {
        self.target = target;
    }

    /// Move toward target. Returns true when arrived.
    pub fn update(&mut self, dt: f32) -> bool {
        let to_target = self.target - self.pos;
        let dist = to_target.length();
        if dist < 1.0 {
            return true;
        }
        let step = self.speed * dt;
        if step >= dist {
            self.pos = self.target;
            true
        } else {
            self.pos += to_target.normalize() * step;
            false
        }
    }

    pub fn draw(&self) {
        draw_circle(self.pos.x, self.pos.y, 8.0, WHITE);
        draw_circle_lines(self.pos.x, self.pos.y, 8.0, 2.0, LIGHTGRAY);
    }
}
