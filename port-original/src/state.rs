use crate::devices::hires::Hires;
use crate::devices::keyboard::Keyboard;
use crate::devices::speaker::Speaker;
use crate::graphics::{draw_sprite, draw_sprite_at_pixels};
use crate::sound::Sound;
use crate::storage::decode_level;
use crate::timing::Clock;
use lode_trace::{
    Action, Actor, ActorId, Cell, Facing, Hole, Life, Motion, Phase, Session, Snapshot, Tile,
};

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub col: u8,
    pub row: u8,
    pub x_adjust: u8,
    pub y_adjust: u8,
    pub animation: u8,
    pub facing: Facing,
    pub motion: Motion,
}

// Parallel arrays retain original slot numbering and the load/store guard workflow.
// Entries zero and six/seven are storage, not additional playable guards.
#[derive(Default)]
pub struct Guards {
    pub count: u8,
    pub cols: [u8; 8],
    pub rows: [u8; 8],
    pub gold_timers: [u8; 8],
    pub x_adjusts: [u8; 8],
    pub y_adjusts: [u8; 8],
    pub animations: [u8; 8],
    pub facings: [u8; 8],
    pub resurrection_timers: [u8; 8],
    pub current: u8,
    pub phase: u8,
}

#[derive(Default)]
pub struct Holes {
    pub cols: [u8; 31],
    pub rows: [u8; 31],
    pub timers: [u8; 31],
    pub generations: [u32; 31],
}

pub struct Game {
    pub active: [Tile; 448],
    pub background: [Tile; 448],
    pub player: Player,
    pub guards: Guards,
    pub holes: Holes,
    pub hidden_ladders: Vec<Cell>,
    pub session: Session,
    pub gold_count: u8,
    pub alive: bool,
    pub command: u8,
    pub command_lr: u8,
    pub didnt_pick_up_gold: bool,
    pub dig_direction: i8,
    pub dig_animation: u8,
    pub hires: Hires,
    pub keyboard: Keyboard,
    pub speaker: Speaker,
    pub sound: Sound,
    pub clock: Clock,
}

impl Game {
    pub fn load_level(record: &[u8; 256], level: u8, paced: bool) -> Result<Self, String> {
        let tiles = decode_level(record);
        let mut game = Self {
            active: tiles,
            background: tiles,
            player: Player {
                col: 0xff,
                row: 0,
                x_adjust: 2,
                y_adjust: 2,
                animation: 8,
                facing: Facing::Right,
                motion: Motion::Standing,
            },
            guards: Guards::default(),
            holes: Holes::default(),
            hidden_ladders: Vec::new(),
            session: Session {
                level,
                attempt: 0,
                phase: Phase::Playing,
                score: 0,
                lives: 5,
                exit_available: false,
            },
            gold_count: 0,
            alive: true,
            command: 0,
            command_lr: 0,
            didnt_pick_up_gold: true,
            dig_direction: 0,
            dig_animation: 0,
            hires: Hires::default(),
            keyboard: Keyboard::default(),
            speaker: Speaker::default(),
            sound: Sound::default(),
            clock: Clock::new(paced),
        };

        // Reverse traversal determines which player and guards survive overfull
        // boards. Actor markers are removed only from the background map.
        for row in (0..16).rev() {
            for col in (0..28).rev() {
                let index = row * 28 + col;
                match game.active[index] {
                    Tile::HiddenLadder => {
                        if game.hidden_ladders.len() < 45 {
                            game.hidden_ladders.push(Cell {
                                col: col as u8,
                                row: row as u8,
                            });
                        }
                        game.active[index] = Tile::Empty;
                        game.background[index] = Tile::Empty;
                    }
                    Tile::Gold => game.gold_count = game.gold_count.wrapping_add(1),
                    Tile::Guard => {
                        if game.guards.count == 5 {
                            game.active[index] = Tile::Empty;
                        } else {
                            game.guards.count += 1;
                            let slot = usize::from(game.guards.count);
                            game.guards.cols[slot] = col as u8;
                            game.guards.rows[slot] = row as u8;
                            game.guards.x_adjusts[slot] = 2;
                            game.guards.y_adjusts[slot] = 2;
                        }
                        game.background[index] = Tile::Empty;
                    }
                    Tile::Player => {
                        if game.player.col != 0xff {
                            game.active[index] = Tile::Empty;
                        } else {
                            game.player.col = col as u8;
                            game.player.row = row as u8;
                        }
                        game.background[index] = Tile::Empty;
                    }
                    _ => (),
                }
                let tile = game.active[index];
                let sprite = if tile == Tile::Trap {
                    Tile::Brick as u8
                } else {
                    tile as u8
                };
                draw_sprite(&mut game.hires, 0, sprite, col as u8, row as u8);
                let tile = game.background[index];
                let sprite = if tile == Tile::Trap {
                    Tile::Brick as u8
                } else {
                    tile as u8
                };
                draw_sprite(&mut game.hires, 1, sprite, col as u8, row as u8);
            }
        }
        if game.player.col == 0xff {
            return Err("level contains no player".into());
        }
        game.hidden_ladders.sort();
        let (sprite, x, y) = game.player_sprite_coordinates();
        // The tile player was already drawn at its initial position. Drawing the
        // same bits here changes nothing; overlap is deliberately ignored on load.
        draw_sprite_at_pixels(&mut game.hires, sprite, x, y);
        Ok(game)
    }

