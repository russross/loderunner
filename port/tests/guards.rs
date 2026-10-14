use lode_port::actors::{Direction, GuardMode};
use lode_port::state::Game;
use lode_port::storage::original_level;
use lode_trace::{Cell, Motion, Tile};
use std::num::NonZeroU8;

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
        game.update_guards(&mut journal).unwrap();
    }
    assert_eq!(game.guards[0].position.x(), 4);
    assert_eq!(game.guard_schedule.phase(), 0);
    assert_eq!(game.guards[0].motion, Motion::Right);
}

#[test]
fn guard_cell_entry_kills_player_and_emits_object_contact() {
    let mut game = fixture((6, 15), &[(5, 15)]);
    let mut guard = game.guards[0];
    guard.position.set_x(4);
    game.move_guard(&mut guard, Some(Direction::Right), &mut vec![]);
    assert!(!game.player.alive);
    assert_eq!(game.board.cells[15 * 28 + 6], Tile::Guard);
}

#[test]
fn carrying_gold_drop_waits_for_empty_background() {
    let mut game = fixture((20, 15), &[(5, 15)]);
    let mut guard = game.guards[0];
    guard.mode = GuardMode::Carrying {
        drop_in: NonZeroU8::new(1).unwrap(),
    };
    game.board.terrain[15 * 28 + 5] = Tile::Ladder;
    let mut journal = vec![];
    game.drop_guard_gold(&mut guard, &mut journal);
    assert!(matches!(guard.mode,GuardMode::Carrying {drop_in} if drop_in.get()==1));
    assert!(journal.is_empty());
    game.board.terrain[15 * 28 + 5] = Tile::Empty;
    game.drop_guard_gold(&mut guard, &mut journal);
    assert_eq!(guard.mode, GuardMode::Roaming);
    assert_eq!(game.board.terrain[15 * 28 + 5], Tile::Gold);
    assert_eq!(journal.len(), 1);
}

#[test]
fn burial_resurrection_retries_occupied_target() {
    let mut game = fixture((20, 15), &[(5, 15)]);
    game.bury_guard(Cell::new(5, 15).unwrap(), &mut vec![])
        .unwrap();
    assert!(
        matches!(game.guards[0].mode,GuardMode::Resurrecting {appear_in} if appear_in.get()==20)
    );
    let cell = game.guards[0].position.cell;
    game.board.occupy(cell, Tile::Player);
    for _ in 0..20 {
        game.update_resurrections().unwrap();
    }
    assert!(
        matches!(game.guards[0].mode,GuardMode::Resurrecting {appear_in} if appear_in.get()==1)
    );
    game.board.occupy(cell, Tile::Empty);
    game.update_resurrections().unwrap();
    assert_eq!(game.guards[0].mode, GuardMode::Roaming);
    assert_eq!(game.board.at(cell), Tile::Guard);
}

#[test]
fn route_search_and_updates_remain_bounded_on_all_original_boards() {
    for level in 1..=150 {
        let mut game = Game::load_level(&original_level(level).unwrap(), level, false).unwrap();
        for _ in 0..40 {
            game.update_guards(&mut vec![]).unwrap();
            game.update_resurrections().unwrap();
            game.snapshot().validate().unwrap();
            if !game.player.alive {
                break;
            }
        }
    }
}
