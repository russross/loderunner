use lode_trace::{Input, Phase, Tile};
use loderunner::app::{Application, NEXT_PACK, PREVIOUS_PACK};
use loderunner::cli::Arguments;
use loderunner::config::{CLASSIC, Paths};
use loderunner::database::{Attempt, Catalog, Outcome, PlayerStore, demos, load_demo, timestamp};
use loderunner::recording::Recording;
use loderunner::render::snap_size;
use loderunner::session::{Configuration, Run};
use loderunner::storage::LevelRecord;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lode-app-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn paths(&self) -> Paths {
        Paths {
            catalog: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/levels.sqlite3"),
            player: self.0.join("data/player.sqlite3"),
            config: self.0.join("config/config.json"),
        }
    }
    fn app(&self) -> (Application, Run) {
        let arguments = Arguments::parse(std::iter::empty()).unwrap().unwrap();
        let (app, configuration) = Application::open_at(&arguments, self.paths()).unwrap();
        let mut run = Run::new(configuration, false).unwrap();
        run.disable_tracing().unwrap();
        app.configure(&mut run.game);
        (app, run)
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn tick(app: &mut Application, run: &mut Run, input: Option<u8>) {
    let mut present = |_: &mut loderunner::state::Game| Ok(());
    if let Some(input) = app
        .before(run, input, std::time::Instant::now(), &mut present)
        .unwrap()
    {
        let phase = run.game.session.phase;
        run.step(input).unwrap();
        app.after(run, phase, &mut present).unwrap();
    }
}

fn attempt(configuration: Configuration, started_at: i64, duration: u64) -> Attempt {
    let run = Run::new(configuration.clone(), false).unwrap();
    let mut recording = Recording::new(configuration);
    recording
        .push(&[Input {
            observation: 0,
            key: 0xcc,
        }])
        .unwrap();
    recording.push(&[]).unwrap();
    Attempt {
        started_at,
        session_started_at: started_at,
        pack: CLASSIC.into(),
        level: 1,
        seed: run.game.attempt_seed(),
        recording,
        elapsed_cycles: duration,
        gold: 3,
        guard_deaths: 1,
    }
}

#[test]
fn catalog_loads_every_pack_and_validates_all_boards() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let packs = catalog.packs().unwrap();
    assert_eq!(packs.len(), 5);
    for (pack, expected) in packs.iter().zip([150, 150, 17, 66, 51]) {
        let configuration = catalog.configuration(pack, 1).unwrap();
        assert_eq!(configuration.levels.len(), expected);
        assert!(catalog.configuration(pack, expected as u8).is_ok());
        assert!(catalog.configuration(pack, 0).is_err());
    }
    let connection = Connection::open(&files.paths().catalog).unwrap();
    let supplied: i64 = connection
        .query_row("SELECT count(*) FROM recordings", [], |row| row.get(0))
        .unwrap();
    assert_eq!(supplied, 0);
}

#[test]
fn pack_resume_and_preferences_survive_new_application_sessions() {
    let files = Files::new();
    let (mut app, mut run) = files.app();
    tick(&mut app, &mut run, Some(0x83));
    assert_eq!(run.level_number(), 2);
    tick(&mut app, &mut run, Some(NEXT_PACK));
    assert_eq!(run.level_number(), 1);
    tick(&mut app, &mut run, Some(0x83));
    tick(&mut app, &mut run, Some(0x83));
    tick(&mut app, &mut run, Some(PREVIOUS_PACK));
    assert_eq!(run.level_number(), 2);
    tick(&mut app, &mut run, Some(0x8b));
    tick(&mut app, &mut run, Some(0x93));
    tick(&mut app, &mut run, Some(0x88));
    run.game.host.size = [875, 600];
    app.finish(&run).unwrap();
    drop(app);
    let (mut app, mut run) = files.app();
    assert_eq!(run.level_number(), 2);
    assert_eq!(run.game.host.size, [875, 600]);
    assert!(run.game.host.commodore);
    assert!(!run.game.sound.enabled);
    assert_eq!(run.game.sound.frame_period, 7);
    tick(&mut app, &mut run, Some(NEXT_PACK));
    assert_eq!(run.level_number(), 3);
}

