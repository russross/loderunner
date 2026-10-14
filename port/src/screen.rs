use crate::graphics::{draw_sprite, row_address};
use crate::state::Game;
use lode_trace::Tile;

impl Game {
    pub fn put_text(&mut self, page: usize, col: u8, row: u8, text: &str) {
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
            draw_sprite(&mut self.hires, page, sprite, col + offset as u8, row);
        }
    }

    pub fn put_status(&mut self) {
        for row in 176..180 {
            let address = row_address(0, row);
            for col in 0..40 {
                self.hires
                    .write(address + col, if col % 2 == 0 { 0xd5 } else { 0xaa });
            }
        }
        self.put_text(0, 0, 16, "SCORE        MEN    LEVEL   ");
        self.put_text(0, 16, 16, &format!("{:03}", self.session.lives));
        self.put_text(0, 25, 16, &format!("{:03}", self.session.level));
        self.add_score(0);
    }

    // High scores reuse the original background page. Closing the table rebuilds
    // that page from logical terrain while leaving the foreground actors intact.
    pub fn show_high_scores(&mut self) {
        self.hires.clear(1);
        self.put_text(1, 4, 0, "LODE RUNNER HIGH SCORES");
        self.put_text(1, 4, 3, "INITIALS LEVEL  SCORE");
        self.put_text(1, 4, 4, "-------- ----- --------");
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
            self.put_text(1, 0, 5 + index as u8, &text);
        }
        self.hires.select(1);
    }

    pub fn restore_background(&mut self) {
        self.hires.clear(1);
        for row in (0..16).rev() {
            for col in (0..28).rev() {
                let tile = self.board.terrain[usize::from(row) * 28 + usize::from(col)];
                draw_sprite(
                    &mut self.hires,
                    1,
                    if tile == Tile::Trap { 1 } else { tile as u8 },
                    col,
                    row,
                );
            }
        }
        self.hires.select(0);
    }
}
