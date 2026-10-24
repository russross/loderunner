#!/usr/bin/env python3
"""Exercise the sync CLI with isolated databases and a local SSH transport."""

import importlib.util
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from contextlib import closing
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "sync-recordings.py"
spec = importlib.util.spec_from_file_location("sync_recordings", SCRIPT)
assert spec is not None and spec.loader is not None
sync = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = sync
spec.loader.exec_module(sync)


class SyncTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.local = self.directory / "local.sqlite3"
        self.remote = self.directory / "remote 'quoted' $(false).sqlite3"
        for path in (self.local, self.remote):
            with closing(sqlite3.connect(path)) as connection, connection:
                connection.executescript((ROOT / "src/recordings.sql").read_text())
                connection.execute("PRAGMA user_version=1")
        # Run the exact remote shell command without needing a local SSH daemon.
        ssh = self.directory / "ssh"
        ssh.write_text(
            '#!/bin/sh\n[ "$1" = "--" ] || exit 90\n[ "$2" = "test-host" ] || exit 91\nexec /bin/sh -c "$3"\n'
        )
        ssh.chmod(0o755)
        self.environment = dict(
            os.environ, PATH=f"{self.directory}:{os.environ['PATH']}"
        )

    def add(self, path: Path, timestamp: int, rounds: int = 3) -> None:
        with closing(sqlite3.connect(path)) as connection, connection:
            connection.execute(
                "INSERT INTO recordings VALUES (?, 1, ?, 1, 'win', 100, ?, 2, 0, ?)",
                ("Classic Lode Runner", timestamp, rounds, json.dumps({"score": 0})),
            )
            connection.execute(
                "INSERT INTO recording_inputs VALUES (?, 1, ?, 0, 204)",
                ("Classic Lode Runner", timestamp),
            )

    def run_sync(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "test-host",
                "--local-db",
                str(self.local),
                "--remote-db",
                str(self.remote),
            ],
            env=self.environment,
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def rows(self, path: Path) -> tuple[list, list]:
        with closing(sqlite3.connect(path)) as connection, connection:
            return (
                connection.execute(
                    "SELECT * FROM recordings ORDER BY started_at"
                ).fetchall(),
                connection.execute(
                    "SELECT * FROM recording_inputs ORDER BY started_at"
                ).fetchall(),
            )

    def test_merge_and_repeat(self) -> None:
        self.add(self.local, 1)
        self.add(self.local, 2)
        self.add(self.remote, 2)
        self.add(self.remote, 3)
        result = self.run_sync()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.rows(self.local), self.rows(self.remote))
        self.assertEqual(len(self.rows(self.local)[0]), 3)
        result = self.run_sync()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count("committed 0"), 2)

    def test_conflict_leaves_both_databases_unchanged(self) -> None:
        self.add(self.local, 1)
        self.add(self.remote, 1, rounds=4)
        self.add(self.remote, 2)
        before = self.rows(self.local), self.rows(self.remote)
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("conflicting recording metadata", result.stderr)
        self.assertEqual((self.rows(self.local), self.rows(self.remote)), before)

    def test_inputs_are_not_compared(self) -> None:
        self.add(self.local, 1)
        self.add(self.remote, 1)
        with closing(sqlite3.connect(self.remote)) as connection, connection:
            connection.execute("UPDATE recording_inputs SET key_code=202")
            connection.execute(
                "UPDATE recordings SET initial_state=' { \"score\" : 0 } '"
            )
        result = self.run_sync()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(self.rows(self.local)[1], self.rows(self.remote)[1])

    def test_remote_failure_preserves_completed_local_import(self) -> None:
        self.add(self.local, 1)
        self.add(self.remote, 2)
        with closing(sqlite3.connect(self.remote)) as connection, connection:
            connection.execute(
                "CREATE TRIGGER refuse_import BEFORE INSERT ON recordings BEGIN SELECT RAISE(ABORT, 'test rejection'); END"
            )
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Local: committed 1", result.stdout)
        self.assertIn("test rejection", result.stderr)
        self.assertEqual(len(self.rows(self.local)[0]), 2)
        self.assertEqual(len(self.rows(self.remote)[0]), 1)
        with closing(sqlite3.connect(self.remote)) as connection, connection:
            connection.execute("DROP TRIGGER refuse_import")
        result = self.run_sync()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.rows(self.local), self.rows(self.remote))

    def test_merge_rolls_back_entire_batch(self) -> None:
        self.add(self.remote, 1)
        self.add(self.remote, 2)
        with closing(sqlite3.connect(self.remote)) as connection, connection:
            connection.execute(
                "UPDATE recording_inputs SET round_number=3 WHERE started_at=2"
            )
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("input outside recording", result.stderr)
        self.assertEqual(self.rows(self.local), ([], []))

    def test_missing_and_unsupported_databases(self) -> None:
        self.local.unlink()
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.local.exists())
        with closing(sqlite3.connect(self.local)) as connection, connection:
            connection.execute("PRAGMA user_version=2")
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported database version 2", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_ssh_failure(self) -> None:
        (self.directory / "ssh").write_text(
            "#!/bin/sh\necho 'test connection failure' >&2\nexit 255\n"
        )
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("test connection failure", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_concurrent_insert_is_checked(self) -> None:
        self.add(self.local, 1)
        connection = sync.open_database(self.local)
        self.addCleanup(connection.close)
        header = sync.Header(
            "Classic Lode Runner", 1, 1, 1, "win", 100, 3, 2, 0, '{"score":0}'
        )
        self.assertEqual(sync.merge(connection, [(header, [])]), 0)
        conflict = sync.Header(
            "Classic Lode Runner", 1, 1, 1, "win", 101, 3, 2, 0, '{"score":0}'
        )
        with self.assertRaisesRegex(ValueError, "conflicting recording metadata"):
            sync.merge(connection, [(conflict, [])])

    def test_snapshot_keeps_inputs_after_concurrent_pruning(self) -> None:
        self.add(self.local, 1)
        connection = sync.open_database(self.local)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        headers = sync.manifest(connection)
        with closing(sqlite3.connect(self.local)) as writer, writer:
            writer.execute("PRAGMA foreign_keys=ON")
            writer.execute("DELETE FROM recordings")
        bundle = sync.export(connection, headers, list(headers))
        connection.rollback()
        self.assertEqual(bundle[0][1], [(0, 204)])
        self.assertEqual(self.rows(self.local), ([], []))

    def test_paths_use_absolute_xdg_values(self) -> None:
        with patch.dict(os.environ, {"XDG_DATA_HOME": str(self.directory)}):
            self.assertEqual(
                sync.database_path(None), self.directory / "loderunner/player.sqlite3"
            )
        with patch.dict(os.environ, {"XDG_DATA_HOME": "relative"}):
            self.assertEqual(
                sync.database_path(None),
                Path.home() / ".local/share/loderunner/player.sqlite3",
            )

    def test_missing_remote_database_does_not_import(self) -> None:
        self.add(self.local, 1)
        self.remote.unlink()
        result = self.run_sync()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.remote.exists())
        self.assertEqual(len(self.rows(self.local)[0]), 1)
        self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
