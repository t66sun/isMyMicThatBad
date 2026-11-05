"""Measure the installed local RVC worker without opening audio devices."""
import argparse
import importlib.util
import json
import os
import sys
import time
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from resource_monitor import Monitor


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--wav', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--threads', type=int, choices=[0, 1, 2, 4], default=0,
                        help='0 uses the production worker budget')
    parser.add_argument('--expect-threads', type=int)
    parser.add_argument('--schedule', help='comma-separated thread budgets; seconds applies to each phase')
    parser.add_argument('--seconds', type=int, default=60, help='0 checks the expected production thread budget only')
    parser.add_argument('--game-pid', type=int, default=0)
    args = parser.parse_args()
    schedule = [int(x) for x in args.schedule.split(',')] if args.schedule else [args.threads]
    assert all(x in (0, 1, 2, 4) for x in schedule)
    root, out = args.root.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    monitor = Monitor(out, args.game_pid)
    worker_path = args.worker.resolve()
    with wave.open(str(args.wav.resolve())) as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (48000, 1, 2)
        audio = np.frombuffer(wav.readframes(wav.getnframes()), '<i2').astype('<f4')/32768
    spec = importlib.util.spec_from_file_location('local_worker', worker_path)
    worker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(worker)
    if schedule[0]:
        os.environ['OMP_NUM_THREADS'] = str(schedule[0])
    started = time.perf_counter()
    streams = worker.Streams(root)
    torch = streams.torch
    import faiss
    actual_threads = torch.get_num_threads()
    if args.expect_threads is not None:
        assert actual_threads == args.expect_threads, (actual_threads, args.expect_threads)
        assert faiss.omp_get_max_threads() == args.expect_threads
    if args.seconds == 0:
        assert args.expect_threads is not None
        (out/'summary.json').write_text(json.dumps({'production_threads': actual_threads,
                                                   'faiss_threads': faiss.omp_get_max_threads(),
                                                   'verification_only': True}), encoding='utf-8')
        monitor.close()
        return
    schedule = [x or actual_threads for x in schedule]
    torch.set_num_threads(schedule[0])
    faiss.omp_set_num_threads(schedule[0])
    torch.manual_seed(1105)
    opened = streams.open({'id': 'test', 'index': str(root/'models/saisho.index')})
    assert worker.HOP == 9600, 'resource trials must preserve the 200 ms hop'
    torch.manual_seed(111)
    torch.cuda.reset_peak_memory_stats()
    print('ready', round(time.perf_counter()-started, 3), flush=True)
    request = {'id': 'test', 'params': {'pitch': 0, 'formant': 0, 'index': 50, 'rms': 50}}
    blocks = [audio[(np.arange(worker.HOP)+i*worker.HOP) % len(audio)].tobytes()
              for i in range(int(np.ceil(len(audio)/worker.HOP)))]
    # Time the already warmed synthesis graph on its CUDA stream. This is a
    # synthesis span including scheduling waits, not total GPU compute time.
    from infer import rtrvc
    graph_call = rtrvc.run_cuda_graph
    gpu_start = torch.cuda.Event(enable_timing=True)
    gpu_end = torch.cuda.Event(enable_timing=True)
    def timed_synthesis(*arguments):
        gpu_start.record()
        result = graph_call(*arguments)
        gpu_end.record()
        return result
    rtrvc.run_cuda_graph = timed_synthesis
    timings = []
    phase_hops = args.seconds*5
    count = phase_hops*len(schedule)
    phase_cpu = []
    origin = time.perf_counter()
    process_start = time.process_time()
    with (out/'timing.jsonl').open('x') as log, (out/'voice.f32').open('xb') as sound:
        for i in range(count):
            time.sleep(max(0, origin+i*.2-time.perf_counter()))
            phase = i//phase_hops
            if i % phase_hops == 0:
                if i:
                    phase_cpu.append(time.process_time()-phase_started)
                phase_started = time.process_time()
                torch.set_num_threads(schedule[phase])
                faiss.omp_set_num_threads(schedule[phase])
            begin = time.perf_counter()
            cpu = time.process_time()
            reply, payload = streams.process(request, blocks[i % len(blocks)])
            cpu_ms = (time.process_time()-cpu)*1000
            item = {'hop': i, 'phase': phase, 'threads': schedule[phase], 'utc_ns': time.time_ns(), 'ms': reply['milliseconds'],
                    'request_ms': (time.perf_counter()-begin)*1000, 'cpu_ms': cpu_ms,
                    'start_late_ms': max(0, (begin-origin-i*.2)*1000),
                    'synthesis_cuda_span_ms': gpu_start.elapsed_time(gpu_end)}
            timings.append(item)
            print(json.dumps(item), file=log, flush=True)
            sound.write(payload)
    time.sleep(max(0, origin+count*.2-time.perf_counter()))
    phase_cpu.append(time.process_time()-phase_started)
    total_cpu = time.process_time()-process_start
    wall = time.perf_counter()-origin
    monitor.close()
    samples = [s for s in timings if s['hop'] % phase_hops >= 10]
    summary = {'threads': schedule, 'seconds': wall, 'cpu_seconds': total_cpu,
               'torch': torch.__version__, 'cuda': torch.version.cuda,
               'gpu': torch.cuda.get_device_name(), 'logical_cpus': os.cpu_count(),
               'cuda_max_allocated_mib': torch.cuda.max_memory_allocated()/2**20,
               'cuda_max_reserved_mib': torch.cuda.max_memory_reserved()/2**20,
               'worker': str(worker_path), 'hop': worker.HOP,
               'declared_latency_frames': opened['latency'], 'parameters': request['params'],
               'game_pid': args.game_pid,
               'measurement': 'direct worker; no Rust bridge or audio devices; first 10 hops excluded'}
    for key in ('ms', 'request_ms', 'cpu_ms', 'start_late_ms', 'synthesis_cuda_span_ms'):
        values = [s[key] for s in samples]
        summary[key] = {label: float(np.percentile(values, percentile))
                        for label, percentile in [('p50', 50), ('p95', 95), ('p99', 99), ('max', 100)]}
        summary[key]['mean'] = float(np.mean(values))
    summary['phases'] = []
    for phase, threads in enumerate(schedule):
        selected = [s for s in samples if s['phase'] == phase]
        result = {'phase': phase, 'threads': threads, 'audio_seconds': args.seconds, 'cpu_seconds': phase_cpu[phase]}
        for key in ('ms', 'cpu_ms', 'synthesis_cuda_span_ms'):
            values = [s[key] for s in selected]
            result[key] = {'mean': float(np.mean(values)), 'p95': float(np.percentile(values, 95)), 'max': max(values)}
        summary['phases'].append(result)
    (out/'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
