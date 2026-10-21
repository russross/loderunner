CREATE TABLE recordings (
    pack_name TEXT NOT NULL,
    level_number INTEGER NOT NULL CHECK (level_number BETWEEN 1 AND 150),
    started_at INTEGER NOT NULL,
    session_started_at INTEGER NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('win', 'death', 'restart', 'abandoned', 'quit')),
    elapsed_cycles INTEGER NOT NULL CHECK (elapsed_cycles >= 0),
    rounds INTEGER NOT NULL CHECK (rounds > 0),
    gold_collected INTEGER NOT NULL CHECK (gold_collected >= 0),
    guard_deaths INTEGER NOT NULL CHECK (guard_deaths >= 0),
    initial_state TEXT NOT NULL CHECK (json_valid(initial_state)),
    PRIMARY KEY (pack_name, level_number, started_at)
) WITHOUT ROWID;

CREATE TABLE recording_inputs (
    pack_name TEXT NOT NULL,
    level_number INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    round_number INTEGER NOT NULL CHECK (round_number >= 0),
    key_code INTEGER NOT NULL CHECK (key_code BETWEEN 128 AND 255),
    PRIMARY KEY (pack_name, level_number, started_at, round_number),
    FOREIGN KEY (pack_name, level_number, started_at)
        REFERENCES recordings ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX recording_sessions ON recordings (session_started_at);