#[test]
fn restart_from_pause_sacrifices_a_life_and_returns_to_ready() {
    let files = Files::new();
    let (mut app, mut run) = files.app();
    tick(&mut app, &mut run, Some(0xcc));
    tick(&mut app, &mut run, Some(0x9b));
    assert_eq!(run.game.session.phase, Phase::Paused);
    let cycles = run.game.clock.cycles;
    tick(&mut app, &mut run, Some(0x92));
    assert_eq!(run.game.session.phase, Phase::Waiting);
    assert_eq!(run.game.session.lives_used, 2);
    assert_eq!(run.game.clock.cycles, cycles);
}

#[test]
fn player_best_becomes_zero_and_other_results_have_signed_order() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let configuration = catalog.configuration(CLASSIC, 1).unwrap();
    let now = timestamp().unwrap();
    for (offset, duration, outcome) in [
        (0, 90, Outcome::Win),
        (1, 60, Outcome::Win),
        (2, 80, Outcome::Win),
        (3, 35, Outcome::Death),
        (4, 40, Outcome::Quit),
    ] {
        store
            .save(
                &attempt(configuration.clone(), now + offset, duration),
                outcome,
                now + 4,
            )
            .unwrap();
    }
    let choices = demos(&catalog, &store, CLASSIC, 1).unwrap();
    assert_eq!(
        choices.iter().map(|demo| demo.number).collect::<Vec<_>>(),
        [-2, -1, 0, 1, 2]
    );
    let zero = choices.iter().find(|demo| demo.number == 0).unwrap();
    let best = load_demo(&catalog, &store, CLASSIC, 1, zero).unwrap();
    assert_eq!(best.recording.key(0), Some(0xcc));
    assert_eq!(best.recording.rounds, 2);
    // Input rows join their natural recording identity and preserve metadata.
    let connection = Connection::open(files.paths().player).unwrap();
    let metadata: (i64, i64) = connection.query_row("SELECT gold_collected, guard_deaths FROM recordings NATURAL JOIN recording_inputs WHERE started_at=?", [now + 1], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(metadata, (3, 1));
}

#[test]
fn pruning_keeps_best_latest_and_recent_useful_runs() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let configuration = catalog.configuration(CLASSIC, 1).unwrap();
    let day = 86_400_000_000;
    let now = timestamp().unwrap();
    for (time, duration, outcome) in [
        (now - 100 * day, 90, Outcome::Win),
        (now - 99 * day, 100, Outcome::Win),
        (now - 98 * day, 110, Outcome::Win),
        (now - 97 * day, 200, Outcome::Death),
        (now - 1, 29, Outcome::Death),
        (now, 30, Outcome::Death),
    ] {
        store
            .save(
                &attempt(configuration.clone(), time, duration),
                outcome,
                now,
            )
            .unwrap();
    }
    let connection = Connection::open(files.paths().player).unwrap();
    let mut query = connection
        .prepare("SELECT elapsed_cycles FROM recordings ORDER BY started_at")
        .unwrap();
    let lengths: Vec<i64> = query
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(lengths, [90, 110, 30]);
    let orphans: i64 = connection.query_row("SELECT count(*) FROM recording_inputs LEFT JOIN recordings USING(pack_name,level_number,started_at) WHERE outcome IS NULL", [], |row| row.get(0)).unwrap();
    assert_eq!(orphans, 0);
}

