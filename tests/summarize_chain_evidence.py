"""Condense fixed-chain trials while retaining the paths to their raw samples."""
import argparse
import collections
import json
import math
from pathlib import Path


def data(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def rows(path):
    if path.exists():
        with path.open(encoding="utf-8-sig") as stream:
            for line in stream:
                if line.strip():
                    yield json.loads(line)


def distribution(values):
    if not values:
        return None
    ordered = sorted(values)
    result = {"min": ordered[0], "max": ordered[-1], "count": len(ordered)}
    for label, fraction in (("p50", .5), ("p95", .95), ("p99", .99)):
        result[label] = ordered[math.ceil(len(ordered) * fraction) - 1]
    return result


def summarize(run):
    result = data(run / "result.json")
    sets = collections.defaultdict(list)
    replies = {}
    request = fault = config = last_queue = queue_at_fault = None
    underruns = collections.Counter()
    clipped = collections.Counter()
    for row in rows(run / "timeline.jsonl"):
        kind = row["kind"]
        if kind == "config":
            config = row
        elif kind == "ai_request":
            request = row
        elif kind == "ai_reply":
            replies[row["hop"]] = row
            for key in ("inference_us", "roundtrip_us"):
                sets[key].append(row[key])
        elif kind == "dsp":
            sets["dsp_us"].append(row["elapsed_us"])
        elif kind == "ai_queue":
            last_queue = row
            sets["playable_frames"].append(row["playable_frames"])
        elif kind == "fault" and fault is None:
            fault = dict(row, request=request)
            queue_at_fault = last_queue
        elif kind in ("underrun", "render"):
            endpoint = "monitor" if row["monitor"] else "main"
            if kind == "underrun":
                underruns[endpoint] += 1
            else:
                clipped[endpoint] += row["clipped"]
    if fault:
        reply = replies.get(fault["request"]["hop"]) if fault["request"] else None
        fault.update(last_queue=queue_at_fault, reply=reply)
        if reply:
            fault["reply_after_fault_ms"] = (reply["t_us"] - fault["t_us"]) / 1000
    peaks = {}
    for row in rows(run / "levels.jsonl"):
        for key in ("input", "cable", "stimulus", "monitor"):
            if row.get(key):
                peaks[key] = max(peaks.get(key, 0), row[key][1])
    return {
        "directory": run.name, "manifest": data(run / "manifest.json"),
        "outcome": result["outcome"], "engine": result.get("engine", result.get("status")),
        "first_output_s": result.get("first_output_s"), "seconds_after_output": result.get("seconds_after_output"),
        "startup_counters": result.get("baseline"), "observer_rates": result.get("observer_rates"), "peak_2ms_rms": peaks,
        "stream": config, "trace_lost": result.get("trace_lost"), "first_fault": fault,
        "distribution": {k: distribution(v) for k,v in sets.items()},
        "render_underruns": dict(underruns), "clipped_render_samples": dict(clipped),
        "calibration": {k:{a:b for a,b in result[k].items() if a!="raw"}
                        for k in ("latency","stimulus_to_mic","monitor_match") if result.get(k)},
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    results = [summarize(p.parent) for p in sorted(args.root.glob("*/result.json"))
               if (p.parent / "manifest.json").is_file()]
    payload = json.dumps(results, indent=2, ensure_ascii=False, allow_nan=False)
    if args.output:
        with args.output.open("x", encoding="utf-8") as dest:
            dest.write(payload)
    else:
        print(payload)
