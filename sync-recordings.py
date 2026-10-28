#!/usr/bin/env python3
"""Merge Lode Runner player recordings over SSH using only the standard library."""

import argparse
import json
import os
import shlex
import sqlite3
import subprocess
import sys
from dataclasses import astuple, dataclass
from pathlib import Path
from typing import IO


@dataclass(frozen=True)
class Header:
    pack_name: str
    level_number: int
    started_at: int
    session_started_at: int
    outcome: str
    elapsed_cycles: int
    rounds: int
    gold_collected: int
    guard_deaths: int
    initial_state: str

    @property
    def key(self) -> tuple[str, int, int]:
        return self.pack_name, self.level_number, self.started_at

    def matches(self, other: "Header") -> bool:
        return astuple(self)[:-1] == astuple(other)[:-1] and json.loads(
            self.initial_state
        ) == json.loads(other.initial_state)


COLUMNS = ", ".join(Header.__dataclass_fields__)
Key = tuple[str, int, int]
Bundle = list[tuple[Header, list[tuple[int, int]]]]


@dataclass(frozen=True)
class Preferences:
    pack: str
    levels: dict[str, int]
    frame_period: int
    demo_frame_period: int

    @classmethod
    def read(cls, path: Path) -> "Preferences":
        values = read_config(path)
        result = cls(
            values.get("pack", "Classic Lode Runner"),
            values.get("levels", {}),
            values.get("frame_period", 6),
            values.get("demo_frame_period", 6),
        )
        if (
            not isinstance(result.pack, str)
            or not isinstance(result.levels, dict)
            or any(
                type(v) is not int or not 1 <= v <= 150 for v in result.levels.values()
            )
            or any(
                type(v) is not int or not 0 <= v <= 15
                for v in (result.frame_period, result.demo_frame_period)
            )
        ):
            raise ValueError(f"{path}: invalid sync preferences")
        return result

    def save(self, path: Path) -> None:
        # Preserve destination display, sound and movement preferences.
        values = read_config(path)
        values.update(zip(self.__dataclass_fields__, astuple(self)))
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(f"config.{os.getpid()}.tmp")
        try:
            temporary.write_text(json.dumps(values, indent=2) + "\n")
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def read_config(path: Path) -> dict:
    try:
        values = json.loads(path.read_text())
    except FileNotFoundError:
        return {}
    if not isinstance(values, dict):
        raise TypeError(f"{path}: configuration must be an object")
    return values


def config_path(override: str | None) -> Path:
    if override is not None:
        return Path(override).expanduser().resolve()
    directory = Path(os.environ.get("XDG_CONFIG_HOME", ""))
    if not directory.is_absolute():
        directory = Path.home() / ".config"
    return directory / "loderunner/config.json"


def database_path(override: str | None) -> Path:
    if override is not None:
        return Path(override).expanduser().resolve()
    data = Path(os.environ.get("XDG_DATA_HOME", ""))
    if not data.is_absolute():
        data = Path.home() / ".local/share"
    return data / "loderunner/player.sqlite3"


def open_database(path: Path) -> sqlite3.Connection:
    # Read/write mode refuses to create an empty database at a mistaken path.
    connection = sqlite3.connect(path.as_uri() + "?mode=rw", uri=True, timeout=5)
    try:
        connection.execute("PRAGMA foreign_keys=ON")
        version = connection.execute("PRAGMA user_version").fetchone()[0]
        if version != 1:
            raise ValueError(f"{path}: unsupported database version {version}")
        connection.execute(f"SELECT {COLUMNS} FROM recordings LIMIT 0")
        connection.execute(
            "SELECT pack_name, level_number, started_at, round_number, key_code FROM recording_inputs LIMIT 0"
        )
    except Exception:
        connection.close()
        raise
    return connection


def manifest(connection: sqlite3.Connection) -> dict[Key, Header]:
    # Header and input reads share one snapshot, even while the game saves.
    connection.execute("BEGIN")
    return {
        header.key: header
        for header in (
            Header(*row)
            for row in connection.execute(f"SELECT {COLUMNS} FROM recordings")
        )
    }


def export(
    connection: sqlite3.Connection, headers: dict[Key, Header], keys: list[Key]
) -> Bundle:
    result: Bundle = []
    for key in keys:
        inputs = connection.execute(
            "SELECT round_number, key_code FROM recording_inputs "
            "WHERE pack_name=? AND level_number=? AND started_at=? ORDER BY round_number",
            key,
        ).fetchall()
        result.append((headers[key], inputs))
    return result


def merge(connection: sqlite3.Connection, bundle: Bundle) -> int:
    connection.execute("BEGIN IMMEDIATE")
    inserted = 0
    try:
        for header, inputs in bundle:
            # A concurrent sync may already have inserted this natural identity.
            row = connection.execute(
                f"SELECT {COLUMNS} FROM recordings WHERE pack_name=? AND level_number=? AND started_at=?",
                header.key,
            ).fetchone()
            if row is not None:
                if not header.matches(Header(*row)):
                    raise ValueError(f"conflicting recording metadata: {header.key!r}")
                continue
            connection.execute(
                f"INSERT INTO recordings ({COLUMNS}) VALUES ({','.join('?' for _ in astuple(header))})",
                astuple(header),
            )
            for round_number, key_code in inputs:
                if not 0 <= round_number < header.rounds:
                    raise ValueError(f"input outside recording: {header.key!r}")
                connection.execute(
                    "INSERT INTO recording_inputs (pack_name, level_number, started_at, round_number, key_code) VALUES (?, ?, ?, ?, ?)",
                    (*header.key, round_number, key_code),
                )
            inserted += 1
        connection.commit()
    except Exception:
        connection.rollback()
        raise
    return inserted


