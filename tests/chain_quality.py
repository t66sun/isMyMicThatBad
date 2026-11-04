"""Prepare deterministic sources and summarize the Rust quality probe outputs."""
import argparse
import json
import math
import random
import wave
from pathlib import Path

import numpy as np
from scipy import signal

RATE = 48000


def write_wav(path, data):
    with wave.open(str(path), "wb") as wav:
        wav.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        wav.writeframes(np.rint(np.clip(data, -1, 32767 / 32768) * 32768).astype("<i2").tobytes())


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False), encoding="utf-8")


def prepare(root, speech):
    if (root / "segments.json").exists() or (root / "stimulus.wav").exists():
        raise FileExistsError("choose a fresh quality directory")
    with wave.open(str(speech), "rb") as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (RATE, 1, 2)
        spoken = np.frombuffer(wav.readframes(wav.getnframes()), "<i2").astype(float) / 32768
    start = np.flatnonzero(abs(spoken) > 0.01)[0]
    spoken = spoken[start:start + 4 * RATE]
    spoken *= 0.55 / np.max(abs(spoken))
    t = np.arange(RATE) / RATE
    vowel = sum(np.sin(2*np.pi*150*k*t)/k for k in range(1, 8)) * 0.24
    vowel *= np.minimum(1, np.minimum(t*50, (1-t)*50))
    rng = np.random.default_rng(1104)
    hiss = signal.sosfilt(signal.butter(4, [4000, 10000], fs=RATE, btype="bandpass", output="sos"), rng.normal(size=RATE))
    hiss *= (np.sin(2*np.pi*2*t) > 0) * 0.08
    burst = 0.8*np.exp(-t*80)*rng.uniform(-1, 1, len(t))
    pieces = [
        ("silence", np.zeros(RATE//2)), ("speech", spoken),
        ("quiet_speech", spoken*0.0631), ("sibilants", hiss), ("plosive", burst),
        ("sweep", 0.35*signal.chirp(t,200,1,11000)),
        ("vowel_150hz", vowel), ("tail_silence", np.zeros(RATE)),
    ]
    offset = 0
    segments = {}
    for name, x in pieces:
        segments[name] = [offset, offset+len(x)]
        offset += len(x)
    dry = np.concatenate([x for _,x in pieces])
    # Pad the worker's last hop with silence.
    dry = np.pad(dry, (0, (-len(dry)) % 12480))
    dry = np.rint(dry*32768).clip(-32768,32767)/32768
    write_wav(root/"stimulus.wav", dry)
    dry.astype("<f4").tofile(root/"dry.f32")
    save(root/"segments.json", segments)
    print(len(dry)/RATE)


def rms(x):
    return float(np.sqrt(np.mean(x.astype(float)**2)))


def voice_metrics(x, segments, shift):
    start, end = segments["vowel_150hz"]
    estimates = []
    for i in range(start+4800, end-4800, 960):
        frame = x[i:i+1920].astype(float)
        frame -= np.mean(frame)
        ac = signal.correlate(frame, frame, mode="full", method="fft")[len(frame)-1:]
        peaks = signal.find_peaks(ac)[0]
        peaks = peaks[(peaks >= 120) & (peaks <= 600)]
        if len(peaks) and ac[peaks].max() > 0.55*ac[0] and ac[0] > 1e-8:
            lag = peaks[ac[peaks] >= 0.9*ac[peaks].max()][0]
            estimates.append(float(RATE/lag))
    target = 150*2**(shift/12)
    cents = [1200*math.log2(f/target) for f in estimates]
    return {"target_hz":target, "voiced_windows":len(cents),
            "median_cents_error":float(np.median(cents)) if cents else None,
            "p95_absolute_cents":float(np.percentile(np.abs(cents),95)) if cents else None,
            "last_80ms_rms":rms(x[end-3840:end]),"following_80ms_rms":rms(x[end:end+3840])}


def listening_clips(root, stages, segments):
    import torch
    from torchaudio.functional import loudness

    torch.set_num_threads(1)
    candidates = ["stage-0", "stage-2", "stage-4", "stage-5", "female", "ethereal"]
    random.Random(1104).shuffle(candidates)
    start, end = segments["speech"][0], segments["quiet_speech"][1]
    clips = [stages[name][start:end].copy() for name in candidates]
    for clip in clips:
        integrated = float(loudness(torch.from_numpy(clip)[None], RATE))
        clip *= 10 ** ((-23 - integrated) / 20)
    common_gain = min(1., 0.95 / max(np.max(abs(clip)) for clip in clips))
    key, measured = {}, {}
    for i, (name, clip) in enumerate(zip(candidates, clips)):
        filename = f"blind-{chr(65 + i)}.wav"
        write_wav(root / filename, clip * common_gain)
        key[filename] = name
        with wave.open(str(root / filename)) as audio:
            pcm = np.frombuffer(audio.readframes(audio.getnframes()), "<i2").astype("float32") / 32768
        measured[filename] = float(loudness(torch.from_numpy(pcm)[None], RATE))
    save(root / "blind-key.json", key)
    assert max(measured.values()) - min(measured.values()) < 0.2, "clips did not match within 0.2 LU"
    return {"target_lufs": -23 + 20 * math.log10(common_gain), "measured_lufs": measured, "rating": "pending"}


def summarize(root):
    if (root / "quality-summary.json").exists() or list(root.glob("blind-*.wav")):
        raise FileExistsError("listening evidence already exists")
    segments = json.loads((root / "segments.json").read_text())
    summary, stages = {}, {}
    for path in sorted(root.glob("*.json")):
        item = json.loads(path.read_text(encoding="utf-8-sig"))
        if not isinstance(item, dict) or "dsp_ns" not in item:
            continue
        raw = np.fromfile(root / (path.stem + ".f32"), "<f4")
        stages[path.stem] = raw[item["algorithmic_frames"]:]
        times = np.asarray(item["dsp_ns"]) / 1000
        summary[path.stem] = {
            "frames": len(raw),
            "peak": float(np.max(abs(raw))),
            "over_full_scale": int(np.sum(abs(raw) > 1)),
            "zeros_pct": float(100 * np.mean(raw == 0)),
            "p99_us": float(np.percentile(times, 99)),
            "max_us": float(times.max()),
        }
    for i in range(1, 6):
        current = stages[f"stage-{i}"]
        previous = stages[f"stage-{i-1}"][:len(current)]
        residual = current - previous
        metrics = summary[f"stage-{i}"]
        metrics["change_rms_over_input_rms"] = rms(residual) / rms(previous)
        windows = residual[:len(residual) // 960 * 960].reshape(-1, 960)
        metrics["largest_change_20ms_start_s"] = float(np.argmax(np.mean(windows**2, axis=1)) / 50)
        metrics["sections"] = {
            section: {
                "input_rms": rms(previous[start:end]),
                "output_rms": rms(current[start:end]),
                "residual_rms": rms(residual[start:end]),
            }
            for section, (start, end) in segments.items() if start < len(current)
        }
    for name, shift in [("female", 5), ("ethereal", 3)]:
        summary[name].update(voice_metrics(stages[name], segments, shift))
    summary["listening"] = listening_clips(root, stages, segments)
    save(root / "quality-summary.json", summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "summarize"])
    parser.add_argument("root", type=Path)
    parser.add_argument("--speech", type=Path)
    args = parser.parse_args()
    if args.action == "prepare":
        if args.speech is None:
            parser.error("prepare requires --speech")
        prepare(args.root, args.speech)
    else:
        summarize(args.root)
