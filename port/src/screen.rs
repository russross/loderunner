use crate::collision::Layer;
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
        self.scene.status_border();
        self.put_text(Layer::Foreground, 0, 16, "SCORE        MEN    LEVEL   ");
        self.put_text(
            Layer::Foreground,
            16,
            16,
            &format!("{:03}", self.session.lives),
        );
        self.put_text(
            Layer::Foreground,
            25,
            16,
            &format!("{:03}", self.session.level),
        );
        self.add_score(0);
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
