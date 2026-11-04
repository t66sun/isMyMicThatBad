"""Measure the bundled RVC implementation's existing five host-clock boundaries."""
import argparse
import importlib.util
import json
import time
import wave
from pathlib import Path

import numpy as np


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--wav", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--blocks", type=int, default=48)
    args = parser.parse_args()
    if args.blocks < 1:
        parser.error("--blocks must be positive")
    root = args.root.resolve()
    output = args.output.resolve()
    with wave.open(str(args.wav.resolve()), "rb") as source:
        assert (source.getframerate(), source.getnchannels(), source.getsampwidth()) == (48000, 1, 2)
        samples = np.frombuffer(source.readframes(source.getnframes()), "<i2").astype("<f4") / 32768
    path = Path(__file__).resolve().parents[1] / "src/native/ai/worker.py"
    spec = importlib.util.spec_from_file_location("local_worker", path)
    worker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(worker)
    streams = worker.Streams(root)
    from infer import rtrvc
    original_clock = rtrvc.ttime
    marks = []

    def observe_clock():
        value = original_clock()
        marks.append(time.perf_counter())
        return value

    offsets = np.arange(worker.HOP)
    with output.open("x", encoding="utf-8") as log:
        try:
            for arm, strength in enumerate([50, 0, 0, 50]):
                streams.open({"id":arm,"path":str(root/"models/saisho.pth"),"index":str(root/"models/saisho.index")})
                params = {"pitch":0,"formant":0,"index":strength,"rms":50}
                rtrvc.ttime = observe_clock
                start = time.perf_counter()
                for hop in range(args.blocks):
                    time.sleep(max(0, start + hop*worker.HOP/48000 - time.perf_counter()))
                    raw = samples[(offsets+hop*worker.HOP) % len(samples)].tobytes()
                    marks.clear()
                    reply, audio = streams.process({"id":arm,"params":params}, raw)
                    assert len(marks) == 5, "RVC stage boundaries changed"
                    assert np.isfinite(np.frombuffer(audio, "<f4")).all()
                    stages = dict(zip(("features_ms","index_ms","pitch_ms","model_ms"),np.diff(marks)*1000))
                    row = {"arm":arm,"index_strength":strength,"hop":hop,"worker_ms":reply["milliseconds"],**stages}
                    row["outside_infer_ms"] = row["worker_ms"] - sum(stages.values())
                    print(json.dumps(row), file=log, flush=True)
                rtrvc.ttime = original_clock
                del streams.streams[arm]
        finally:
            rtrvc.ttime = original_clock


if __name__ == "__main__":
    main()
