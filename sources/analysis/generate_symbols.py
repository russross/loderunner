#!/usr/bin/env python3
"""Locate reference labels using the assembled image's documented opcode lengths."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re

REFERENCE = Path(__file__).resolve().parents[1] / "lode_runner_reveng"
ONE = set(bytes.fromhex("00 08 0a 18 28 2a 38 40 48 4a 58 60 68 6a 78 88 8a 98 9a a8 aa b8 ba c8 ca d8 e8 ea f8"))
TWO = set(bytes.fromhex("01 05 06 09 10 11 15 16 21 24 25 26 29 30 31 35 36 41 45 46 49 50 51 55 56 61 65 66 69 70 71 75 76 81 84 85 86 90 91 94 95 96 a0 a1 a2 a4 a5 a6 a9 b0 b1 b4 b5 b6 c0 c1 c4 c5 c6 c9 d0 d1 d5 d6 e0 e1 e4 e5 e6 e9 f0 f1 f5 f6"))
THREE = set(bytes.fromhex("0d 0e 19 1d 1e 20 2c 2d 2e 39 3d 3e 4c 4d 4e 59 5d 5e 6c 6d 6e 79 7d 7e 8c 8d 8e 99 9d ac ad ae b9 bc bd be cc cd ce d9 dd de ec ed ee f9 fd fe"))
MNEMONICS = set("ADC AND ASL BCC BCS BEQ BIT BMI BNE BPL BRK BVC BVS CLC CLD CLI CLV CMP CPX CPY DEC DEX DEY EOR INC INX INY JMP JSR LDA LDX LDY LSR NOP ORA PHA PHP PLA PLP ROL ROR RTI RTS SBC SEC SED SEI STA STX STY TAX TAY TSX TXA TXS TYA".split())


def symbols() -> dict[str, int]:
    image = (REFERENCE / "main.bin").read_bytes()
    result: dict[str, int] = {}
    address, scope = 0, ""

    # Data includes advance the same address cursor as assembly directives. Local
    # labels are qualified by their nearest global label for stable lookup names.
    def visit(path: Path) -> None:
        nonlocal address, scope
        for number, raw in enumerate(path.read_text().splitlines(), 1):
            line = raw.split(";", 1)[0].strip()
            if not line:
                continue
            words = line.split()
            if len(words) >= 3 and words[1] == "EQU":
                continue
            head = words[0].rstrip(":")
            if head == "ORG":
                address = int(words[1].replace("$", "0x"), 0)
            elif head == "INCLUDE":
                visit(path.parent / words[1].strip('"'))
            elif head == "HEX":
                address += len(bytes.fromhex("".join(words[1:])))
            elif head in ("BYTE", "WORD"):
                address += (2 if head == "WORD" else 1) * len(line.split(None, 1)[1].split(","))
            elif head == "DS":
                address += int(words[1].replace("$", "0x"), 0)
            elif head in ("PROCESSOR", "SUBROUTINE"):
                continue
            elif head in MNEMONICS:
                opcode = image[address]
                length = 1 if opcode in ONE else 2 if opcode in TWO else 3 if opcode in THREE else 0
                if not length:
                    raise ValueError(f"{path.name}:{number}: unsupported opcode {opcode:02x} at {address:04x}")
                address += length
            else:
                if not re.fullmatch(r"[.A-Za-z_][.A-Za-z_0-9]*", head) or len(words) != 1:
                    raise ValueError(f"{path.name}:{number}: unsupported source line {line}")
                if head.startswith("."):
                    result[scope + head] = address
                else:
                    scope = head
                    result[head] = address
    visit(REFERENCE / "main.asm")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    options = parser.parse_args()
    options.output.write_text(json.dumps(symbols(), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
