//! Guard route selection, including original candidate order and exit quirks.
use crate::actors::{Direction, Guard, GuardMode};
use crate::board::Board;
use lode_trace::{Cell, Tile};

// Search extents describe algorithmic work, independent of branch evaluation
// order. The pacing model consumes them after the route has been selected.
#[derive(Default)]
pub struct SearchWork {
    pub same_row_cells: u64,
    pub horizontal_span: u64,
    pub candidate_columns: u64,
    pub upward_rows: u64,
    pub downward_rows: u64,
}

pub struct Route {
    pub direction: Option<Direction>,
    pub work: SearchWork,
}

// Same-row destinations outrank destinations above, which outrank those below.
// Within a class, smaller distance wins; equal ranks retain the earlier route.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Destination {
    SameRow,
    Above,
    Below,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Rank(Destination, u8);

struct Search<'a> {
    board: &'a Board,
    player: Cell,
    guard: Cell,
    best: Option<(Rank, Direction)>,
    work: SearchWork,
}

pub fn choose(board: &Board, player: Cell, guard: Guard) -> Route {
    let mut search = Search {
        board,
        player,
        guard: guard.position.cell,
        best: None,
        work: SearchWork::default(),
    };
    let direction = if board.terrain_at(search.guard) == Tile::Brick
        && matches!(guard.mode, GuardMode::Trapped { .. })
    {
        Some(Direction::Up)
    } else {
        search.choose()
    };
    Route {
        direction,
        work: search.work,
    }
}

impl Search<'_> {
    fn choose(&mut self) -> Option<Direction> {
        // Direct pursuit checks support, not intervening walls. Actual movement
        // checks obstacles separately, preserving the original pursuit behavior.
        if self.guard.row == self.player.row && self.guard.col != self.player.col {
            let step = if self.guard.col < self.player.col {
                1_i16
            } else {
                -1
            };
            let mut col = i16::from(self.guard.col);
            let mut supported = true;
            while col != i16::from(self.player.col) {
                col += step;
                self.work.same_row_cells += 1;
                let index = usize::from(self.guard.row) * 28 + col as usize;
                if !matches!(self.board.terrain[index], Tile::Ladder | Tile::Rope)
                    && self.guard.row != 15
                    && matches!(self.board.terrain[index + 28], Tile::Empty | Tile::Trap)
                {
                    supported = false;
                    break;
                }
            }
            if supported {
                return Some(if step > 0 {
                    Direction::Right
                } else {
                    Direction::Left
                });
            }
        }

        // Search the current column down then up, the left span from its far
        // edge, and finally the right span from its far edge. Ties are stable.
        let (left, right) = self.horizontal_limits();
        self.work.horizontal_span = u64::from(right - left);
        self.consider(self.guard.col, Direction::Down, Direction::Up);
        for col in left..self.guard.col {
            self.consider(col, Direction::Left, Direction::Left);
        }
        for col in (self.guard.col + 1..=right).rev() {
            self.consider(col, Direction::Right, Direction::Right);
        }
        self.best.map(|(_, direction)| direction)
    }

    fn consider(&mut self, col: u8, down: Direction, up: Direction) {
        self.work.candidate_columns += 1;
        let index = usize::from(self.guard.row) * 28 + usize::from(col);
        if self.guard.row < 15
            && !matches!(self.board.terrain[index + 28], Tile::Brick | Tile::Stone)
        {
            let (row, scanned) = self.below(col);
            self.work.downward_rows += scanned;
            self.offer(col, row, down);
        }
        if self.guard.row > 0 && self.board.terrain[index] == Tile::Ladder {
            let (row, scanned) = self.above(col);
            self.work.upward_rows += scanned;
            self.offer(col, row, up);
        }
    }

    fn offer(&mut self, col: u8, row: u8, direction: Direction) {
        let rank = match row.cmp(&self.player.row) {
            std::cmp::Ordering::Equal => Rank(Destination::SameRow, col.abs_diff(self.guard.col)),
            std::cmp::Ordering::Less => Rank(Destination::Above, self.player.row - row),
            std::cmp::Ordering::Greater => Rank(Destination::Below, row - self.player.row),
        };
        if self.best.is_none_or(|(best, _)| rank < best) {
            self.best = Some((rank, direction));
        }
    }

    fn horizontal_limits(&self) -> (u8, u8) {
        let traversable = |col: u8| {
            let index = usize::from(self.guard.row) * 28 + usize::from(col);
            let enter = !matches!(self.board.cells[index], Tile::Brick | Tile::Stone);
            let supported = matches!(self.board.cells[index], Tile::Ladder | Tile::Rope)
                || self.guard.row == 15
                || matches!(
                    self.board.terrain[index + 28],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                );
            (enter, supported)
        };
        // Unsupported cells can be entered but end the horizontal reach. Both
        // directions include that final cell in the candidate span.
        let mut left = self.guard.col;
        while left > 0 {
            let (enter, supported) = traversable(left - 1);
            if !enter {
                break;
            }
            left -= 1;
            if !supported {
                break;
            }
        }
        let mut right = self.guard.col;
        while right < 27 {
            let (enter, supported) = traversable(right + 1);
            if !enter {
                break;
            }
            right += 1;
            if !supported {
                break;
            }
        }
        (left, right)
    }

    fn above(&self, col: u8) -> (u8, u64) {
        let mut row = self.guard.row;
        let mut scanned = 0;
        loop {
            scanned += 1;
            let old = usize::from(row) * 28 + usize::from(col);
            if self.board.terrain[old] != Tile::Ladder {
                return (row, scanned);
            }
            row -= 1;
            let exit = |neighbor: u8| {
                matches!(
                    self.board.terrain[usize::from(row + 1) * 28 + usize::from(neighbor)],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                ) || self.board.terrain[usize::from(row) * 28 + usize::from(neighbor)] == Tile::Rope
            };
            if row == 0
                || (row <= self.player.row
                    && ((col > 0 && exit(col - 1)) || (col < 27 && exit(col + 1))))
            {
                return (row, scanned);
            }
        }
    }

    fn below(&self, col: u8) -> (u8, u64) {
        // Downward exits retain the original asymmetry: a non-rope left neighbor
        // moves the right rope check down a row. Its support check stays below
        // the current row. This is a route rule, not an evaluation-order effect.
        for row in self.guard.row..=15 {
            let scanned = u64::from(row - self.guard.row) + 1;
            let index = usize::from(row) * 28 + usize::from(col);
            if row == 15 || matches!(self.board.terrain[index + 28], Tile::Brick | Tile::Stone) {
                return (row, scanned);
            }
            if self.board.terrain[index] == Tile::Empty || row < self.player.row {
                continue;
            }
            let left_rope = col > 0 && self.board.terrain[index - 1] == Tile::Rope;
            let support = |index| {
                matches!(
                    self.board.terrain[index],
                    Tile::Brick | Tile::Stone | Tile::Ladder
                )
            };
            if col > 0 && (left_rope || support(index + 27)) {
                return (row, scanned);
            }
            let right_row = if col > 0 && !left_rope { row + 1 } else { row };
            let right = usize::from(right_row) * 28 + usize::from(col) + 1;
            if col < 27 && (self.board.terrain[right] == Tile::Rope || support(index + 29)) {
                return (row, scanned);
            }
        }
        unreachable!("a downward search reaches the bottom row")
    }
}

