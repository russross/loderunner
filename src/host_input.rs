//! Held movement is sampled at gameplay input boundaries, independently of keys.

use crate::key_code;
use lode_trace::Phase;
use loderunner::actors::Command;
use loderunner::state::Game;
use sdl2::keyboard::{Keycode, Mod};

#[derive(Default)]
pub struct HeldArrows(Vec<Keycode>);

impl HeldArrows {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    // Physical presses establish priority. OS repeat cannot promote an older
    // held arrow over the most recently pressed one.
    pub fn press(&mut self, key: Keycode, modifiers: Mod, repeat: bool) -> bool {
        if !matches!(
            key,
            Keycode::Left | Keycode::Right | Keycode::Up | Keycode::Down
        ) || modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
        {
            return false;
        }
        if !repeat {
            self.release(key);
            self.0.push(key);
        }
        true
    }

    pub fn release(&mut self, key: Keycode) {
        self.0.retain(|&held| held != key);
    }

    // Fresh key commands and the original pending latch always take priority.
    // Falling, digging and pause retain their original observation boundaries.
    pub fn delivery(&self, game: &Game, key: Option<u8>) -> Option<u8> {
        if key.is_some()
            || !game.host.commodore
            || game.demonstrating
            || game.session.phase != Phase::Playing
            || game.player.digging.is_some()
            || game.player_should_fall()
            || game.keyboard.read() & 0x80 != 0
        {
            return key;
        }

        // Feed only changes to the persistent command into the recording path.
        // A released arrow stops movement but cannot cancel a pending action.
        let held = self.0.last().and_then(|&key| key_code(key, Mod::NOMOD));
        match held {
            Some(code) if game.player.command != Command::from_key(code) => Some(code),
            None if matches!(game.player.command, Command::Move(_)) => Some(0xa0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HeldArrows;
    use lode_trace::{Phase, Tile};
    use loderunner::actors::{Command, Direction};
    use loderunner::recording::Recording;
    use loderunner::session::{Configuration, Run};
    use loderunner::state::Game;
    use loderunner::storage::LevelRecord;
    use sdl2::keyboard::{Keycode, Mod};

    fn game(row: usize) -> Game {
        let mut sector = [0; 256];
        for (col, row, tile) in
            (0..28)
                .map(|col| (col, 15, Tile::Brick))
                .chain([(5, row, Tile::Player)])
        {
            let index = row * 28 + col;
            sector[index / 2] |= (tile as u8) << ((index % 2) * 4);
        }
        let mut game = Game::load_level(&LevelRecord(sector).decode().unwrap(), 1, false);
        game.host.commodore = true;
        game
    }

    fn update(game: &mut Game, held: &HeldArrows, key: Option<u8>) -> Vec<u8> {
        if let Some(code) = held.delivery(game, key) {
            game.keyboard.deliver(code);
        }
        game.update_player(&mut vec![])
            .unwrap()
            .into_iter()
            .map(|input| input.key)
            .collect()
    }

    #[test]
    fn release_cannot_replace_a_dig_in_the_same_poll() {
        for dig in [0xd5, 0xcf] {
            let mut game = game(14);
            let mut held = HeldArrows::default();
            held.press(Keycode::Right, Mod::NOMOD, false);
            held.release(Keycode::Right);
            assert_eq!(update(&mut game, &held, Some(dig)), [dig]);
            assert!(game.player.digging.is_some());
            for _ in 0..12 {
                assert!(update(&mut game, &held, None).is_empty());
            }
            let col = if dig == 0xd5 { 4 } else { 6 };
            assert_eq!(game.board.cells[15 * 28 + col], Tile::Empty);
        }
    }

    #[test]
    fn held_movement_resumes_after_dig_and_stops_on_release() {
        let mut game = game(14);
        let mut held = HeldArrows::default();
        held.press(Keycode::Right, Mod::NOMOD, false);
        assert_eq!(update(&mut game, &held, Some(0xd5)), [0xd5]);
        for _ in 0..12 {
            assert!(update(&mut game, &held, None).is_empty());
        }
        assert_eq!(update(&mut game, &held, None), [0xcc]);
        let position = game.player.position;
        assert!(update(&mut game, &held, None).is_empty());
        assert_ne!(game.player.position.x(), position.x());
        held.release(Keycode::Right);
        let position = game.player.position;
        assert_eq!(update(&mut game, &held, None), [0xa0]);
        assert_eq!(game.player.position.x(), position.x());
    }

    #[test]
    fn falling_preserves_pending_dig_despite_arrow_release_or_repeat() {
        let mut game = game(13);
        let mut held = HeldArrows::default();
        held.press(Keycode::Right, Mod::NOMOD, false);
        assert!(update(&mut game, &held, Some(0xd5)).is_empty());
        held.press(Keycode::Right, Mod::NOMOD, true);
        held.release(Keycode::Right);
        while game.player_should_fall() {
            assert!(update(&mut game, &held, None).is_empty());
        }
        assert_eq!(update(&mut game, &held, None), [0xd5]);
        assert!(game.player.digging.is_some());
    }

    #[test]
    fn later_real_key_still_replaces_unobserved_input() {
        let mut game = game(13);
        let held = HeldArrows::default();
        assert!(update(&mut game, &held, Some(0xd5)).is_empty());
        assert!(update(&mut game, &held, Some(0xa0)).is_empty());
        while game.player_should_fall() {
            update(&mut game, &held, None);
        }
        assert_eq!(update(&mut game, &held, None), [0xa0]);
        assert!(game.player.digging.is_none());
    }

    #[test]
    fn newest_held_arrow_wins_until_released_and_repeat_does_not_reorder() {
        let mut game = game(14);
        let mut held = HeldArrows::default();
        held.press(Keycode::Left, Mod::NOMOD, false);
        held.press(Keycode::Right, Mod::NOMOD, false);
        held.press(Keycode::Left, Mod::NOMOD, true);
        assert_eq!(update(&mut game, &held, None), [0xcc]);
        held.release(Keycode::Right);
        assert_eq!(update(&mut game, &held, None), [0xca]);
        assert_eq!(game.player.command, Command::Move(Direction::Left));
    }

    #[test]
    fn pause_retains_hold_changes_until_gameplay_resumes() {
        let mut game = game(14);
        let mut held = HeldArrows::default();
        held.press(Keycode::Right, Mod::NOMOD, false);
        update(&mut game, &held, None);
        game.session.phase = Phase::Paused;
        held.release(Keycode::Right);
        assert_eq!(held.delivery(&game, None), None);
        assert_eq!(held.delivery(&game, Some(0x9b)), Some(0x9b));
        game.session.phase = Phase::Playing;
        assert_eq!(update(&mut game, &held, None), [0xa0]);
    }

    #[test]
    fn blocked_held_movement_retries_without_os_repeat() {
        let mut game = game(14);
        let mut held = HeldArrows::default();
        held.press(Keycode::Right, Mod::NOMOD, false);
        game.board.cells[14 * 28 + 6] = Tile::Stone;
        update(&mut game, &held, None);
        update(&mut game, &held, None);
        let position = game.player.position;
        update(&mut game, &held, None);
        assert_eq!(game.player.position.x(), position.x());
        game.board.cells[14 * 28 + 6] = Tile::Empty;
        update(&mut game, &held, None);
        assert_ne!(game.player.position.x(), position.x());
    }

    #[test]
    fn classic_waiting_and_demo_inputs_keep_their_existing_policy() {
        let mut game = game(14);
        let mut held = HeldArrows::default();
        held.press(Keycode::Right, Mod::NOMOD, false);
        game.host.commodore = false;
        assert_eq!(held.delivery(&game, None), None);
        assert_eq!(held.delivery(&game, Some(0xcf)), Some(0xcf));
        game.host.commodore = true;
        game.session.phase = Phase::Waiting;
        assert_eq!(held.delivery(&game, None), None);
        game.session.phase = Phase::Playing;
        game.demonstrating = true;
        assert_eq!(held.delivery(&game, None), None);
        held.clear();
        game.demonstrating = false;
        assert_eq!(held.delivery(&game, None), None);
    }

    #[test]
    fn recordings_insert_only_needed_stops_and_replay_in_classic_mode() {
        // Preserve an unfinished objective so the fixture remains in gameplay.
        let mut sector = [0; 256];
        for (col, row, tile) in (0..28)
            .map(|col| (col, 15, Tile::Brick))
            .chain([(5, 14, Tile::Player), (25, 14, Tile::Gold)])
        {
            let index = row * 28 + col;
            sector[index / 2] |= (tile as u8) << ((index % 2) * 4);
        }
        let configuration = Configuration {
            levels: vec![LevelRecord(sector)],
            start: 0,
            wait_on_level_start: false,
        };
        let mut live = Run::new(configuration.clone(), false).unwrap();
        live.game.host.commodore = true;
        let mut held = HeldArrows::default();
        let mut recording = Recording::new(configuration);

        // First release stops persistent motion. The second occurs during a
        // dig, whose own command reset already supplies the required stop.
        for round in 0..20 {
            match round {
                0 | 3 => {
                    held.press(Keycode::Right, Mod::NOMOD, false);
                }
                2 | 4 => held.release(Keycode::Right),
                _ => (),
            }
            let key = (round == 3).then_some(0xd5);
            let delivery = held.delivery(&live.game, key);
            live.step(delivery).unwrap();
            recording.capture(&live).unwrap();
        }
        assert_eq!(recording.key(0), Some(0xcc));
        assert_eq!(recording.key(2), Some(0xa0));
        assert_eq!(recording.key(3), Some(0xd5));
        assert!((4..20).all(|round| recording.key(round).is_none()));

        // Playback needs only Apple II codes, with no knowledge of host holds.
        let recording = Recording::from_json(&recording.to_json().unwrap()).unwrap();
        let mut replay = Run::new(recording.configuration.clone(), false).unwrap();
        assert!(!replay.game.host.commodore);
        for round in 0..recording.rounds {
            replay.step(recording.key(round)).unwrap();
        }
        assert_eq!(live.trace().unwrap(), replay.trace().unwrap());
    }
}
