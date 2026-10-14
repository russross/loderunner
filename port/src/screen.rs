use crate::collision::Layer;
use crate::event_timing::TimingEvent;
use crate::score::Score;
use crate::state::Game;
use lode_trace::Tile;

impl Game {
    pub fn put_text(&mut self, layer: Layer, col: u8, row: u8, text: &str) {
        for (offset, byte) in text.bytes().enumerate() {
            let sprite = match byte {
                b'A'..=b'Z' => byte - b'A' + 69,
                b'0'..=b'9' => byte - b'0' + 59,
                b' ' => 0,
                b'>' => 95,
                b'.' => 96,
                b'(' => 97,
                b')' => 98,
                b'/' => 99,
                b'-' => 100,
                b'<' => 101,
                _ => 0,
            };
            self.paint_tile(layer, sprite, col + offset as u8, row);
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
            self.paint_tile(Layer::Foreground, 0x3b + digit, 5 + index as u8, 16);
        }
    }

    // High scores occupy the background layer. Closing the table rebuilds that
    // layer from logical terrain while leaving the foreground actors intact.
    pub fn show_high_scores(&mut self) {
        self.scene.clear_background();
        self.put_text(Layer::Background, 4, 0, "LODE RUNNER HIGH SCORES");
        self.put_text(Layer::Background, 4, 3, "INITIALS LEVEL  SCORE");
        self.put_text(Layer::Background, 4, 4, "-------- ----- --------");
        for index in 0..self.high_scores.len() {
            let entry = &self.high_scores[index];
            let initials = String::from_utf8_lossy(&entry.initials);
            let text = format!(
                "{:>2}.    {}    {:03}  {:08}",
                index + 1,
                initials,
                entry.level,
                entry.score
            );
            self.put_text(Layer::Background, 0, 5 + index as u8, &text);
        }
        self.display.select(Layer::Background);
    }

    pub fn restore_background(&mut self) {
        self.scene.clear_background();
        for row in (0..16).rev() {
            for col in (0..28).rev() {
                let tile = self.board.terrain[usize::from(row) * 28 + usize::from(col)];
                self.paint_tile(
                    Layer::Background,
                    if tile == Tile::Trap { 1 } else { tile as u8 },
                    col,
                    row,
                );
            }
        }
        self.display.select(Layer::Foreground);
    }
}
