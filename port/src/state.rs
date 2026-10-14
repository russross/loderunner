use crate::actors::{Command, DigSide, Guard, GuardId, GuardMode, GuardSchedule, Player, Position};
use crate::board::{Board, Holes};
use crate::collision::Scene;
use crate::devices::hires::Hires;
use crate::devices::keyboard::Keyboard;
use crate::devices::speaker::Speaker;
use crate::score::Score;
use crate::sound::Sound;
use crate::storage::decode_level;
use crate::timing::Clock;
use lode_trace::{
    Action, Actor, ActorId, Cell, Facing, Hole, Life, Motion, Phase, Session, Snapshot, Tile,
};

pub struct Game {
    pub board: Board,
    pub player: Player,
    pub guards: Vec<Guard>,
    pub guard_schedule: GuardSchedule,
    pub holes: Holes,
    pub hidden_ladders: Vec<Cell>,
    pub session: Session,
    pub score: Score,
    pub gold_count: u8,
    pub falling_sound: bool,
    pub falling_pitch: u8,
    pub resurrection_col: u8,
    pub difficulty: u8,
    pub skip_level: bool,
    pub high_scores: Vec<lode_trace::HighScore>,
    pub eligible_for_score: bool,
    pub hires: Hires,
    pub contacts: Scene,
    pub keyboard: Keyboard,
    pub speaker: Speaker,
    pub sound: Sound,
    pub clock: Clock,
}

impl Game {
    pub fn load_level(record: &[u8; 256], level: u8, paced: bool) -> Result<Self, String> {
        let tiles = decode_level(record);
        let index = tiles
            .iter()
            .rposition(|&tile| tile == Tile::Player)
            .ok_or("level contains no player")?;
        let cell = Cell {
            col: (index % 28) as u8,
            row: (index / 28) as u8,
        };
        let mut game = Self {
            board: Board::new(tiles),
            player: Player {
                position: Position::centered(cell),
                animation: 8,
                facing: Facing::Right,
                motion: Motion::Standing,
                digging: None,
                command: Command::Stop,
                alive: true,
                collected_gold: false,
            },
            guards: Vec::new(),
            guard_schedule: GuardSchedule::default(),
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
            score: Score::default(),
            gold_count: 0,
            falling_sound: false,
            falling_pitch: 0x20,
            resurrection_col: 0,
            difficulty: 0,
            skip_level: false,
            high_scores: Vec::new(),
            eligible_for_score: true,
            hires: Hires::default(),
            contacts: Scene::default(),
            keyboard: Keyboard::default(),
            speaker: Speaker::default(),
            sound: Sound::default(),
            clock: Clock::new(paced),
        };

        // Reverse board traversal determines actor identities and which markers
        // survive an overfull board. Rendering follows that same cell order.
        for index in (0..448).rev() {
            let cell = Cell {
                col: (index % 28) as u8,
                row: (index / 28) as u8,
            };
            match game.board.cells[index] {
                Tile::HiddenLadder => {
                    if game.hidden_ladders.len() < 45 {
                        game.hidden_ladders.push(cell);
                    }
                    game.board.cells[index] = Tile::Empty;
                    game.board.terrain[index] = Tile::Empty;
                }
                Tile::Gold => game.gold_count = game.gold_count.wrapping_add(1),
                Tile::Guard => {
                    if game.guards.len() == 5 {
                        game.board.cells[index] = Tile::Empty;
                    } else {
                        game.guards
                            .push(Guard::new(GuardId::for_index(game.guards.len()), cell));
                    }
                    game.board.terrain[index] = Tile::Empty;
                }
                Tile::Player => {
                    if cell != game.player.position.cell {
                        game.board.cells[index] = Tile::Empty;
                    }
                    game.board.terrain[index] = Tile::Empty;
                }
                _ => (),
            }
            for (page, tile) in [(0, game.board.cells[index]), (1, game.board.terrain[index])] {
                game.paint_tile(
                    page,
                    if tile == Tile::Trap { 1 } else { tile as u8 },
                    cell.col,
                    cell.row,
                );
            }
        }
        let (sprite, x, y) = game.player_sprite_coordinates();
        game.overlay_shape(sprite, x, y);
        game.put_status();
        Ok(game)
    }

    pub fn player_sprite_coordinates(&self) -> (u8, u8, u8) {
        const ANIMATION: [u8; 18] = [
            11, 12, 13, 24, 25, 26, 15, 19, 9, 16, 17, 21, 22, 23, 37, 20, 14, 18,
        ];
        let (x, y) = self.player.position.pixels();
        (ANIMATION[usize::from(self.player.animation)], x, y)
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
            high_scores: self.high_scores.clone(),
            session: self.session.clone(),
            actors,
            active: self.board.cells.to_vec(),
            background: self.board.terrain.to_vec(),
            hidden_ladders,
            holes,
        }
    }
}