#[test]
fn demo_speed_defaults_migrates_and_persists_independently_of_playing_speed() {
    let files = Files::new();
    fs::create_dir_all(files.paths().config.parent().unwrap()).unwrap();
    fs::write(&files.paths().config, r#"{"frame_period":9}"#).unwrap();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let now = timestamp().unwrap();
    store
        .save(
            &attempt(catalog.configuration(CLASSIC, 1).unwrap(), now, 90),
            Outcome::Win,
            now,
        )
        .unwrap();
    let (mut app, mut run) = files.app();
    assert_eq!(run.game.sound.frame_period, 9);
    tick(&mut app, &mut run, Some(0xc4));
    assert_eq!(run.game.clock.playback_speed(), 2);
    // Playback retains recorded timing instead of borrowing the live setting.
    assert_eq!(run.game.sound.frame_period, 6);
    tick(&mut app, &mut run, Some(0x95));
    assert_eq!(run.game.clock.playback_speed(), 3);
    tick(&mut app, &mut run, Some(0xcc));
    assert_eq!(run.game.clock.playback_speed(), 3);
    tick(&mut app, &mut run, Some(0x87));
    assert_eq!(run.game.clock.playback_speed(), 1);
    assert_eq!(run.game.sound.frame_period, 9);
    app.finish(&run).unwrap();
    drop(app);
    let (mut app, mut run) = files.app();
    assert_eq!(run.game.sound.frame_period, 9);
    tick(&mut app, &mut run, Some(0xc4));
    assert_eq!(run.game.clock.playback_speed(), 3);
    for _ in 0..20 {
        tick(&mut app, &mut run, Some(0x88));
    }
    assert_eq!(run.game.clock.playback_speed(), 1);
    for _ in 0..20 {
        tick(&mut app, &mut run, Some(0x95));
    }
    assert_eq!(run.game.clock.playback_speed(), 16);
    tick(&mut app, &mut run, Some(0x87));
    tick(&mut app, &mut run, Some(0x88));
    app.finish(&run).unwrap();
    let settings = loderunner::config::Settings::load(&files.paths().config).unwrap();
    assert_eq!(settings.frame_period, 10);
    assert_eq!(settings.demo_speed, 16);
    fs::write(&files.paths().config, r#"{"demo_speed":0}"#).unwrap();
    assert!(loderunner::config::Settings::load(&files.paths().config).is_err());
}

#[test]
fn mute_is_shared_and_persistent_across_live_play_and_demo_playback() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let now = timestamp().unwrap();
    let mut demo = attempt(catalog.configuration(CLASSIC, 1).unwrap(), now, 90);
    demo.recording = Recording::new(demo.recording.configuration.clone());
    demo.recording
        .push(&[Input {
            observation: 0,
            key: 0x93,
        }])
        .unwrap();
    demo.recording.push(&[]).unwrap();
    store.save(&demo, Outcome::Win, now).unwrap();

    // Live mute overrides the recording seed and its recorded sound control.
    let (mut app, mut run) = files.app();
    tick(&mut app, &mut run, Some(0x93));
    tick(&mut app, &mut run, Some(0xc4));
    tick(&mut app, &mut run, None);
    assert!(!run.game.sound.enabled);
    tick(&mut app, &mut run, Some(0x93));
    assert!(run.game.sound.enabled);
    for _ in 0..5 {
        tick(&mut app, &mut run, None);
        assert!(run.game.sound.enabled);
    }
    tick(&mut app, &mut run, Some(0x87));
    assert!(run.game.sound.enabled);

    // A demo preference survives closing the application while still playing it.
    tick(&mut app, &mut run, Some(0xc4));
    tick(&mut app, &mut run, Some(0x93));
    app.finish(&run).unwrap();
    drop(app);
    let (mut app, mut run) = files.app();
    assert!(!run.game.sound.enabled);
    tick(&mut app, &mut run, Some(0xc4));
    tick(&mut app, &mut run, None);
    assert!(!run.game.sound.enabled);
    tick(&mut app, &mut run, Some(0x87));
    assert!(!run.game.sound.enabled);
    tick(&mut app, &mut run, Some(0xcc));
    assert!(!run.game.sound.enabled);
}

#[test]
fn demo_uses_best_loops_and_returns_to_ready_without_recording_itself() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let now = timestamp().unwrap();
    store
        .save(
            &attempt(catalog.configuration(CLASSIC, 1).unwrap(), now, 90),
            Outcome::Win,
            now,
        )
        .unwrap();
    let (mut app, mut run) = files.app();
    tick(&mut app, &mut run, Some(0xc4));
    assert_eq!(app.demo_number(), Some(0));
    assert!(run.game.demonstrating);
    tick(&mut app, &mut run, Some(0x93));
    tick(&mut app, &mut run, Some(0x8b));
    assert_eq!(app.demo_number(), Some(0));
    let preferences = loderunner::config::Settings::load(&files.paths().config).unwrap();
    assert!(preferences.muted && preferences.display.commodore);
    for _ in 0..8 {
        tick(&mut app, &mut run, None);
    }
    assert_eq!(app.demo_number(), Some(0));
    tick(&mut app, &mut run, Some(0x87));
    assert_eq!(app.demo_number(), None);
    assert!(!run.game.demonstrating);
    assert_eq!(run.game.session.phase, Phase::Waiting);
    app.finish(&run).unwrap();
    let connection = Connection::open(files.paths().player).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM recordings", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn idle_timeout_starts_the_best_and_arrows_wrap_through_saved_failures() {
    let files = Files::new();
    let catalog = Catalog::open(&files.paths().catalog).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let configuration = catalog.configuration(CLASSIC, 1).unwrap();
    let now = timestamp().unwrap();
    store
        .save(&attempt(configuration.clone(), now, 90), Outcome::Win, now)
        .unwrap();
    store
        .save(
            &attempt(configuration, now + 1, 40),
            Outcome::Death,
            now + 1,
        )
        .unwrap();
    let (mut app, mut run) = files.app();
    app.ready();
    let mut present = |_: &mut loderunner::state::Game| Ok(());
    let handled = app
        .before(
            &mut run,
            None,
            std::time::Instant::now() + std::time::Duration::from_secs(31),
            &mut present,
        )
        .unwrap();
    assert!(handled.is_none());
    assert_eq!(app.demo_number(), Some(0));
    tick(&mut app, &mut run, Some(0xcc));
    assert_eq!(app.demo_number(), Some(-1));
    tick(&mut app, &mut run, Some(0xca));
    assert_eq!(app.demo_number(), Some(0));
    tick(&mut app, &mut run, Some(0xa0));
    assert_eq!(run.game.session.phase, Phase::Waiting);
}

