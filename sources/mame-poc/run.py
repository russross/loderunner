#!/usr/bin/env python3
"""Exercise MAME's debugger interface without implementing a trace validator."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from enum import Enum
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from typing import TypeAlias

Json: TypeAlias = None | bool | int | str | list["Json"] | dict[str, "Json"]
ROOT = Path(__file__).resolve().parents[2]
REFERENCE = ROOT / "sources" / "lode_runner_reveng"
MACHINE = "apple2ee"
CPU = "w65c02"


class Mode(Enum):
    SYNTHETIC = "synthetic"
    GAME = "game"


@dataclass(frozen=True)
class Boundary:
    index: int
    cycles: int
    delta: int
    pc: int
    ram: bytes
    active: bytes
    background: bytes
    guards: bytes
    holes: bytes
    observed: tuple[int, ...]
    acknowledgements: int
    speaker_edges: int


# Parsed records become fixed models before assertions inspect behavior. The raw
# JSON envelope stays confined to the boundary between MAME and this probe.
def integer(record: dict[str, Json], name: str) -> int:
    value = record.get(name)
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ValueError(f"invalid {name} in MAME output")
    return value


def memory(record: dict[str, Json], name: str, size: int) -> bytes:
    value = record.get(name)
    if not isinstance(value, str):
        raise ValueError(f"missing {name} in MAME output")
    result = bytes.fromhex(value)
    if len(result) != size:
        raise ValueError(f"unexpected {name} snapshot size")
    return result


def parse_boundaries(path: Path, mode: Mode, rounds: int) -> tuple[str, list[Boundary]]:
    records: list[dict[str, Json]] = []
    for line in path.read_text().splitlines():
        record: Json = json.loads(line)
        if not isinstance(record, dict):
            raise ValueError("invalid MAME record")
        records.append(record)
    if not records or records[0].get("kind") != "header":
        raise ValueError("MAME did not emit a probe header")
    version = records[0].get("version")
    if (not isinstance(version, str) or records[0].get("mode") != mode.value
            or records[0].get("machine") != MACHINE or records[0].get("cpu") != CPU):
        raise ValueError("MAME probe identity mismatch")
    if records[-1].get("kind") != "complete":
        raise ValueError(f"MAME probe did not finish: {records[-1]}")
    if integer(records[-1], "rounds") != rounds or integer(records[-1], "tap_count") != 4:
        raise ValueError("MAME probe completion mismatch")

    boundaries: list[Boundary] = []
    for record in records[1:-1]:
        if record.get("kind") != "boundary":
            raise ValueError(f"unexpected MAME probe record: {record}")
        observed = record.get("observed")
        if not isinstance(observed, list) or any(
            not isinstance(key, int) or isinstance(key, bool) or not 128 <= key <= 255
            for key in observed
        ):
            raise ValueError("invalid observed keyboard codes")
        keys = tuple(key for key in observed if isinstance(key, int))
        boundaries.append(Boundary(
            integer(record, "index"), integer(record, "cycles"), integer(record, "delta"),
            integer(record, "pc"), memory(record, "ram", 256), memory(record, "active", 448),
            memory(record, "background", 448), memory(record, "guards", 64),
            memory(record, "holes", 96), keys, integer(record, "acknowledgements"),
            integer(record, "speaker_edges"),
        ))
    if len(boundaries) != rounds + 1:
        raise ValueError("MAME skipped or duplicated a boundary")
    for index, boundary in enumerate(boundaries):
        delta = boundary.cycles - boundaries[index - 1].cycles if index else 0
        if boundary.index != index or boundary.delta != delta or (index and delta <= 0):
            raise ValueError("MAME boundary numbering or cycle accounting differs")
    return version, boundaries


def first_level() -> bytes:
    data = bytearray()
    selected = False
    for line in (REFERENCE / "disk" / "track_03.asm").read_text().splitlines():
        if line.startswith("; Track $03, sector $"):
            selected = int(line.rsplit("$", 1)[1], 16) == 0
        elif selected and line.strip().startswith("HEX"):
            data.extend(bytes.fromhex(line.strip()[3:]))
    if len(data) != 256:
        raise ValueError("reference level sector is not 256 bytes")
    return bytes(data)


def deliveries(mode: Mode) -> list[dict[str, int]]:
    pairs = [(0, 0xcc), (2, 0xcc), (3, 0xa0)] if mode == Mode.SYNTHETIC else [(0, 0xcc), (10, 0xa0), (12, 0xd5)]
    return [{"round": index, "key": key} for index, key in pairs]


# Each launch gets separate configuration directories and output files. A host
# timeout bounds stalled debugger setup even while emulated time is stopped.
def launch(mame: str, work: Path, rompath: str | None, mode: Mode, run: int) -> tuple[str, list[Boundary]]:
    directory = work / MACHINE / f"{mode.value}-{run}"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "boundaries.jsonl"
    rounds = 8 if mode == Mode.SYNTHETIC else 20
    config = {
        "mode": mode.value, "rounds": rounds, "output": str(path),
        "image": str(REFERENCE / "main.bin"), "level": str(work / "level-1.bin"),
        "inputs": deliveries(mode),
    }
    config_path = directory / "config.json"
    config_path.write_text(json.dumps(config))
    path.unlink(missing_ok=True)
    args = [mame, MACHINE, "-video", "none", "-sound", "none", "-debug",
            "-debugger", "none", "-nothrottle", "-skip_gameinfo", "-seconds_to_run", "20",
            "-autoboot_script", str(Path(__file__).with_name("probe.lua")),
            "-cfg_directory", str(directory / "cfg"), "-nvram_directory", str(directory / "nvram"),
            "-diff_directory", str(directory / "diff"), "-state_directory", str(directory / "states")]
    if rompath is not None:
        args.extend(["-rompath", rompath])
    environment = dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy",
                       LODE_POC_CONFIG=str(config_path))
    log_path = directory / "mame.log"
    with log_path.open("w") as log:
        result = subprocess.run(args, cwd=directory, env=environment, stdout=log,
                                stderr=subprocess.STDOUT, timeout=30, check=False)
    log_text = log_path.read_text()
    if (result.returncode or "[LUA ERROR]" in log_text or "WRONG CHECKSUMS" in log_text
            or "WARNING:" in log_text or not path.exists()):
        raise ValueError(f"MAME failed ({result.returncode}); inspect {log_path}")
    return parse_boundaries(path, mode, rounds)


def verify(mode: Mode, boundaries: list[Boundary]) -> None:
    schedule = {entry["round"]: entry["key"] for entry in deliveries(mode)}
    pc = 0x1000 if mode == Mode.SYNTHETIC else 0x60e4
    for index, boundary in enumerate(boundaries):
        expected = (schedule[index - 1],) if index - 1 in schedule else ()
        if boundary.pc != pc or boundary.observed != expected:
            raise ValueError(f"{mode.value}: wrong boundary/input observation at {index}")
        # Continuing a dig bypasses CHECK_FOR_INPUT. The first dig call still
        # observes its command; the following seven intervals do not poll.
        expected_ack = 1 if mode == Mode.SYNTHETIC or index <= 13 else 0
        if index and boundary.acknowledgements != expected_ack:
            raise ValueError(f"{mode.value}: keyboard acknowledgement differs at {index}")
        if mode == Mode.SYNTHETIC and index:
            delivered = next(schedule[key] for key in sorted(schedule, reverse=True) if key < index)
            expected_key = delivered if expected else delivered & 0x7f
            if boundary.delta != 23 or boundary.ram[0x20] != index or boundary.ram[0x21] != expected_key:
                raise ValueError(f"synthetic: instruction effects/timing differ at {index}")
            if boundary.speaker_edges != 1:
                raise ValueError(f"synthetic: speaker read missing at {index}")
    if mode == Mode.GAME:
        start, moved, stopped = boundaries[0], boundaries[10], boundaries[-1]
        if start.ram[:4] != bytes([14, 14, 2, 2]) or start.ram[0x8d] != 3 or start.ram[0x93] != 6:
            raise ValueError("original game initialization differs from level 1")
        if moved.ram[:4] != bytes([16, 14, 2, 2]) or moved.ram[:4] != stopped.ram[:4]:
            raise ValueError("original movement/stop did not take effect")
        if stopped.ram[0x9c] != 0xff or stopped.ram[0xa0] != 8:
            raise ValueError("original left dig did not advance through eight updates")
        if start.guards == stopped.guards or not any(item.speaker_edges for item in boundaries):
            raise ValueError("original guards/sound did not execute")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mame", default=shutil.which("mame") or "/usr/games/mame")
    parser.add_argument("--work", type=Path, default=ROOT / "artifacts" / "mame-poc")
    parser.add_argument("--rompath", help="MAME ROM search path; otherwise use MAME configuration")
    options = parser.parse_args()
    try:
        work = options.work.resolve()
        work.mkdir(parents=True, exist_ok=True)
        (work / "level-1.bin").write_bytes(first_level())
        for mode in Mode:
            first = launch(options.mame, work, options.rompath, mode, 1)
            second = launch(options.mame, work, options.rompath, mode, 2)
            if first != second:
                raise ValueError(f"{mode.value}: repeated runs differ")
            version, boundaries = first
            verify(mode, boundaries)
            elapsed = boundaries[-1].cycles - boundaries[0].cycles
            print(f"MAME {version} {MACHINE}/{CPU}: {mode.value}, {len(boundaries) - 1} intervals, {elapsed} cycles, identical repeated snapshots")
        return 0
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"mame-poc: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
