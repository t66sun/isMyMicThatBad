"""Local RVC streams over inherited pipes; no audio files or network services."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import time
import traceback

HOP = 12480
os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                  TORCH_FORCE_WEIGHTS_ONLY_LOAD="1", RVC_CUDA_GRAPH="0",
                  OMP_NUM_THREADS="4", OPENBLAS_NUM_THREADS="1")


def read_exact(stream, count):
    result = bytearray()
    while len(result) < count:
        chunk = stream.read(count - len(result))
        if not chunk:
            raise EOFError("RVC pipe closed during audio request")
        result.extend(chunk)
    return result


class Streams:
    def __init__(self, root):
        import numpy as np
        import torch
        torch.set_num_threads(4)
        if not torch.cuda.is_available():
            raise RuntimeError("CUDA GPU is unavailable; RVC processing has stopped")
        self.np = np
        self.torch = torch
        self.root = root
        sys.path.insert(0, str(root / "rvc"))
        os.chdir(root / "rvc")
        sys.argv = [sys.argv[0]]
        path = root / "rvc/RVCRealtimeVST/worker/rvc_worker.py"
        spec = importlib.util.spec_from_file_location("official_rvc_stream", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.engine_type = module.RVCStreamEngine
        self.streams = {}
        self.last_rvc = None
        from infer import rtrvc
        import faiss
        faiss.omp_set_num_threads(4)
        original = rtrvc.RVC
        original_index_rate = original.change_index_rate

        def cached_index_rate(rvc, rate):
            if rate and not hasattr(rvc, "index"):
                original_index_rate(rvc, rate)
                if rvc.index.d != (256 if rvc.version == "v1" else 768) or rvc.index.ntotal < 8:
                    raise ValueError("RVC index does not match this model or has fewer than eight vectors")
                if hasattr(rvc.index, "nprobe"):
                    # Eight clusters avoid dropping retrieval when the nearest cluster has fewer than eight vectors.
                    rvc.index.nprobe = min(8, rvc.index.nlist)
            else:
                rvc.index_rate = rate
        original.change_index_rate = cached_index_rate

        def shared_rvc(*args):
            previous = self.last_rvc
            rvc = original(*args[:-1], previous)
            if previous is not None and previous.index_path == rvc.index_path and hasattr(previous, "index"):
                rvc.index = previous.index
                rvc.big_npy = previous.big_npy
                rvc.index_rate = previous.index_rate
            self.last_rvc = rvc
            return rvc
        rtrvc.RVC = shared_rvc

    def open(self, request):
        model = Path(request.get("path") or self.root / "models/saisho.pth").resolve()
        index = str(Path(request["index"]).resolve()) if request.get("index") else ""
        if not model.is_file() or model.suffix.lower() != ".pth":
            raise ValueError("Select an existing RVC .pth model")
        if index and (not Path(index).is_file() or Path(index).suffix.lower() != ".index"):
            raise ValueError("Select an existing RVC .index file")
        if self.streams and any(engine.rvc.pth_path != str(model) for engine in self.streams.values()):
            raise ValueError("Stop processing before changing the RVC model")
        engine = self.engine_type(dict(rvc_root=str(self.root / "rvc"), model_path=str(model),
                                      index_path=index, sample_rate=48000, block_ms=260,
                                      crossfade_ms=40, extra_ms=1500))
        if self.torch.device(engine.config.device).type != "cuda":
            raise RuntimeError("RVC requires a supported CUDA GPU")
        with self.torch.inference_mode():
            engine.prewarm()
            engine.process(self.np.zeros(HOP, dtype=self.np.float32), 0, 0, 0.5 if index else 0,
                           0.5, -60, 0)
            engine.input_wav.zero_()
            engine.input_wav_res.zero_()
            engine.rms_buffer.fill(0)
            engine.sola_buffer.zero_()
            engine.rvc.cache_pitch.zero_()
            engine.rvc.cache_pitchf.zero_()
        if engine.block_frame != HOP:
            raise ValueError("RVC block size mismatch")
        self.streams[request["id"]] = engine
        return {"ok": True, "frames": HOP, "latency": 2 * HOP}

    def process(self, request, data):
        engine = self.streams[request["id"]]
        parameters = request["params"]
        values = [float(parameters[key]) for key in ("pitch", "formant", "index", "rms")]
        if not all(self.np.isfinite(values)) or not all(lo <= x <= hi for x, lo, hi in zip(values, [-24,-12,0,0], [24,12,100,100])):
            raise ValueError("Invalid RVC parameters")
        if values[2] and not engine.rvc.index_path:
            values[2] = 0
        samples = self.np.frombuffer(data, dtype="<f4")
        if len(samples) != HOP or not self.np.isfinite(samples).all():
            raise ValueError("Invalid RVC audio block")
        started = time.perf_counter()
        with self.torch.inference_mode():
            output = engine.process(samples, values[0], values[1], values[2] / 100,
                                    values[3] / 100, -60, 0)
        elapsed = (time.perf_counter() - started) * 1000
        if len(output) != HOP or not self.np.isfinite(output).all():
            raise RuntimeError("RVC returned invalid samples")
        return {"ok": True, "frames": HOP, "milliseconds": elapsed}, output.astype("<f4").tobytes()


def serve(root):
    source, target = sys.stdin.buffer, sys.stdout.buffer
    # RVC prints model diagnostics; keep stdout reserved for framed replies.
    sys.stdout = sys.stderr
    streams = Streams(root)
    target.write(b'{"ok":true,"ready":true}\n')
    target.flush()
    while True:
        line = source.readline(400001)
        if not line:
            break
        if len(line) > 400000 or not line.endswith(b"\n"):
            raise ValueError("Invalid RVC request header")
        request = json.loads(line)
        data = b""
        if request["op"] == "open":
            result = streams.open(request)
        elif request["op"] == "process":
            result, data = streams.process(request, read_exact(source, HOP * 4))
        elif request["op"] == "close":
            streams.streams.pop(request["id"], None)
            result = {"ok": True}
        else:
            raise ValueError("Unknown RVC request")
        target.write(json.dumps(result).encode("utf-8") + b"\n" + data)
        target.flush()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    try:
        serve(args.root.resolve())
    except Exception as error:
        traceback.print_exc(file=sys.stderr)
        try:
            sys.__stdout__.buffer.write(json.dumps({"ok": False, "error": str(error)[:1024]}).encode("utf-8") + b"\n")
            sys.__stdout__.buffer.flush()
        except OSError:
            pass
        raise SystemExit(1)
