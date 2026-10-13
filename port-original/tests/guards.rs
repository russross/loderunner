use lode_original::guards::GuardAction;
use lode_original::state::Game;
use lode_original::storage::original_level;
use lode_trace::{Motion, Tile};

fn fixture(player: (usize, usize), guards: &[(usize, usize)]) -> Game {
    let mut record = [0; 256];
    let mut put = |col: usize, row: usize, tile: Tile| {
        let index = row * 28 + col;
        record[index / 2] |= (tile as u8) << ((index % 2) * 4);
    };
    put(player.0, player.1, Tile::Player);
    for &(col, row) in guards {
        put(col, row, Tile::Guard);
    }
    Game::load_level(&record, 1, false).unwrap()
}

#[test]
fn one_guard_moves_in_two_of_three_scheduler_phases() {
    let mut game = fixture((20, 15), &[(5, 15)]);
    let mut journal = vec![];
    for _ in 0..3 {
        game.move_guards(&mut journal).unwrap();
    }
    assert_eq!(game.guards.x_adjusts[1], 4);
    assert_eq!(game.guards.phase, 0);
    assert_eq!(game.guards.motions[1], Some(Motion::Right));
}

#[test]
fn guard_cell_entry_kills_player_and_emits_object_contact() {
    let mut game = fixture((6, 15), &[(5, 15)]);
    let mut guard = game.load_guard(1);
    guard.x = 4;
    game.try_guard_move(&mut guard, GuardAction::Right, &mut vec![])
        .unwrap();
    assert!(!game.alive);
    assert_eq!(game.active[15 * 28 + 6], Tile::Guard);
}

#[test]
fn carrying_gold_drop_waits_for_empty_background() {
    let mut game = fixture((20, 15), &[(5, 15)]);
    let mut guard = game.load_guard(1);
    guard.gold = 0xff;
    game.background[15 * 28 + 5] = Tile::Ladder;
    let mut journal = vec![];
    game.guard_drop_gold(&mut guard, &mut journal);
    assert_eq!(guard.gold, 0xff);
    assert!(journal.is_empty());
    game.background[15 * 28 + 5] = Tile::Empty;
    game.guard_drop_gold(&mut guard, &mut journal);
    assert_eq!(guard.gold, 0);
    assert_eq!(game.background[15 * 28 + 5], Tile::Gold);
    assert_eq!(journal.len(), 1);
}

#[test]
fn burial_resurrection_retries_occupied_target() {
    let mut game = fixture((20, 15), &[(5, 15)]);
    game.bury_guard(5, 15, &mut vec![]).unwrap();
    assert_eq!(game.guards.resurrection_timers[1], 20);
    let index = usize::from(game.guards.rows[1]) * 28 + usize::from(game.guards.cols[1]);
    game.active[index] = Tile::Player;
    for _ in 0..20 {
        game.guard_resurrections().unwrap();
    }
    assert_eq!(game.guards.resurrection_timers[1], 1);
    game.active[index] = Tile::Empty;
    game.guard_resurrections().unwrap();
    assert_eq!(game.guards.resurrection_timers[1], 0);
    assert_eq!(game.active[index], Tile::Guard);
}

#[test]
fn route_search_and_updates_remain_bounded_on_all_original_boards() {
    for level in 1..=150 {
        let mut game = Game::load_level(&original_level(level).unwrap(), level, false).unwrap();
        for _ in 0..40 {
            game.move_guards(&mut vec![]).unwrap();
            game.guard_resurrections().unwrap();
            game.snapshot().validate().unwrap();
            if !game.alive {
                break;
            }
        }
    }
}
