#!/usr/bin/env python3
"""Fit a linear event model with whole-scenario validation and untouched tests."""
import argparse
import hashlib
from collections import Counter
import json
import math
from pathlib import Path
import sys


FIXED = {"graphics_cycles": 1.0, "sound_cycles": 1.0}
BASE = ["round", "hole_scan", "death", "completion", "player_attempt", "guard_update", "dig", "score", "active_hole", "input"]
GROUPS = [
    ["distance_same_row", "distance_above", "distance_below", "better_route", "below_player_probe", "above_player_probe"],
    ["below_non_empty", "below_left_probe", "below_right_probe", "above_left_probe", "above_right_probe", "route_down", "route_up", "horizontal_search", "same_row_support"],
    ["same_row_scan", "limit_scan", "route_candidate", "above_scan", "below_scan"],
    ["player_fall", "guard_fall", "guard_route", "guard_attempt"],
    ["tile_draw", "actor_draw", "sprite_erase"],
    ["player_animation", "player_draw", "gold_check"],
    ["sound_section", "note"],
    ["exit", "active_resurrection"],
]


def events(row):
    return row["native"]["events"]


def predict(row, weights):
    return max(0, round(sum(events(row).get(key, 0) * value for key, value in weights.items())))


def metrics(rows, weights=None):
    errors = [(predict(row, weights) if weights is not None else row["native"]["legacy"]) - row["actual"] for row in rows]
    return {
        "rounds": len(rows),
        "mae": sum(abs(e) for e in errors) / len(errors),
        "rmse": math.sqrt(sum(e * e for e in errors) / len(errors)),
        "bias": sum(errors) / len(errors),
        "max_absolute": max(abs(e) for e in errors),
        "max_percent": max(abs(e) / row["actual"] * 100 for e, row in zip(errors, rows)),
        "total_percent": sum(errors) / sum(row["actual"] for row in rows) * 100,
    }


