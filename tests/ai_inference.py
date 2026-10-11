"""Short real-GPU regression probe for the local RVC worker protocol.

This script intentionally requires the prepared CUDA runtime and never falls
back to CPU. It reports worker-reported single-block inference latency; the
separate wall-clock figure includes startup, model loading, and pipe traffic.
"""
import argparse
import json
import math
import queue
import struct
import subprocess
import sys
import threading
import time
import wave
from pathlib import Path


HOP = 12_480
RATE = 48_000
TOTAL_TIMEOUT_SECONDS = 120
EOF = object()


class PipeReader:
    """Read framed worker output on Windows while keeping deadline checks."""

    def __init__(self, stream):
        self._stream = stream
        self._chunks = queue.Queue()
        self._buffer = bytearray()
        self._thread = threading.Thread(target=self._pump, daemon=True)
        self._thread.start()

    def _pump(self):
        try:
            while True:
                chunk = self._stream.read1(65_536)
                if not chunk:
                    self._chunks.put(EOF)
                    return
                self._chunks.put(chunk)
        except Exception as error:
            self._chunks.put(error)

    def read_exact(self, count, deadline):
        while len(self._buffer) < count:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("worker exceeded the 120-second total test deadline")
            try:
                chunk = self._chunks.get(timeout=remaining)
            except queue.Empty as error:
                raise TimeoutError("worker exceeded the 120-second total test deadline") from error
            if chunk is EOF:
                raise RuntimeError("worker closed stdout before completing its response")
            if isinstance(chunk, Exception):
                raise RuntimeError(f"failed reading worker stdout: {chunk}") from chunk
            self._buffer.extend(chunk)
        result = bytes(self._buffer[:count])
        del self._buffer[:count]
        return result

    def read_line(self, deadline):
        while True:
            newline = self._buffer.find(b"\n")
            if newline >= 0:
                if newline > 4096:
                    raise RuntimeError("worker response header exceeded 4096 bytes")
                result = bytes(self._buffer[:newline])
                del self._buffer[:newline + 1]
                return result
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("worker exceeded the 120-second total test deadline")
            try:
                chunk = self._chunks.get(timeout=remaining)
            except queue.Empty as error:
                raise TimeoutError("worker exceeded the 120-second total test deadline") from error
            if chunk is EOF:
                raise RuntimeError("worker closed stdout before completing its response header")
            if isinstance(chunk, Exception):
                raise RuntimeError(f"failed reading worker stdout: {chunk}") from chunk
            self._buffer.extend(chunk)
            newline = self._buffer.find(b"\n")
            if newline > 4096 or (newline < 0 and len(self._buffer) > 4096):
                raise RuntimeError("worker response header exceeded 4096 bytes")


def load_wav(path):
    with wave.open(str(path), "rb") as source:
        if source.getnchannels() != 1 or source.getframerate() != RATE or source.getsampwidth() != 2:
            raise ValueError("input WAV must be 48 kHz, mono, signed PCM16")
        if source.getcomptype() != "NONE":
            raise ValueError("input WAV must be uncompressed PCM")
        raw = source.readframes(source.getnframes())
    if not raw:
        raise ValueError("input WAV contains no samples")
    return [sample[0] / 32768.0 for sample in struct.iter_unpack("<h", raw)]


def request(process, reader, payload, deadline, body=b""):
    if time.monotonic() >= deadline:
        raise TimeoutError("worker exceeded the 120-second total test deadline")
    process.stdin.write(json.dumps(payload, separators=(",", ":")).encode("utf-8") + b"\n")
    if body:
        process.stdin.write(body)
    process.stdin.flush()
    header = json.loads(reader.read_line(deadline))
    if header.get("ok") is not True:
        raise RuntimeError(f"worker rejected {payload.get('op')} request: {header.get('error', header)}")
    return header


def rms(sum_squares, sample_count):
    return math.sqrt(sum_squares / sample_count) if sample_count else 0.0


def percentile_95(values):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]


def pcm16_bytes(samples):
    pcm = [max(-32768, min(32767, round(sample * 32768.0))) for sample in samples]
    return struct.pack(f"<{len(pcm)}h", *pcm)


def terminate_child(process):
    if process.poll() is None:
        try:
            process.terminate()
        except OSError:
            pass