#[test]
fn supplied_solution_keeps_zero_and_player_best_moves_to_one() {
    let files = Files::new();
    let catalog_path = files.0.join("catalog.sqlite3");
    fs::copy(files.paths().catalog, &catalog_path).unwrap();
    let mut store = PlayerStore::open(&files.paths().player).unwrap();
    let catalog = Catalog::open(&catalog_path).unwrap();
    let now = timestamp().unwrap();
    store
        .save(
            &attempt(catalog.configuration(CLASSIC, 1).unwrap(), now, 90),
            Outcome::Win,
            now,
        )
        .unwrap();
    let connection = Connection::open(&catalog_path).unwrap();
    connection
        .execute(
            "ATTACH DATABASE ? AS player",
            [files.paths().player.to_str().unwrap()],
        )
        .unwrap();
    connection.execute_batch("INSERT INTO recordings SELECT * FROM player.recordings; INSERT INTO recording_inputs SELECT * FROM player.recording_inputs;").unwrap();
    let choices = demos(&catalog, &store, CLASSIC, 1).unwrap();
    assert_eq!(
        choices.iter().map(|demo| demo.number).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        load_demo(&catalog, &store, CLASSIC, 1, &choices[0])
            .unwrap()
            .recording
            .rounds,
        2
    );
}

