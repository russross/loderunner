use lode_port::score::Score;
use lode_port::state::Game;
use lode_trace::{Motion, Occurrence, Tile};

fn board(player_col: usize, player_row: usize, tiles: &[(usize, usize, Tile)]) -> Game {
    let mut record = [0; 256];
    for &(col, row, tile) in
        tiles
            .iter()
            .chain(std::iter::once(&(player_col, player_row, Tile::Player)))
    {
        let index = row * 28 + col;
        let shift = (index % 2) * 4;
        record[index / 2] = (record[index / 2] & !(15 << shift)) | ((tile as u8) << shift);
    }
    Game::load_level(&record, 1, false).unwrap()
}

#[test]
fn persistent_keyboard_command_crosses_cell_and_stop_keeps_position() {
    let mut game = board(5, 15, &[]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcc);
    assert_eq!(game.move_player(&mut journal).unwrap().len(), 1);
    for _ in 0..4 {
        assert!(game.move_player(&mut journal).unwrap().is_empty());
    }
    assert_eq!((game.player.col, game.player.x_adjust), (6, 2));
    assert_eq!(game.active[15 * 28 + 5], Tile::Empty);
    assert_eq!(game.active[15 * 28 + 6], Tile::Player);
    game.keyboard.deliver(0xa0);
    game.move_player(&mut journal).unwrap();
    assert_eq!(game.player.motion, Motion::Standing);
    assert_eq!((game.player.col, game.player.x_adjust), (6, 2));
    assert!(game.alive);
}

#[test]
fn gold_is_collected_at_center_and_only_once() {
    let mut game = board(5, 15, &[(6, 15, Tile::Gold)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcc);
    for _ in 0..4 {
        game.move_player(&mut journal).unwrap();
    }
    assert_eq!(game.gold_count, 1);
    game.move_player(&mut journal).unwrap();
    assert_eq!(game.gold_count, 0);
    assert_eq!(game.session.score, 250);
    assert_eq!(
        journal
            .iter()
            .filter(|(_, event)| *event == Occurrence::GoldCollected)
            .count(),
        1
    );
    assert_eq!(game.background[15 * 28 + 6], Tile::Empty);
    assert!(game.alive);
}

#[test]
fn falling_defers_observation_of_latched_key_until_landing() {
    let mut game = board(5, 13, &[(5, 15, Tile::Brick)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xa0);
    while game.player_should_fall() {
        assert!(game.move_player(&mut journal).unwrap().is_empty());
        assert_eq!(game.player.motion, Motion::Falling);
    }
    assert_eq!((game.player.row, game.player.y_adjust), (14, 2));
    assert_eq!(game.move_player(&mut journal).unwrap().len(), 1);
    assert_eq!(
        journal
            .iter()
            .filter(|(_, event)| *event == Occurrence::Landed)
            .count(),
        1
    );
    assert_eq!(game.player.motion, Motion::Standing);
}

#[test]
fn digging_opens_after_thirteen_calls_without_moving_player() {
    for (key, col) in [(0xd5, 4), (0xcf, 6)] {
        let mut game = board(
            5,
            14,
            &[
                (4, 15, Tile::Brick),
                (5, 15, Tile::Brick),
                (6, 15, Tile::Brick),
            ],
        );
        let mut journal = vec![];
        game.keyboard.deliver(key);
        for _ in 0..12 {
            game.move_player(&mut journal).unwrap();
            assert_eq!(game.active[15 * 28 + col], Tile::Brick);
        }
        game.move_player(&mut journal).unwrap();
        assert_eq!(game.active[15 * 28 + col], Tile::Empty);
        assert_eq!(game.background[15 * 28 + col], Tile::Brick);
        assert_eq!((game.player.col, game.player.row), (5, 14));
        assert_eq!(game.holes.timers[0], 180);
        assert_eq!(game.sound.index, 13);
        assert!(game.alive);
    }
}

#[test]
fn digging_cancelled_by_obstruction_keeps_brick() {
    let mut game = board(5, 14, &[(5, 15, Tile::Brick), (6, 15, Tile::Brick)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcf);
    game.move_player(&mut journal).unwrap();
    game.active[14 * 28 + 6] = Tile::Guard;
    game.move_player(&mut journal).unwrap();
    assert_eq!(game.dig_direction, 0);
    assert_eq!(game.active[15 * 28 + 6], Tile::Brick);
    assert!(game.holes.timers.iter().all(|timer| *timer == 0));
}

#[test]
fn ladder_up_and_trap_down_follow_different_obstacle_rules() {
    let mut up = board(5, 14, &[(5, 14, Tile::Ladder), (5, 13, Tile::Trap)]);
    // The fixture's player replaces the level marker; terrain underneath is explicit.
    up.background[14 * 28 + 5] = Tile::Ladder;
    assert!(!up.try_moving_up(&mut vec![]).unwrap());
    let mut down = board(5, 13, &[(5, 14, Tile::Trap)]);
    assert!(down.try_moving_down(&mut vec![]).unwrap());
    assert_eq!(down.player.y_adjust, 3);
}

#[test]
fn packed_score_carries_across_pairs_and_wraps() {
    let mut score = Score([0x99, 0x99, 0x99, 0x99]);
    score.add(1, 0);
    assert_eq!(score, Score::default());
    score.add(0x50, 2);
    assert_eq!(score.0, [0x50, 2, 0, 0]);
    assert_eq!(score.decimal(), 250);
}
