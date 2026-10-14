use lode_port::state::Game;
use lode_port::storage::{decode_level, original_high_scores, original_level};
use lode_trace::{ActorId, Cell, Tile};

#[test]
fn all_original_levels_decode_and_initialize_observer_state() {
    for level in 1..=150 {
        let record = original_level(level).unwrap();
        let game = Game::load_level(&record, level, false).unwrap();
        game.snapshot().validate().unwrap();
        assert!(game.guards.len() <= 5);
        assert!(game.hidden_ladders.len() <= 45);
    }
}

// Reference conversion preserves full sectors, including bytes outside the
// playfield, and the initial score table used by native capture configuration.
#[test]
fn generated_assets_preserve_original_records_and_scores() {
    for level in 1..=150 {
        assert_eq!(
            original_level(level).unwrap(),
            lode_original::storage::original_level(level).unwrap()
        );
    }
    assert_eq!(
        original_high_scores(),
        lode_original::storage::original_high_scores().unwrap()
    );
}

#[test]
fn reverse_scan_retains_last_player_and_numbers_guards_from_bottom_right() {
    let mut record = [0_u8; 256];
    record[0] = 0x89;
    record[223] = 0x98;
    let game = Game::load_level(&record, 1, false).unwrap();
    let state = game.snapshot();
    assert_eq!(state.actors[0].cell, Cell::new(27, 15).unwrap());
    assert_eq!(state.actors[1].id, ActorId::Guard(1));
    assert_eq!(state.actors[1].cell, Cell::new(26, 15).unwrap());
    assert_eq!(state.actors[2].cell, Cell::new(1, 0).unwrap());
    assert_eq!(state.active[0], Tile::Empty);
    assert_eq!(state.background[447], Tile::Empty);
}

#[test]
fn decoder_discards_invalid_nibbles_and_ignores_sector_tail() {
    let mut record = [0xff; 256];
    record[0] = 0x39;
    let map = decode_level(&record);
    assert_eq!(map[0], Tile::Player);
    assert_eq!(map[1], Tile::Ladder);
    assert!(map[2..].iter().all(|tile| *tile == Tile::Empty));
    assert!(original_level(0).is_err());
    assert!(original_level(151).is_err());
    assert!(Game::load_level(&[0; 256], 1, false).is_err());
}
