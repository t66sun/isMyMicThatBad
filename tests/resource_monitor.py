"""Windows process/CPU counters and NVIDIA telemetry for a silent acceptance run."""
import argparse
import atexit
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import threading
import time


class Monitor:
    def __init__(self, out, game_pid=0, gpu=True):
        self.out = Path(out)
        self.closed = False
        self.stop = threading.Event()
        self.kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        self.kernel.OpenProcess.restype = ctypes.c_void_p
        self.kernel.GetProcessTimes.argtypes = [ctypes.c_void_p]+[ctypes.POINTER(wintypes.FILETIME)]*4
        self.kernel.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.DWORD)]
        self.kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        self.game = self.kernel.OpenProcess(0x1000, False, game_pid) if game_pid else None
        if game_pid and not self.game:
            raise ctypes.WinError(ctypes.get_last_error())
        self.gpu = None
        if gpu:
            self.gpu_log = (self.out/'gpu.csv').open('x')
            self.gpu_errors = (self.out/'gpu-errors.log').open('x')
            self.gpu = subprocess.Popen(['nvidia-smi.exe', '--query-gpu=timestamp,utilization.gpu,clocks.sm,temperature.gpu,memory.used,power.draw',
                                         '--format=csv', '-l', '1'], stdout=self.gpu_log,
                                        stderr=self.gpu_errors, creationflags=subprocess.CREATE_NO_WINDOW)
        self.thread = threading.Thread(target=self.sample, daemon=True)
        self.thread.start()
        atexit.register(self.close)

    @staticmethod
    def ticks(value):
        return (value.dwHighDateTime << 32)+value.dwLowDateTime

    def times(self):
        idle, kernel, user = (wintypes.FILETIME() for _ in range(3))
        if not self.kernel.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
            raise ctypes.WinError(ctypes.get_last_error())
        game_cpu, active = None, False
        if self.game:
            created, exited, game_kernel, game_user = (wintypes.FILETIME() for _ in range(4))
            if not self.kernel.GetProcessTimes(self.game, ctypes.byref(created), ctypes.byref(exited),
                                               ctypes.byref(game_kernel), ctypes.byref(game_user)):
                raise ctypes.WinError(ctypes.get_last_error())
            game_cpu = self.ticks(game_kernel)+self.ticks(game_user)
            code = wintypes.DWORD()
            self.kernel.GetExitCodeProcess(self.game, ctypes.byref(code))
            active = code.value == 259
        return (self.ticks(idle), self.ticks(kernel)+self.ticks(user), game_cpu, active)

    def sample(self):
        try:
            previous = self.times()
            before = time.perf_counter()
            with (self.out/'cpu.jsonl').open('x') as log:
                while not self.stop.wait(1):
                    current = self.times()
                    now = time.perf_counter()
                    total = current[1]-previous[1]
                    record = {'utc_ns': time.time_ns(), 'cpu_percent': 100*(1-(current[0]-previous[0])/total),
                              'game_active': current[3],
                              'game_cpu_percent': ((current[2]-previous[2])/1e7/(now-before)/os.cpu_count()*100)
                              if current[2] is not None else None}
                    print(json.dumps(record), file=log, flush=True)
                    previous, before = current, now
        except Exception as error:
            (self.out/'monitor-error.txt').write_text(repr(error), encoding='utf-8')

    def close(self):
        if self.closed:
            return
        self.closed = True
        self.stop.set()
        self.thread.join()
        if self.gpu is not None:
            if self.gpu.poll() is None:
                self.gpu.terminate()
            self.gpu.wait()
            self.gpu_log.close()
            self.gpu_errors.close()
        if self.game:
            self.kernel.CloseHandle(self.game)
        atexit.unregister(self.close)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--game-pid', type=int, default=0)
    parser.add_argument('--seconds', type=int, default=2100)
    parser.add_argument('--cpu-only', action='store_true', help='the audio runner already records GPU telemetry')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    monitor = Monitor(args.out, args.game_pid, gpu=not args.cpu_only)
    start = time.perf_counter()
    try:
        while time.perf_counter()-start < args.seconds and not (args.out/'stop').exists():
            time.sleep(.5)
    finally:
        monitor.close()
