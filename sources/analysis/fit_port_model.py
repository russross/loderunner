#!/usr/bin/env python3
"""Fit native pacing from whole-operation counts, without branch probes."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from fit_event_model import fit, metrics


# These features describe rounds, visible operations and search extents. They
# deliberately exclude branch outcomes, neighbor probes and instruction paths.
FEATURES = [
    "round", "hole_scan", "death", "completion", "player_attempt", "guard_update",
    "dig", "score", "active_hole", "input", "same_row_scan", "limit_scan",
    "route_candidate", "above_scan", "below_scan", "tile_draw", "actor_draw",
    "sprite_erase",
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("measurements", type=Path)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    raw = args.measurements.read_bytes()
    rows = [row for row in json.loads(raw)
            if row["native"]["phase"] == "Playing"
            and row["native"]["end_phase"] in ["Playing", "Died", "LevelCleared"]]
    sets = {split: [row for row in rows if row["split"] == split]
            for split in ["train", "validation", "test"]}
    if not all(sets.values()):
        raise ValueError("training, validation and test scenarios are required")

    # Fit training levels and choose regularization on validation levels only.
    # Test levels report the frozen choice; they never select its coefficients.
    trials = [(fit(sets["train"], FEATURES, ridge), ridge)
              for ridge in [1e-6, 1e-4, 1e-2, 0.1]]
    weights, ridge = min(trials, key=lambda trial: metrics(sets["validation"], trial[0])["rmse"])
    report = {
        "measurement_sha256": hashlib.sha256(raw).hexdigest(),
        "features": FEATURES,
        "ridge": ridge,
        "selection": "fixed whole-operation features; minimum validation RMSE selects ridge",
        "metrics": {split: {"legacy": metrics(values), "model": metrics(values, weights)}
                    for split, values in sets.items()},
    }
    # Menu polling and pause entry are measured separately from gameplay. Sound
    # countdowns and graphics placement retain the fixed calibrated terms.
    weights.update(poll=5000.0, pause=187.0 - weights["round"] - weights["input"])
    args.model.write_text(json.dumps({"coefficients": weights}, indent=2, sort_keys=True) + "\n")
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["metrics"], indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, ZeroDivisionError) as error:
        print(f"port timing model: {error}", file=sys.stderr)
        sys.exit(1)