#[cfg(test)]
mod tests {
    use super::{Search, SearchWork, choose};
    use crate::actors::{Direction, Guard, GuardId};
    use crate::board::Board;
    use lode_trace::{Cell, Tile};

    #[test]
    fn downward_right_exit_uses_lower_row_when_left_is_not_rope() {
        let mut board = Board::new([Tile::Empty; 448]);
        board.terrain[5 * 28 + 5] = Tile::Ladder;
        board.terrain[5 * 28 + 6] = Tile::Rope;
        fn search(board: &Board) -> Search<'_> {
            Search {
                board,
                player: Cell { col: 20, row: 5 },
                guard: Cell { col: 5, row: 5 },
                best: None,
                work: SearchWork::default(),
            }
        }
        assert_eq!(search(&board).below(5).0, 15);
        board.terrain[6 * 28 + 6] = Tile::Rope;
        assert_eq!(search(&board).below(5).0, 5);

        // At the left edge there is no left-neighbor probe to shift the row.
        board.terrain[5 * 28] = Tile::Ladder;
        board.terrain[5 * 28 + 1] = Tile::Rope;
        assert_eq!(search(&board).below(0).0, 5);
    }

    #[test]
    fn equally_ranked_ladders_choose_left_before_right() {
        let mut board = Board::new([Tile::Empty; 448]);
        for col in 0..28 {
            board.terrain[11 * 28 + col] = Tile::Brick;
        }
        for row in 5..=10 {
            board.terrain[row * 28 + 4] = Tile::Ladder;
            board.terrain[row * 28 + 6] = Tile::Ladder;
        }
        board.terrain[6 * 28 + 3] = Tile::Brick;
        board.terrain[6 * 28 + 7] = Tile::Brick;
        board.cells = board.terrain;
        let guard = Guard::new(GuardId::for_index(0), Cell { col: 5, row: 10 });
        let route = choose(&board, Cell { col: 5, row: 5 }, guard);
        assert_eq!(route.direction, Some(Direction::Left));
    }

    #[test]
    fn supported_same_row_pursuit_leaves_obstacles_to_movement() {
        let mut board = Board::new([Tile::Empty; 448]);
        board.terrain[15 * 28 + 6] = Tile::Stone;
        board.cells = board.terrain;
        let guard = Guard::new(GuardId::for_index(0), Cell { col: 5, row: 15 });
        assert_eq!(
            choose(&board, Cell { col: 10, row: 15 }, guard).direction,
            Some(Direction::Right)
        );
    }
}
