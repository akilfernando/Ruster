"""Read-only Linux counters for exactly the server and its spawned native child."""
import os
from pathlib import Path
import time


def stat(pid):
    # comm may contain spaces and parentheses; fields after its final ')' are stable.
    fields = (Path('/proc') / str(pid) / 'stat').read_text().rsplit(')', 1)[1].split()
    return {'start': int(fields[19]), 'ticks': int(fields[11]) + int(fields[12]),
            'rss_bytes': int(fields[21]) * os.sysconf('SC_PAGE_SIZE')}


class ProcessMetrics:
    def __init__(self, native_pid):
        self.pids = {'bridge': os.getpid(), 'native': native_pid}
        self.starts = {name: stat(pid)['start'] for name, pid in self.pids.items()}
        self.previous = {}
        self.cached = None
        self.last = 0

    def sample(self):
        now = time.monotonic()
        if self.cached is not None and now - self.last < 1:
            return self.cached
        result = {'monotonic_ms': now * 1000, 'processes': {},
                  'cpu_convention': '100 percent = one CPU core',
                  'browser': 'excluded: embedded workspace cannot be attributed',
                  'gpu': 'unmeasured: no whole-GPU attribution'}
        for name, pid in self.pids.items():
            before = stat(pid)
            if before['start'] != self.starts[name]:
                raise ProcessLookupError('owned process identity changed')
            pss = None
            try:
                for line in (Path('/proc') / str(pid) / 'smaps_rollup').read_text().splitlines():
                    if line.startswith('Pss:'):
                        pss = int(line.split()[1]) * 1024
            except (OSError, ValueError):
                pass
            after = stat(pid)
            if after['start'] != before['start']:
                raise ProcessLookupError('owned process identity changed')
            previous = self.previous.get(name)
            interval = now - previous[0] if previous else None
            cpu = ((before['ticks'] - previous[1]) / os.sysconf('SC_CLK_TCK') / interval * 100
                   if previous and interval is not None and interval > 0 and before['ticks'] >= previous[1] else None)
            result['processes'][name] = {'cpu_percent_one_core': cpu,
                'interval_ms': interval * 1000 if interval is not None else None,
                'rss_bytes': before['rss_bytes'], 'pss_bytes': pss,
                'pss_availability': 'available' if pss is not None else 'unavailable'}
            self.previous[name] = (now, before['ticks'])
        self.last, self.cached = now, result
        return result
