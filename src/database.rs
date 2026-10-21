//! Catalog lookup and transactional, per-attempt player history.

use crate::recording::{AttemptSeed, Recording};
use crate::session::Configuration;
use crate::storage::LevelRecord;
use crate::timing::NTSC_CPU_HZ;
use lode_trace::Input;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;

pub fn timestamp() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_micros(),
    )
    .map_err(|error| error.to_string())
}

fn check_version(connection: &Connection) -> Result<()> {
    let version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if version != 1 {
        return Err(format!("unsupported database version {version}"));
    }
    Ok(())
}

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| {
                format!(
                    "{}: {error}; run make install to install the level catalog",
                    path.display()
                )
            })?;
        check_version(&connection)?;
        Ok(Self { connection })
    }

    pub fn packs(&self) -> Result<Vec<String>> {
        let mut query = self
            .connection
            .prepare("SELECT pack_name FROM level_packs ORDER BY pack_order")
            .map_err(|error| error.to_string())?;
        query
            .query_map([], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    // Ordered cells are packed only at the existing validated-board interface.
    pub fn configuration(&self, pack: &str, level: u8) -> Result<Configuration> {
        let mut query = self.connection.prepare("SELECT level_number, tile_row, tile_column, tile_kind FROM level_tiles WHERE pack_name=? ORDER BY level_number, tile_row, tile_column").map_err(|error| error.to_string())?;
        let cells = query
            .query_map([pack], |row| {
                Ok((
                    usize::from(row.get::<_, u8>(0)?),
                    usize::from(row.get::<_, u8>(1)?),
                    usize::from(row.get::<_, u8>(2)?),
                    row.get::<_, u8>(3)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut levels: Vec<LevelRecord> = Vec::new();
        let mut count = 0;
        for cell in cells {
            let (number, row, column, tile) = cell.map_err(|error| error.to_string())?;
            if number != count / 448 + 1 || row * 28 + column != count % 448 || tile > 9 {
                return Err(format!("{pack}: invalid level cells"));
            }
            if count % 448 == 0 {
                levels.push(LevelRecord([0; 256]));
            }
            let index = row * 28 + column;
            levels[number - 1].0[index / 2] |= tile << ((index % 2) * 4);
            count += 1;
        }
        if count == 0 || count % 448 != 0 || level == 0 || usize::from(level) > levels.len() {
            return Err(format!("{pack}: invalid starting level {level}"));
        }
        for record in &levels {
            record.decode()?;
        }
        Ok(Configuration {
            levels,
            start: usize::from(level - 1),
            wait_on_level_start: true,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Death,
    Restart,
    Abandoned,
    Quit,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Death => "death",
            Self::Restart => "restart",
            Self::Abandoned => "abandoned",
            Self::Quit => "quit",
        }
    }
}

pub struct Attempt {
    pub started_at: i64,
    pub session_started_at: i64,
    pub pack: String,
    pub level: u8,
    pub seed: AttemptSeed,
    pub recording: Recording,
    pub elapsed_cycles: u64,
    pub gold: u32,
    pub guard_deaths: u32,
}

pub struct PlayerStore {
    connection: Connection,
}

impl PlayerStore {
    pub fn open(path: &Path) -> Result<Self> {
        fs::create_dir_all(path.parent().ok_or("player database has no parent")?)
            .map_err(|error| error.to_string())?;
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .map_err(|error| error.to_string())?;
        let version: i32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if version == 0 {
            connection
                .execute_batch(&format!(
                    "BEGIN IMMEDIATE; {} PRAGMA user_version=1; COMMIT;",
                    include_str!("recordings.sql")
                ))
                .map_err(|error| error.to_string())?;
        }
        check_version(&connection)?;
        Ok(Self { connection })
    }

    // Header, inputs, and pruning commit together so interruption cannot leave
    // partial recordings or dangling input rows visible to subsequent readers.
    pub fn save(&mut self, attempt: &Attempt, outcome: Outcome, now: i64) -> Result<()> {
        if attempt.recording.rounds == 0 {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let started_at: i64 = transaction.query_row("SELECT MAX(?, COALESCE(MAX(started_at)+1, ?)) FROM recordings WHERE pack_name=? AND level_number=?", params![attempt.started_at, attempt.started_at, attempt.pack, attempt.level], |row| row.get(0)).map_err(|error| error.to_string())?;
        let seed = serde_json::to_string(&attempt.seed).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO recordings VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    attempt.pack,
                    attempt.level,
                    started_at,
                    attempt.session_started_at,
                    outcome.name(),
                    i64::try_from(attempt.elapsed_cycles).map_err(|error| error.to_string())?,
                    i64::try_from(attempt.recording.rounds).map_err(|error| error.to_string())?,
                    attempt.gold,
                    attempt.guard_deaths,
                    seed
                ],
            )
            .map_err(|error| error.to_string())?;
        for round in 0..attempt.recording.rounds {
            if let Some(key) = attempt.recording.key(round) {
                transaction
                    .execute(
                        "INSERT INTO recording_inputs VALUES (?, ?, ?, ?, ?)",
                        params![
                            attempt.pack,
                            attempt.level,
                            started_at,
                            i64::try_from(round).map_err(|error| error.to_string())?,
                            key
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        // Keep the personal best and newest win permanently. Recent useful runs
        // survive thirty days; the best useful run per session survives ninety.
        transaction
            .execute(
                include_str!("prune.sql"),
                params![attempt.pack, attempt.level, now, (NTSC_CPU_HZ * 10) as i64],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy)]
enum Origin {
    Catalog,
    Player,
}

pub struct Demo {
    pub number: i32,
    started_at: i64,
    origin: Origin,
}

pub struct DemoPlayback {
    pub seed: AttemptSeed,
    pub recording: Recording,
}

// Logical numbers are a view of durable timestamps, never stored identities.
// A personal-best fallback occupies zero once, without duplicating that replay.
pub fn demos(catalog: &Catalog, player: &PlayerStore, pack: &str, level: u8) -> Result<Vec<Demo>> {
    let supplied: Option<i64> = catalog.connection.query_row("SELECT started_at FROM recordings WHERE pack_name=? AND level_number=? AND outcome='win' ORDER BY elapsed_cycles, started_at DESC LIMIT 1", params![pack, level], |row| row.get(0)).optional().map_err(|error| error.to_string())?;
    let mut query = player.connection.prepare("SELECT started_at, outcome, elapsed_cycles FROM recordings WHERE pack_name=? AND level_number=? ORDER BY started_at DESC").map_err(|error| error.to_string())?;
    let rows = query
        .query_map(params![pack, level], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let best = rows
        .iter()
        .filter(|row| row.1 == "win")
        .min_by_key(|row| (row.2, std::cmp::Reverse(row.0)))
        .map(|row| row.0);
    let mut result = Vec::new();
    if let Some(started_at) = supplied {
        result.push(Demo {
            number: 0,
            started_at,
            origin: Origin::Catalog,
        });
    }
    if let Some(started_at) = best {
        result.push(Demo {
            number: if supplied.is_some() { 1 } else { 0 },
            started_at,
            origin: Origin::Player,
        });
    }
    let mut win = if supplied.is_some() { 2 } else { 1 };
    let mut fail = -1;
    for (started_at, outcome, _) in rows {
        if Some(started_at) == best {
            continue;
        }
        let number = if outcome == "win" {
            let number = win;
            win += 1;
            number
        } else {
            let number = fail;
            fail -= 1;
            number
        };
        result.push(Demo {
            number,
            started_at,
            origin: Origin::Player,
        });
    }
    result.sort_by_key(|demo| demo.number);
    Ok(result)
}

pub fn load_demo(
    catalog: &Catalog,
    player: &PlayerStore,
    pack: &str,
    level: u8,
    demo: &Demo,
) -> Result<DemoPlayback> {
    let connection = match demo.origin {
        Origin::Catalog => &catalog.connection,
        Origin::Player => &player.connection,
    };
    let (rounds, text): (i64, String) = connection.query_row("SELECT rounds, initial_state FROM recordings WHERE pack_name=? AND level_number=? AND started_at=?", params![pack, level, demo.started_at], |row| Ok((row.get(0)?, row.get(1)?))).map_err(|error| error.to_string())?;
    let rounds = usize::try_from(rounds).map_err(|error| error.to_string())?;
    let seed = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let configuration = catalog.configuration(pack, level)?;
    let mut recording = Recording::new(configuration);
    let mut query = connection.prepare("SELECT round_number, key_code FROM recording_inputs WHERE pack_name=? AND level_number=? AND started_at=? ORDER BY round_number").map_err(|error| error.to_string())?;
    let inputs: BTreeMap<i64, u8> = query
        .query_map(params![pack, level, demo.started_at], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| error.to_string())?;
    if inputs
        .keys()
        .any(|&round| round < 0 || round as u64 >= rounds as u64)
    {
        return Err("input outside database recording".into());
    }
    for round in 0..rounds {
        let input = inputs.get(&(round as i64)).map(|&key| Input {
            observation: 0,
            key,
        });
        recording.push(input.as_slice())?;
    }
    Ok(DemoPlayback { seed, recording })
}
