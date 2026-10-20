use crate::collision::Layer;
use crate::event_timing::TimingEvent;
use crate::graphics::Sprite;
use crate::score::Score;
use crate::state::Game;

impl Game {
    pub fn put_text(&mut self, layer: Layer, col: u8, row: u8, text: &str) {
        for (offset, byte) in text.bytes().enumerate() {
            self.paint_tile(layer, Sprite::glyph(byte), col + offset as u8, row);
        }
    }

    pub fn put_status(&mut self) {
        self.paint_status(self.session.level, self.session.lives, self.score);
    }

    // Board initialization paints the default counters before reload paints the
    // retained progress. Both passes are part of the modeled drawing work.
    pub fn paint_status(&mut self, level: u8, lives: u8, score: Score) {
        self.scene.status_border();
        self.put_text(Layer::Foreground, 0, 16, "SCORE        MEN    LEVEL   ");
        self.put_text(Layer::Foreground, 16, 16, &format!("{lives:03}"));
        self.put_text(Layer::Foreground, 25, 16, &format!("{level:03}"));
        self.paint_score(score);
    }

    pub fn add_score(&mut self, points: u32) {
        self.score.add(points);
        self.paint_score(self.score);
    }

    fn paint_score(&mut self, score: Score) {
        self.clock.events.add(TimingEvent::Score, 1);
        for (index, digit) in score.visible_digits().into_iter().enumerate() {
            self.paint_tile(
                Layer::Foreground,
                Sprite::glyph(b'0' + digit),
                5 + index as u8,
                16,
            );
        }
    }
}
