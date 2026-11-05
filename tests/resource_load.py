"""Fixed local CPU/CUDA jobs for resource contention trials; no audio playback."""
import argparse
import hashlib
import json
import multiprocessing as mp
import os
import statistics
import time
from pathlib import Path


def save(path, value):
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)


def cpu_work(block, repeats):
    for _ in range(repeats):
        hashlib.sha256(block).digest()


def cpu_jobs(index, config, out, stop):
    block = bytes(1024*1024)
    period = config['cpu_period_ms']/1000
    time.sleep(index*period/config['cpu_workers'])
    due = origin = time.perf_counter()
    initial_cpu = time.process_time()
    count = 0
    with (out/f'cpu-{index}.jsonl').open('x') as log:
        while not stop.is_set():
            if stop.wait(max(0, due-time.perf_counter())):
                break
            start = time.perf_counter()
            cpu = time.process_time()
            cpu_work(block, config['cpu_repeats'])
            end = time.perf_counter()
            print(json.dumps({'job': count, 'utc_ns': time.time_ns(), 'work_ms': (end-start)*1000,
                              'cpu_ms': (time.process_time()-cpu)*1000,
                              'completion_ms': (end-due)*1000}), file=log, flush=True)
            count += 1
            due = max(due+period, end)
    save(out/f'cpu-{index}.json', {'jobs': count, 'seconds': time.perf_counter()-origin,
                                 'cpu_seconds': time.process_time()-initial_cpu})


class GpuJobs:
    def __init__(self, dimension, repeats):
        import torch
        self.torch = torch
        torch.set_num_threads(1)
        torch.backends.cuda.matmul.allow_tf32 = False
        torch.manual_seed(1105)
        self.a = torch.randn(dimension, dimension, device='cuda')
        self.b = torch.randn_like(self.a)
        self.c = torch.empty_like(self.a)
        self.begin = torch.cuda.Event(enable_timing=True)
        self.end = torch.cuda.Event(enable_timing=True)
        self.repeats = repeats
        for _ in range(32):
            self.run()

    def run(self):
        start = time.perf_counter()
        self.begin.record()
        for _ in range(self.repeats):
            self.torch.mm(self.a, self.b, out=self.c)
        self.end.record()
        self.end.synchronize()
        return {'gpu_ms': self.begin.elapsed_time(self.end), 'work_ms': (time.perf_counter()-start)*1000}


def calibrate(path):
    gpu = GpuJobs(2048, 2)
    times = [gpu.run()['work_ms'] for _ in range(40)]
    block = bytes(1024*1024)
    start = time.perf_counter()
    cpu_work(block, 100)
    per_hash = (time.perf_counter()-start)/100
    repeats = max(1, round(.04/per_hash))
    cpus = os.cpu_count()
    workers = max(1, cpus//2)
    config = {'dimension': 2048, 'gpu_repeats': 2, 'gpu_job_alone_ms': statistics.median(times),
              'gpu_middle_period_ms': statistics.median(times)/.6,
              'gpu_high_period_ms': statistics.median(times)/.8,
              'cpu_repeats': repeats, 'cpu_workers': workers,
              'cpu_period_ms': per_hash*repeats*1000/(.4*cpus/workers),
              'logical_cpus': cpus, 'gpu': gpu.torch.cuda.get_device_name(),
              'torch': gpu.torch.__version__, 'tf32': False,
              'description': 'Initial fixed-job estimate without audio. Verify sustained utilization after warmup, then freeze the workload for paired trials; record actual total utilization separately.'}
    save(path, config)
    print(json.dumps(config), flush=True)


def run(config_path, out, seconds, level):
    config = json.loads(config_path.read_text())
    out.mkdir(parents=True, exist_ok=False)
    save(out/'workload.json', dict(config, level=level, planned_seconds=seconds))
    gpu = GpuJobs(config['dimension'], config['gpu_repeats'])
    stop = mp.Event()
    children = []
    if level == 'middle':
        for i in range(config['cpu_workers']):
            child = mp.Process(target=cpu_jobs, args=(i, config, out, stop))
            child.start()
            children.append(child)
    period = config[f'gpu_{level}_period_ms']/1000
    due = origin = time.perf_counter()
    save(out/'ready.json', {'utc_ns': time.time_ns(), 'pid': os.getpid()})
    print('ready', out, flush=True)
    count = 0
    try:
        with (out/'gpu.jsonl').open('x') as log:
            while time.perf_counter()-origin < seconds and not (out/'stop').exists():
                time.sleep(max(0, due-time.perf_counter()))
                timing = gpu.run()
                end = time.perf_counter()
                timing.update(job=count, utc_ns=time.time_ns(), completion_ms=(end-due)*1000)
                print(json.dumps(timing), file=log, flush=True)
                count += 1
                due = max(due+period, end)
    finally:
        stop.set()
        for child in children:
            child.join(10)
            if child.is_alive():
                child.terminate()
                child.join()
        save(out/'result.json', {'jobs': count, 'seconds': time.perf_counter()-origin,
                                  'child_exit_codes': [c.exitcode for c in children]})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['calibrate', 'run'])
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--level', choices=['middle', 'high'], default='middle')
    args = parser.parse_args()
    if args.action == 'calibrate':
        calibrate(args.config.resolve())
    else:
        assert args.out
        run(args.config.resolve(), args.out.resolve(), args.seconds, args.level)