def send(stream: IO[str], value: object) -> None:
    stream.write(json.dumps(value, separators=(",", ":")) + "\n")
    stream.flush()


def receive(stream: IO[str]):
    line = stream.readline()
    if not line:
        raise ValueError("SSH connection closed before sync completed")
    return json.loads(line)


def encode(bundle: Bundle) -> list:
    return [[astuple(header), inputs] for header, inputs in bundle]


def decode(value: list) -> Bundle:
    return [
        (
            Header(*header),
            [(round_number, key_code) for round_number, key_code in inputs],
        )
        for header, inputs in value
    ]


def remote(connection: sqlite3.Connection, config: Path) -> None:
    headers = manifest(connection)
    send(
        sys.stdout,
        [
            [astuple(header) for header in headers.values()],
            astuple(Preferences.read(config)),
        ],
    )
    keys = [tuple(key) for key in receive(sys.stdin)]
    bundle = export(connection, headers, keys)
    connection.rollback()
    send(sys.stdout, encode(bundle))
    inserted = merge(connection, decode(receive(sys.stdin)))
    send(sys.stdout, inserted)
    preferences = receive(sys.stdin)
    if preferences is not None:
        Preferences(*preferences).save(config)
    send(sys.stdout, True)


def synchronize(
    connection: sqlite3.Connection,
    target: str,
    remote_db: str | None,
    config: Path,
    remote_config: str | None,
) -> None:
    # The helper travels inside the SSH command; stdin remains a JSON channel.
    source = Path(__file__).read_text()
    command = "python3 -c " + shlex.quote(source) + " --_remote"
    if remote_db is not None:
        command += " --remote-db " + shlex.quote(remote_db)
    if remote_config is not None:
        command += " --remote-config " + shlex.quote(remote_config)
    with subprocess.Popen(
        ["ssh", "--", target, command],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    ) as process:
        assert process.stdin is not None and process.stdout is not None
        try:
            local = manifest(connection)
            preferences = Preferences.read(config)
            distant_headers, distant_preferences = receive(process.stdout)
            distant = {
                header.key: header
                for header in (Header(*row) for row in distant_headers)
            }
            # Compare the original snapshots before imports equalize timestamps.
            local_latest = max((h.started_at for h in local.values()), default=None)
            remote_latest = max((h.started_at for h in distant.values()), default=None)
            newer_local = local_latest is not None and (
                remote_latest is None or local_latest > remote_latest
            )
            newer_remote = remote_latest is not None and (
                local_latest is None or remote_latest > local_latest
            )
            for key in local.keys() & distant.keys():
                if not local[key].matches(distant[key]):
                    raise ValueError(f"conflicting recording metadata: {key!r}")
            outgoing = export(connection, local, sorted(local.keys() - distant.keys()))
            connection.rollback()
            send(process.stdin, sorted(distant.keys() - local.keys()))
            incoming = decode(receive(process.stdout))
            pulled = merge(connection, incoming)
            print(f"Local: committed {pulled} recording(s) from {target}.", flush=True)
            send(process.stdin, encode(outgoing))
            pushed = receive(process.stdout)
            send(process.stdin, astuple(preferences) if newer_local else None)
            receive(process.stdout)
            process.stdin.close()
            status = process.wait()
            if status:
                raise ValueError(f"SSH exited with status {status}")
            print(f"Remote: committed {pushed} recording(s) to {target}.")
            if newer_remote:
                Preferences(*distant_preferences).save(config)
                print(f"Local: updated level and speeds from {target}.")
            elif newer_local:
                print(f"Remote: updated level and speeds from local host to {target}.")
        except BaseException:
            # Closing stdin releases a helper waiting for the next request.
            process.stdin.close()
            if process.poll() is None:
                process.terminate()
            process.wait()
            raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", nargs="?", help="SSH Host alias or user@hostname")
    parser.add_argument("--local-db", help="local player database path")
    parser.add_argument("--remote-db", help="remote player database path")
    parser.add_argument("--local-config", help="local configuration path")
    parser.add_argument("--remote-config", help="remote configuration path")
    parser.add_argument("--_remote", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not args._remote and (not args.target or args.target.startswith("-")):
        parser.error("an SSH target is required and must not start with '-' ")
    connection = None
    try:
        connection = open_database(
            database_path(args.remote_db if args._remote else args.local_db)
        )
        if args._remote:
            remote(connection, config_path(args.remote_config))
        else:
            synchronize(
                connection,
                args.target,
                args.remote_db,
                config_path(args.local_config),
                args.remote_config,
            )
        return 0
    except (OSError, sqlite3.Error, ValueError, TypeError, KeyError) as error:
        print(f"sync-recordings: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("sync-recordings: interrupted", file=sys.stderr)
        return 130
    finally:
        if connection is not None:
            connection.close()


if __name__ == "__main__":
    sys.exit(main())