def run(args):
    root = args.root.resolve()
    wav_path = args.wav.resolve()
    runtime_python = root / "python" / "python.exe"
    worker_path = Path(__file__).resolve().parents[1] / "src" / "native" / "ai" / "worker.py"
    model_path = root / "models" / "saisho.pth"
    index_path = root / "models" / "saisho.index"
    for path, label in ((runtime_python, "prepared Python"), (worker_path, "RVC worker"),
                         (model_path, "default Saisho model"), (index_path, "default Saisho index")):
        if not path.is_file():
            raise FileNotFoundError(f"{label} not found: {path}")

    source = load_wav(wav_path)
    block_count = math.ceil(args.seconds * RATE / HOP)
    samples_per_stream = block_count * HOP
    deadline = time.monotonic() + TOTAL_TIMEOUT_SECONDS
    wall_started = time.monotonic()
    worker_log_path = root / "probe-worker.log"
    process = None
    timeout_timer = None
    inference_ms = [[] for _ in range(args.streams)]
    sums_input = [0.0] * args.streams
    sums_output = [0.0] * args.streams
    sums_difference = [0.0] * args.streams
    probe_blocks = []

    with worker_log_path.open("wb") as worker_log:
        try:
            process = subprocess.Popen(
                [str(runtime_python), "-I", "-u", str(worker_path), "--root", str(root)],
                cwd=str(root), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=worker_log, creationflags=0x08000000,
            )
            timeout_timer = threading.Timer(max(0.0, deadline - time.monotonic()), terminate_child,
                                            args=(process,))
            timeout_timer.daemon = True
            timeout_timer.start()
            reader = PipeReader(process.stdout)
            ready = json.loads(reader.read_line(deadline))
            if ready.get("ok") is not True or ready.get("ready") is not True:
                raise RuntimeError(f"worker did not become ready: {ready}")

            stream_ids = [f"probe-{index}" for index in range(args.streams)]
            for stream_id in stream_ids:
                opened = request(process, reader, {
                    "op": "open", "id": stream_id,
                    "path": str(model_path), "index": str(index_path),
                }, deadline)
                if opened.get("frames") != HOP or opened.get("latency") != 2 * HOP:
                    raise RuntimeError(f"unexpected open response: {opened}")

            params = {"pitch": 0, "formant": 0, "index": 50, "rms": 50}
            for block_index in range(block_count):
                offset = (block_index * HOP) % len(source)
                block = [source[(offset + index) % len(source)] for index in range(HOP)]
                payload = struct.pack(f"<{HOP}f", *block)
                block_input_sq = math.fsum(sample * sample for sample in block)
                for stream_index, stream_id in enumerate(stream_ids):
                    response = request(process, reader, {
                        "op": "process", "id": stream_id, "params": params,
                    }, deadline, payload)
                    if response.get("frames") != HOP:
                        raise RuntimeError(f"unexpected process response: {response}")
                    elapsed = response.get("milliseconds")
                    if isinstance(elapsed, bool) or not isinstance(elapsed, (int, float)) or not math.isfinite(elapsed) or elapsed < 0:
                        raise RuntimeError(f"invalid worker inference time: {response}")
                    output_raw = reader.read_exact(HOP * 4, deadline)
                    output = [sample[0] for sample in struct.iter_unpack("<f", output_raw)]
                    if len(output) != HOP or not all(math.isfinite(sample) for sample in output):
                        raise RuntimeError(f"stream {stream_id} returned invalid audio samples")

                    inference_ms[stream_index].append(float(elapsed))
                    sums_input[stream_index] += block_input_sq
                    sums_output[stream_index] += math.fsum(sample * sample for sample in output)
                    sums_difference[stream_index] += math.fsum((source_sample - result_sample) ** 2
                                                               for source_sample, result_sample in zip(block, output))
                    if stream_index == 0:
                        probe_blocks.append(pcm16_bytes(output))

            for stream_id in stream_ids:
                closed = request(process, reader, {"op": "close", "id": stream_id}, deadline)
                if closed.get("ok") is not True:
                    raise RuntimeError(f"worker failed to close {stream_id}: {closed}")

            worker_log = worker_log_path.read_text(encoding="utf-8", errors="replace")
            for marker in ("Invalid index", "索引无效", "Traceback (most recent call last)"):
                if marker in worker_log:
                    raise AssertionError(f"worker log contains internal error marker {marker!r}: {worker_log_path}")

            probe_path = root / "probe.wav"
            with wave.open(str(probe_path), "wb") as output_wav:
                output_wav.setnchannels(1)
                output_wav.setsampwidth(2)
                output_wav.setframerate(RATE)
                output_wav.writeframes(b"".join(probe_blocks))

            wall_elapsed = time.monotonic() - wall_started
            print(f"Worker wall time (startup, model load, and processing): {wall_elapsed:.2f} s")
            print("Single-block inference latency (worker-reported; not end-to-end):")
            for index, stream_id in enumerate(stream_ids):
                output_rms = rms(sums_output[index], samples_per_stream)
                input_rms = rms(sums_input[index], samples_per_stream)
                difference_rms = rms(sums_difference[index], samples_per_stream)
                if input_rms <= 1e-5:
                    raise AssertionError(f"stream {stream_id} input is silent (RMS {input_rms:.8g})")
                if output_rms <= 1e-5:
                    raise AssertionError(f"stream {stream_id} output is silent (RMS {output_rms:.8g})")
                if difference_rms <= 1e-4:
                    raise AssertionError(f"stream {stream_id} output did not differ from input (delta RMS {difference_rms:.8g})")
                print(f"  {stream_id}: p95={percentile_95(inference_ms[index]):.2f} ms, "
                      f"max={max(inference_ms[index]):.2f} ms, input RMS={input_rms:.6f}, "
                      f"output RMS={output_rms:.6f}, input/output delta RMS={difference_rms:.6f}")
            print("GPU memory: not measured")
            print(f"Probe audio: {probe_path}")
            print(f"Worker log: {worker_log_path}")
        finally:
            if timeout_timer is not None:
                timeout_timer.cancel()
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path, help="prepared isolated AI runtime root")
    parser.add_argument("--wav", required=True, type=Path, help="48 kHz mono PCM16 input WAV")
    parser.add_argument("--seconds", type=float, default=3.0, help="requested probe duration; default 3 seconds")
    parser.add_argument("--streams", type=int, choices=(1, 2), default=1, help="concurrent model streams; default 1")
    args = parser.parse_args()
    if not math.isfinite(args.seconds) or args.seconds <= 0:
        parser.error("--seconds must be a finite positive number")
    try:
        run(args)
    except Exception as error:
        print(f"AI inference probe failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