#[test]
fn live_wins_are_automatically_saved_with_replayable_initial_state() {
    let files = Files::new();
    let mut paths = files.paths();
    paths.catalog = files.0.join("catalog.sqlite3");
    fs::copy(files.paths().catalog, &paths.catalog).unwrap();
    let connection = Connection::open(&paths.catalog).unwrap();
    connection.execute("UPDATE level_tiles SET tile_kind=CASE WHEN tile_row=0 AND tile_column=0 THEN 9 WHEN tile_row=0 AND tile_column=1 THEN 7 WHEN tile_row=1 THEN 2 ELSE 0 END WHERE pack_name=? AND level_number=1", [CLASSIC]).unwrap();
    connection
        .execute(
            "DELETE FROM level_tiles WHERE pack_name=? AND level_number>1",
            [CLASSIC],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM levels WHERE pack_name=? AND level_number>1",
            [CLASSIC],
        )
        .unwrap();
    let arguments = Arguments::parse(std::iter::empty()).unwrap().unwrap();
    let (mut app, configuration) = Application::open_at(&arguments, paths).unwrap();
    let mut run = Run::new(configuration, false).unwrap();
    run.disable_tracing().unwrap();
    for _ in 0..5 {
        tick(&mut app, &mut run, Some(0xcc));
    }
    assert_eq!(run.game.session.phase, Phase::LevelCleared);
    let player = Connection::open(files.paths().player).unwrap();
    let metadata: (String, i64, i64) = player
        .query_row(
            "SELECT outcome, rounds, gold_collected FROM recordings",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(metadata, ("win".into(), 5, 1));
    tick(&mut app, &mut run, None);
    assert_eq!(run.level_number(), 1);
    assert_eq!(run.game.session.level, 1);
    // A new application starts at the saved level and can use that win as demo zero.
    drop(app);
    let paths = Paths {
        catalog: files.0.join("catalog.sqlite3"),
        ..files.paths()
    };
    let (mut app, configuration) = Application::open_at(&arguments, paths).unwrap();
    let mut replay = Run::new(configuration, false).unwrap();
    replay.disable_tracing().unwrap();
    tick(&mut app, &mut replay, Some(0xc4));
    assert_eq!(app.demo_number(), Some(0));
    for _ in 0..4 {
        tick(&mut app, &mut replay, None);
    }
    assert_eq!(replay.game.player.position.cell.col, 1);
    tick(&mut app, &mut replay, None);
    assert_eq!(app.demo_number(), Some(0));
    assert_eq!(replay.game.player.position.cell.col, 0);
}

#[test]
fn independent_attempt_seed_preserves_reloaded_guard_and_player_history() {
    let configuration = Configuration::original(1).unwrap();
    let mut original = Run::new(configuration.clone(), false).unwrap();
    original.start_ready_level();
    for _ in 0..21 {
        original.step(Some(0xcc)).unwrap();
    }
    original.step(Some(0x92)).unwrap();
    original.step(None).unwrap();
    let seed = original.game.attempt_seed();
    let mut replay = Run::new(configuration.clone(), false).unwrap();
    replay
        .game
        .restore_attempt(&seed, &configuration.levels[0].decode().unwrap())
        .unwrap();
    original.start_ready_level();
    replay.start_ready_level();
    for key in [Some(0xca), None, None, Some(0xd5), None, None] {
        original.step(key).unwrap();
        replay.step(key).unwrap();
        assert_eq!(original.game.snapshot(), replay.game.snapshot());
    }
}

#[test]
fn speed_changes_elapsed_gameplay_time_and_custom_size_snaps_both_ways() {
    let mut sector = [0; 256];
    sector[15 * 14 + 2] = (Tile::Player as u8) << 4;
    let configuration = Configuration {
        levels: vec![LevelRecord(sector)],
        start: 0,
        wait_on_level_start: false,
    };
    let duration = |period| {
        let mut run = Run::new(configuration.clone(), false).unwrap();
        run.game.sound.frame_period = period;
        for _ in 0..20 {
            run.step(None).unwrap();
        }
        run.game.clock.cycles
    };
    assert!(duration(0) < duration(6));
    assert!(duration(6) < duration(15));
    assert_eq!(snap_size([1120, 768], true), (1400, 960));
    assert_eq!(snap_size([1120, 768], false), (840, 576));
    assert_eq!(snap_size([1000, 685], true), (1120, 768));
    assert_eq!(snap_size([1000, 685], false), (840, 576));
    assert_eq!(snap_size([280, 192], false), (280, 192));
}
