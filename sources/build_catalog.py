#!/usr/bin/env python3
"""Build the normalized level catalog from the vendored Total Recall boards."""

import argparse
import json
from pathlib import Path
import re
import sqlite3
import sys
import tempfile


PACKS = (
    ("Classic Lode Runner", "classic.js", 150),
    ("Professional Lode Runner", "professional.js", 150),
    ("Revenge of Lode Runner", "revenge.js", 17),
    ("Lode Runner Fan Book", "fan-book.js", 66),
    ("Championship Lode Runner", "championship.js", 51),
)
SYMBOLS = " #@H-XS$0&"
DESCRIPTIONS = ("empty", "brick", "stone", "ladder", "rope", "trap", "hidden ladder", "gold", "guard", "player")


def boards(path: Path) -> list[str]:
    # These data-only arrays contain quoted rows joined with plus signs.
    text = re.sub(r"/\*.*?\*/|//[^\n]*", "", path.read_text(), flags=re.DOTALL)
    match = re.fullmatch(r"\s*var\s+\w+\s*=\s*\[(.*)\]\s*;?\s*", text, flags=re.DOTALL)
    if match is None:
        raise ValueError(f"{path}: expected a single level array")
    result: list[str] = []
    current = ""
    for token in re.findall(r'"(?:[^"\\]|\\.)*"|[^\s]', match[1]):
        if token.startswith('"'):
            row: str = json.loads(token)
            current += row
        elif token == ",":
            result.append(current)
            current = ""
        elif token != "+":
            raise ValueError(f"{path}: unexpected token {token!r}")
    if current:
        result.append(current)
    for number, board in enumerate(result, 1):
        if len(board) != 448 or any(tile not in SYMBOLS for tile in board) or "&" not in board:
            raise ValueError(f"{path}: invalid board {number}")
    return result


def build(destination: Path) -> None:
    root = Path(__file__).resolve().parent.parent
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temporary:
        path = Path(temporary) / "levels.sqlite3"
        with sqlite3.connect(path) as database:
            database.executescript((root / "src/catalog.sql").read_text())
            database.executescript((root / "src/recordings.sql").read_text())
            database.executemany("INSERT INTO tile_kinds VALUES (?, ?, ?)", ((i, symbol, DESCRIPTIONS[i]) for i, symbol in enumerate(SYMBOLS)))
            # A complete transaction is published only after every pack validates.
            for order, (name, filename, count) in enumerate(PACKS):
                levels = boards(root / "sources/levels" / filename)
                if len(levels) != count:
                    raise ValueError(f"{name}: expected {count} levels, found {len(levels)}")
                database.execute("INSERT INTO level_packs VALUES (?, ?)", (name, order))
                for number, board in enumerate(levels, 1):
                    database.execute("INSERT INTO levels VALUES (?, ?)", (name, number))
                    database.executemany("INSERT INTO level_tiles VALUES (?, ?, ?, ?, ?)", ((name, number, i // 28, i % 28, SYMBOLS.index(symbol)) for i, symbol in enumerate(board)))
            if database.execute("PRAGMA foreign_key_check").fetchall():
                raise ValueError("catalog foreign-key validation failed")
        path.replace(destination)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        build(args.output)
    except (OSError, ValueError, sqlite3.Error) as error:
        print(f"build_catalog: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
