use crate::actors::{DigSide, Guard, GuardId, GuardMode, GuardSchedule, Player};
use crate::board::{Board, GoldProgress, Holes, Level};
use crate::collision::{Layer, Scene};
use crate::devices::keyboard::Keyboard;
use crate::devices::speaker::Speaker;
use crate::graphics::Sprite;
use crate::render::Display;
use crate::score::Score;
use crate::sound::Sound;
use crate::timing::Clock;
use lode_trace::{
    Action, Actor, ActorId, Cell, Facing, Hole, Life, Phase, Session, Snapshot, Tile,
};

pub struct Game {
    pub board: Board,
    pub player: Player,
    pub guards: Vec<Guard>,
    pub guard_schedule: GuardSchedule,
    pub holes: Holes,
    pub hidden_ladders: Vec<Cell>,
    pub session: Progress,
    pub score: Score,
    pub gold: GoldProgress,
    pub falling_sound: bool,
    pub falling_pitch: u8,
    pub resurrection_col: u8,
    pub difficulty: u8,
    pub skip_level: bool,
    pub display: Display,
    pub scene: Scene,
    pub keyboard: Keyboard,
    pub speaker: Speaker,
    pub sound: Sound,
    pub clock: Clock,
    guard_facings: [Facing; 5],
}

// Session progress survives board loading. Score has one authoritative decimal
// value in Game and is included in the external session only when observed.
pub struct Progress {
    pub level: u8,
    pub attempt: u32,
    pub phase: Phase,
    pub lives: u8,
    pub exit_available: bool,
}

impl Progress {
    pub fn new(level: u8) -> Self {
        Self {
            level,
            attempt: 0,
            phase: Phase::Playing,
            lives: 5,
            exit_available: false,
        }
    }
}

impl Game {
    pub fn load_level(board: &Level, level: u8, paced: bool) -> Self {
        let mut game = Self {
            board: Board::new(board.tiles),
            player: Player::new(board.player, Facing::Right),
            guards: Vec::new(),
            guard_schedule: GuardSchedule::default(),
            holes: Holes::default(),
            hidden_ladders: Vec::new(),
            session: Progress::new(level),
            score: Score::default(),
            gold: board.gold,
            falling_sound: false,
            falling_pitch: 0x20,
            resurrection_col: 0,
            difficulty: 0,
            skip_level: false,
            display: Display::default(),
            scene: Scene::default(),
            keyboard: Keyboard::default(),
            speaker: Speaker::default(),
            sound: Sound::default(),
            clock: Clock::new(paced),
            guard_facings: [Facing::Right; 5],
        };
        game.populate_board();
        game
    }

    // Loading replaces board-local state in place. The clock, speaker, score,
    // preferences and falling whistle keep their identities and pending work.
    pub fn reload_board(&mut self, board: &Level) {
        self.clock.flush_draws();
        for guard in &self.guards {
            self.guard_facings[guard.id.index()] = guard.facing;
        }
        self.board = Board::new(board.tiles);
        self.player = Player::new(board.player, self.player.facing);
        self.guards.clear();
        self.guard_schedule.restart_roster();
        self.holes = Holes::default();
        self.hidden_ladders.clear();
        self.gold = board.gold;
        self.skip_level = false;
        self.keyboard = Keyboard::default();
        self.sound.clear();
        self.scene = Scene::default();
        self.display = Display::default();
        self.session.exit_available = false;
        self.populate_board();
    }

    fn populate_board(&mut self) {
        // Reverse board traversal determines actor identities and which markers
        // survive an overfull board. Rendering follows that same cell order.
        for index in (0..448).rev() {
            let cell = Cell {
                col: (index % 28) as u8,
                row: (index / 28) as u8,
            };
            match self.board.cells[index] {
                Tile::HiddenLadder => {
                    if self.hidden_ladders.len() < 45 {
                        self.hidden_ladders.push(cell);
                    }
                    self.board.cells[index] = Tile::Empty;
                    self.board.terrain[index] = Tile::Empty;
                }
                Tile::Guard => {
                    if self.guards.len() == 5 {
                        self.board.cells[index] = Tile::Empty;
                    } else {
                        let id = GuardId::for_index(self.guards.len());
                        let mut guard = Guard::new(id, cell);
                        guard.facing = self.guard_facings[id.index()];
                        self.guards.push(guard);
                    }
                    self.board.terrain[index] = Tile::Empty;
                }
                Tile::Player => {
                    if cell != self.player.position.cell {
                        self.board.cells[index] = Tile::Empty;
                    }
                    self.board.terrain[index] = Tile::Empty;
                }
                _ => (),
            }
            for (layer, tile) in [
                (Layer::Foreground, self.board.cells[index]),
                (Layer::Background, self.board.terrain[index]),
            ] {
                self.paint_cell(layer, Sprite::tile(tile), cell);
            }
        }
        let (sprite, x, y) = self.player_sprite_coordinates();
        self.overlay_shape(sprite, x, y);
        self.paint_status(self.session.level, 5, Score::default());
    }

    pub fn player_sprite_coordinates(&self) -> (Sprite, u8, u8) {
        let (half_x, y) = self.player.position.half_x_and_pixel_y();
        (self.player.animation.sprite(), half_x, y)
    }

    // Observation translates concrete game state into the shared trace contract.
    // It neither drives movement nor reconstructs transient gameplay occurrences.
    pub fn snapshot(&self) -> Snapshot {
        let mut actors = vec![Actor {
            id: ActorId::Player,
            cell: self.player.position.cell,
            x_adjust: self.player.position.x(),
            y_adjust: self.player.position.y(),
            facing: self.player.facing,
            motion: self.player.motion,
            action: match self.player.digging.map(|dig| dig.side) {
                Some(DigSide::Left) => Action::DigLeft,
                Some(DigSide::Right) => Action::DigRight,
                None => Action::None,
            },
            life: if self.player.alive {
                Life::Alive
            } else {
                Life::Dead
            },
            carrying_gold: false,
        }];
        actors.extend(self.guards.iter().map(|guard| Actor {
            id: guard.id.actor(),
            cell: guard.position.cell,
            x_adjust: guard.position.x(),
            y_adjust: guard.position.y(),
            facing: guard.facing,
            motion: guard.motion,
            action: match guard.mode {
                GuardMode::Trapped { release_in } if release_in.get() <= 6 => Action::ClimbingOut,
                GuardMode::Trapped { release_in } if release_in.get() < 127 => Action::Trapped,
                _ => Action::None,
            },
            life: if matches!(guard.mode, GuardMode::Resurrecting { .. }) {
                Life::Resurrecting
            } else {
                Life::Alive
            },
            carrying_gold: guard.mode.carrying(),
        }));
        let mut holes: Vec<_> = self
            .holes
            .entries
            .iter()
            .flatten()
            .map(|hole| Hole {
                cell: hole.cell,
                generation: hole.generation,
                phase: hole.phase(),
            })
            .collect();
        holes.sort_by_key(|hole| hole.cell);
        let mut hidden_ladders = self.hidden_ladders.clone();
        hidden_ladders.sort();
        Snapshot {
            session: Session {
                level: self.session.level,
                attempt: self.session.attempt,
                phase: self.session.phase,
                score: self.score.decimal(),
                lives: self.session.lives,
                exit_available: self.session.exit_available,
            },
            actors,
            active: self.board.cells.to_vec(),
            background: self.board.terrain.to_vec(),
            hidden_ladders,
            holes,
        }
    }
}
