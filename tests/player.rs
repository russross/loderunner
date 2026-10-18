use lode_trace::{Motion, Occurrence, Tile};
use loderunner::actors::Direction;
use loderunner::score::Score;
use loderunner::state::Game;
use loderunner::storage::LevelRecord;

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
    Game::load_level(&LevelRecord(record).decode().unwrap(), 1, false)
}

#[test]
fn persistent_keyboard_command_crosses_cell_and_stop_keeps_position() {
    let mut game = board(5, 15, &[]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcc);
    assert_eq!(game.update_player(&mut journal).unwrap().len(), 1);
    for _ in 0..4 {
        assert!(game.update_player(&mut journal).unwrap().is_empty());
    }
    assert_eq!(
        (game.player.position.cell.col, game.player.position.x()),
        (6, 2)
    );
    assert_eq!(game.board.cells[15 * 28 + 5], Tile::Empty);
    assert_eq!(game.board.cells[15 * 28 + 6], Tile::Player);
    game.keyboard.deliver(0xa0);
    game.update_player(&mut journal).unwrap();
    assert_eq!(game.player.motion, Motion::Standing);
    assert_eq!(
        (game.player.position.cell.col, game.player.position.x()),
        (6, 2)
    );
    assert!(game.player.alive);
}

#[test]
fn gold_is_collected_at_center_and_only_once() {
    let mut game = board(5, 15, &[(6, 15, Tile::Gold)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcc);
    for _ in 0..4 {
        game.update_player(&mut journal).unwrap();
    }
    assert!(!game.gold.allows_completion());
    game.update_player(&mut journal).unwrap();
    assert!(game.gold.allows_completion());
    assert_eq!(game.score.decimal(), 250);
    assert_eq!(
        journal
            .iter()
            .filter(|(_, event)| *event == Occurrence::GoldCollected)
            .count(),
        1
    );
    assert_eq!(game.board.terrain[15 * 28 + 6], Tile::Empty);
    assert!(game.player.alive);
}

#[test]
fn falling_defers_observation_of_latched_key_until_landing() {
    let mut game = board(5, 13, &[(5, 15, Tile::Brick)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xa0);
    while game.player_should_fall() {
        assert!(game.update_player(&mut journal).unwrap().is_empty());
        assert_eq!(game.player.motion, Motion::Falling);
    }
    assert_eq!(
        (game.player.position.cell.row, game.player.position.y()),
        (14, 2)
    );
    assert_eq!(game.update_player(&mut journal).unwrap().len(), 1);
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
            game.update_player(&mut journal).unwrap();
            assert_eq!(game.board.cells[15 * 28 + col], Tile::Brick);
        }
        game.update_player(&mut journal).unwrap();
        assert_eq!(game.board.cells[15 * 28 + col], Tile::Empty);
        assert_eq!(game.board.terrain[15 * 28 + col], Tile::Brick);
        assert_eq!(
            (game.player.position.cell.col, game.player.position.cell.row),
            (5, 14)
        );
        assert_eq!(game.holes.entries[0].unwrap().remaining.get(), 180);
        assert_eq!(game.sound.len(), 13);
        assert!(game.player.alive);
    }
}

#[test]
fn digging_cancelled_by_obstruction_keeps_brick() {
    let mut game = board(5, 14, &[(5, 15, Tile::Brick), (6, 15, Tile::Brick)]);
    let mut journal = vec![];
    game.keyboard.deliver(0xcf);
    game.update_player(&mut journal).unwrap();
    game.board.cells[14 * 28 + 6] = Tile::Guard;
    game.update_player(&mut journal).unwrap();
    assert!(game.player.digging.is_none());
    assert_eq!(game.board.cells[15 * 28 + 6], Tile::Brick);
    assert!(game.holes.entries.iter().all(Option::is_none));
}

#[test]
fn ladder_up_and_trap_down_follow_different_obstacle_rules() {
    let mut up = board(5, 14, &[(5, 14, Tile::Ladder), (5, 13, Tile::Trap)]);
    // The fixture's player replaces the level marker; terrain underneath is explicit.
    up.board.terrain[14 * 28 + 5] = Tile::Ladder;
    assert!(!up.move_player(Direction::Up, &mut vec![]).unwrap());
    let mut down = board(5, 13, &[(5, 14, Tile::Trap)]);
    assert!(down.move_player(Direction::Down, &mut vec![]).unwrap());
    assert_eq!(down.player.position.y(), 3);
}

#[test]
fn decimal_score_wraps_and_renders_seven_digits() {
    let mut score = Score::new(99_999_999);
    score.add(1);
    assert_eq!(score, Score::default());
    score.add(250);
    assert_eq!(score.visible_digits(), [0, 0, 0, 0, 2, 5, 0]);
    assert_eq!(score.decimal(), 250);
}
