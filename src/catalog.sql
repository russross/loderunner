PRAGMA foreign_keys = ON;
PRAGMA user_version = 1;

CREATE TABLE level_packs (
    pack_name TEXT PRIMARY KEY,
    pack_order INTEGER NOT NULL UNIQUE
) WITHOUT ROWID;

CREATE TABLE tile_kinds (
    tile_kind INTEGER PRIMARY KEY CHECK (tile_kind BETWEEN 0 AND 9),
    symbol TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE levels (
    pack_name TEXT NOT NULL REFERENCES level_packs,
    level_number INTEGER NOT NULL CHECK (level_number BETWEEN 1 AND 150),
    PRIMARY KEY (pack_name, level_number)
) WITHOUT ROWID;

CREATE TABLE level_tiles (
    pack_name TEXT NOT NULL,
    level_number INTEGER NOT NULL,
    tile_row INTEGER NOT NULL CHECK (tile_row BETWEEN 0 AND 15),
    tile_column INTEGER NOT NULL CHECK (tile_column BETWEEN 0 AND 27),
    tile_kind INTEGER NOT NULL REFERENCES tile_kinds,
    PRIMARY KEY (pack_name, level_number, tile_row, tile_column),
    FOREIGN KEY (pack_name, level_number) REFERENCES levels
) WITHOUT ROWID;