def fit(rows, names, ridge):
    # Scale columns by RMS. Ridge resolves redundant event counts without adding
    # dependencies or allowing a singular matrix to masquerade as a fitted model.
    names = [name for name in names if any(events(row).get(name, 0) for row in rows)]
    scales = [math.sqrt(sum(events(row).get(name, 0) ** 2 for row in rows) / len(rows)) for name in names]
    size = len(names)
    matrix = [[0.0] * size for _ in names]
    target = [0.0] * size
    for row in rows:
        x = [events(row).get(name, 0) / scale for name, scale in zip(names, scales)]
        y = row["actual"] - sum(events(row).get(name, 0) * value for name, value in FIXED.items())
        for i in range(size):
            target[i] += x[i] * y
            for j in range(i + 1):
                matrix[i][j] += x[i] * x[j]
    for i in range(size):
        matrix[i][i] += ridge * len(rows)
        for j in range(i):
            matrix[j][i] = matrix[i][j]
    # Cholesky solves the positive definite normal equations. All fitted values
    # are checked for finiteness before they can become runtime coefficients.
    lower = [[0.0] * size for _ in names]
    for i in range(size):
        for j in range(i + 1):
            value = matrix[i][j] - sum(lower[i][k] * lower[j][k] for k in range(j))
            lower[i][j] = math.sqrt(value) if i == j else value / lower[j][j]
    forward = [0.0] * size
    result = [0.0] * size
    for i in range(size):
        forward[i] = (target[i] - sum(lower[i][j] * forward[j] for j in range(i))) / lower[i][i]
    for i in reversed(range(size)):
        result[i] = (forward[i] - sum(lower[j][i] * result[j] for j in range(i + 1, size))) / lower[i][i]
    weights = FIXED | {name: value / scale for name, value, scale in zip(names, result, scales)}
    if not all(math.isfinite(value) for value in weights.values()):
        raise ValueError("nonfinite model coefficient")
    return weights


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("work", type=Path)
    parser.add_argument("--report", type=Path, default=Path(__file__).with_name("event-model-report.json"))
    parser.add_argument("--model", type=Path, default=Path(__file__).with_name("event-model.json"))
    args = parser.parse_args()
    rows = json.loads((args.work / "measurements.json").read_text())
    fitted = [row for row in rows if row["native"]["phase"] == "Playing" and row["native"]["end_phase"] in ["Playing", "Died", "LevelCleared"]]
    sets = {split: [row for row in fitted if row["split"] == split] for split in ["train", "validation", "test"]}
    if not all(sets.values()):
        raise ValueError("training, validation and test scenarios must all be present")
    # Source-entry counts are measured independently, rather than copied from
    # native execution. Fail if profiling and native work disagree on this domain.
    common = ["tile_draw", "actor_draw", "sprite_erase", "sound_section", "note", "player_animation", "player_draw", "guard_route", "hole_scan", "exit"]
    disagreements = []
    for row in fitted:
        counts = Counter(item["event"] for item in row["machine_events"])
        for event in common:
            if counts[event] != events(row).get(event, 0):
                disagreements.append({"scenario": row["scenario"], "interval": row["interval"], "event": event, "native": events(row).get(event, 0), "machine": counts[event]})
    (args.work / "event-disagreements.json").write_text(json.dumps(disagreements, indent=2) + "\n")
    if disagreements:
        raise ValueError(f"{len(disagreements)} source-entry count disagreements; inspect event-disagreements.json")
    history = []
    names = BASE.copy()
    remaining = GROUPS.copy()
    def choose(columns):
        candidates = [(fit(sets["train"], columns, ridge), ridge) for ridge in [1e-6, 1e-4, 1e-2, 0.1]]
        return min(candidates, key=lambda pair: metrics(sets["validation"], pair[0])["rmse"])
    weights, ridge = choose(names)
    history.append({"factors": names.copy(), "ridge": ridge, "validation": metrics(sets["validation"], weights)})
    while remaining:
        trials = []
        for group in remaining:
            candidate, penalty = choose(names + group)
            trials.append((metrics(sets["validation"], candidate)["rmse"], group, candidate, penalty))
        error, group, candidate, penalty = min(trials, key=lambda trial: trial[0])
        previous = metrics(sets["validation"], weights)["rmse"]
        history[-1]["trials"] = [{"added": trial[1], "rmse": trial[0]} for trial in trials]
        if error >= previous * 0.995:
            break
        names += group
        remaining.remove(group)
        weights, ridge = candidate, penalty
        history.append({"factors": names.copy(), "ridge": ridge, "validation": metrics(sets["validation"], weights)})
    # After group selection, try adding or removing individual factors. Require
    # a material validation improvement at every step, so this search terminates
    # without tuning against the final test set or preserving redundant terms.
    refinement = []
    available = list(dict.fromkeys(BASE + [name for group in GROUPS for name in group]))
    while True:
        current = metrics(sets["validation"], weights)["rmse"]
        trials = []
        for name in available:
            columns = [column for column in names if column != name] if name in names else names + [name]
            candidate, penalty = choose(columns)
            trials.append((metrics(sets["validation"], candidate)["rmse"], name, columns, candidate, penalty))
        best = min(trials, key=lambda trial: trial[0])
        if best[0] >= current * 0.995:
            refinement.append({"stopped_at_rmse": current, "best_trial_rmse": best[0], "best_trial_factor": best[1]})
            break
        error, name, columns, weights, ridge = best
        refinement.append({"factor": name, "action": "remove" if name in names else "add", "validation_rmse": error, "ridge": ridge})
        names = columns
    report = {"refinement": refinement, "measurement_sha256": hashlib.sha256((args.work / "measurements.json").read_bytes()).hexdigest(), "coefficients": weights, "scenarios": {split: sorted({row["scenario"] for row in values}) for split, values in sets.items()}, "selection": history, "stopping_rule": "no remaining group or individual factor addition/removal reduces whole-level validation RMSE by 0.5%", "counts_verified": len(fitted), "metrics": {split: {"legacy": metrics(values), "model": metrics(values, weights)} for split, values in sets.items()}}
    args.model.write_text(json.dumps({"coefficients": weights}, indent=2, sort_keys=True) + "\n")
    (args.work / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    # Rebuild both trace estimates from complete interval records. Intervals
    # outside the fitted gameplay domain preserve their original durations.
    for path in args.work.glob("*-legacy.json"):
        scenario = path.name.removesuffix("-legacy.json")
        trace = json.loads(path.read_text())
        durations = [r["start_delta"] for r in trace["rounds"][1:]] + [trace["final_delta"]]
        for row in fitted:
            if row["scenario"] == scenario:
                durations[row["interval"]] = predict(row, weights)
        for i, interval in enumerate(trace["rounds"]):
            interval["start_delta"] = 0 if i == 0 else durations[i - 1]
        trace["final_delta"] = durations[-1]
        (args.work / f"{scenario}-model.json").write_text(json.dumps(trace, indent=2) + "\n")
    estimates = [{"scenario": row["scenario"], "split": row["split"], "interval": row["interval"],
                  "actual": row["actual"], "legacy": row["native"]["legacy"],
                  "model": predict(row, weights) if row in fitted else row["native"]["legacy"]} for row in rows]
    (args.work / "estimates.json").write_text(json.dumps(estimates, indent=2) + "\n")
    print(json.dumps(report["metrics"], indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, ZeroDivisionError) as error:
        print(f"event model: {error}", file=sys.stderr)
        sys.exit(1)