    pub fn player_sprite_coordinates(&self) -> (u8, u8, u8) {
        const ANIMATION: [u8; 18] = [
            11, 12, 13, 24, 25, 26, 15, 19, 9, 16, 17, 21, 22, 23, 37, 20, 14, 18,
        ];
        const ROW_ADJUST: [u8; 5] = [0xfb, 0xfd, 0, 2, 4];
        let x = (self.player.col * 5)
            .wrapping_add(self.player.x_adjust)
            .wrapping_sub(2);
        let y = (self.player.row * 11).wrapping_add(ROW_ADJUST[usize::from(self.player.y_adjust)]);
        (ANIMATION[usize::from(self.player.animation)], x, y)
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut actors = vec![Actor {
            id: ActorId::Player,
            cell: Cell {
                col: self.player.col,
                row: self.player.row,
            },
            x_adjust: self.player.x_adjust,
            y_adjust: self.player.y_adjust,
            facing: self.player.facing,
            motion: self.player.motion,
            action: match self.dig_direction {
                -1 => Action::DigLeft,
                1 => Action::DigRight,
                _ => Action::None,
            },
            life: if self.alive { Life::Alive } else { Life::Dead },
            carrying_gold: false,
        }];
        for slot in 1..=usize::from(self.guards.count) {
            actors.push(Actor {
                id: ActorId::Guard(slot as u8),
                cell: Cell {
                    col: self.guards.cols[slot],
                    row: self.guards.rows[slot],
                },
                x_adjust: self.guards.x_adjusts[slot],
                y_adjust: self.guards.y_adjusts[slot],
                facing: if self.guards.facings[slot] & 0x80 != 0 {
                    Facing::Left
                } else {
                    Facing::Right
                },
                motion: Motion::Standing,
                action: if (1..0x7f).contains(&self.guards.gold_timers[slot]) {
                    Action::Trapped
                } else {
                    Action::None
                },
                life: if self.guards.resurrection_timers[slot] != 0 {
                    Life::Resurrecting
                } else {
                    Life::Alive
                },
                carrying_gold: self.guards.gold_timers[slot] & 0x80 != 0,
            });
        }
        let mut holes = Vec::new();
        for slot in 0..31 {
            let timer = self.holes.timers[slot];
            if timer != 0 {
                holes.push(Hole {
                    cell: Cell {
                        col: self.holes.cols[slot],
                        row: self.holes.rows[slot],
                    },
                    generation: self.holes.generations[slot],
                    phase: if timer <= 10 {
                        lode_trace::HolePhase::ClosingSecond
                    } else if timer <= 20 {
                        lode_trace::HolePhase::ClosingFirst
                    } else {
                        lode_trace::HolePhase::Open
                    },
                });
            }
        }
        holes.sort_by_key(|hole| hole.cell);
        Snapshot {
            session: self.session.clone(),
            actors,
            active: self.active.to_vec(),
            background: self.background.to_vec(),
            hidden_ladders: self.hidden_ladders.clone(),
            holes,
        }
    }
}
