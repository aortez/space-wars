#!/usr/bin/env python3
"""Summarize raw bounded Pizza captures; keep physical work beside timings."""
import argparse, csv, io, json, math, statistics
from pathlib import Path

def summary(values):
    values = sorted(values)
    if not values:
        return None
    return dict(mean=statistics.mean(values), **{name: values[max(0, math.ceil(p * len(values)) - 1)] for name, p in [('p50', 0.5), ('p95', 0.95), ('p99', 0.99), ('max', 1)]})

def simulation(path):
    d = json.loads(path.read_text())
    s = d['samples']
    timings = {k: summary([v[k] / 1000 for v in s]) for k in s[0] if k.endswith('_us')}
    increments = []
    last = 0
    for v in s:
        now = v['population']['births']['exploded']
        increments.append(now - last)
        last = now
    windows = []
    for start in range(0, len(s), 120):
        a = s[start:start + 120]
        windows.append(dict(first_tick=a[0]['tick'], last_tick=a[-1]['tick'], mean_live=statistics.mean((x['population']['balls'] for x in a)), step_ms=summary([x['step_us'] / 1000 for x in a]), exploded=sum(increments[start:start + 120]), generation_deaths=a[-1]['population']['births']['exploded_by_generation']))
    return dict(samples=len(s), timings_ms=timings, first_population=s[0]['population'], final_population=s[-1]['population'], peak_live=max((x['population']['balls'] for x in s)), explosion_updates=sum((x > 0 for x in increments)), single_parent_updates=sum((x == 1 for x in increments)), no_explosion_updates=sum((x == 0 for x in increments)), windows=windows)

def cpu(path):
    samples = []
    for block in path.read_text().split('BEGIN ')[1:]:
        lines = block.splitlines()
        d = {'index': int(lines[0]), 'uptime': float(lines[1].split()[0]), 'cores': {}, 'threads': {}}
        for line in lines[2:]:
            if line.startswith('cpu'):
                words = line.split()
                vals = list(map(int, words[1:9]))
                d['cores'][words[0]] = (sum(vals), vals[3] + vals[4])
            elif line.startswith(('PROCESS ', 'THREAD ')):
                kind, stat = line.split(' ', 1)
                tid = int(stat.split(' ', 1)[0])
                v = stat[stat.rfind(')') + 2:].split()
                ticks = int(v[11]) + int(v[12])
                if kind == 'PROCESS':
                    d['process'] = ticks
                    d['pid'] = tid
                    d['rss_pages'] = int(v[21])
                else:
                    d['threads'][tid] = ticks
            elif line.startswith(('TEMP ', 'FREQ ', 'THROTTLED ')):
                key, val = line.split()
                d[key.lower()] = int(val, 0)
        samples.append(d)
    intervals = []
    for a, b in zip(samples, samples[1:]):
        ncores = len(b['cores']) - 1
        ticks = (b['cores']['cpu'][0] - a['cores']['cpu'][0]) / ncores
        if ticks <= 0 or a['pid'] != b['pid']:
            continue
        core = {k: 100 * (1 - (b['cores'][k][1] - a['cores'][k][1]) / (b['cores'][k][0] - a['cores'][k][0])) for k in b['cores'] if k != 'cpu' and b['cores'][k][0] > a['cores'][k][0]}
        intervals.append(dict(weight_ticks=ticks, index=b['index'], start_uptime=a['uptime'], end_uptime=b['uptime'], process_pct=100 * (b['process'] - a['process']) / ticks, main_thread_pct=100 * (b['threads'][b['pid']] - a['threads'].get(a['pid'], 0)) / ticks, cores_pct=core, temp_c=b['temp'] / 1000, freq_khz=b['freq'], throttled=b['throttled']))
    return cpu_intervals(intervals)

def cpu_intervals(intervals):
    """Aggregate selected intervals without joining streaks across capture gaps."""
    streak = max_streak = 0
    previous_end = None
    for x in intervals:
        if previous_end != x['start_uptime']:
            streak = 0
        streak = streak + (x['end_uptime'] - x['start_uptime']) if x['main_thread_pct'] >= 95 else 0
        max_streak = max(max_streak, streak)
        previous_end = x['end_uptime']

    def utilization(key):
        result = summary([x[key] for x in intervals])
        if result is not None:
            result['mean'] = sum((x[key] * x['weight_ticks'] for x in intervals)) / sum((x['weight_ticks'] for x in intervals))
        return result
    throttle_bits = 0
    for x in intervals:
        throttle_bits |= x['throttled']
    return dict(intervals=intervals, process_pct=utilization('process_pct'), main_thread_pct=utilization('main_thread_pct'), longest_main_thread_ge95_seconds=max_streak, temperature_c=summary([x['temp_c'] for x in intervals]), throttle_bits=throttle_bits)

def render(paths):
    traces = []
    for path in paths:
        text = path.read_text()
        start = text.find('loop,marker,')
        if start < 0:
            continue
        rows = list(csv.DictReader(io.StringIO(text[start:])))
        rows = [r for r in rows if r.get('loop', '').isdigit() and r.get('marker') == '1']

        def numbers(key):
            return [float(r[key]) for r in rows if r.get(key)]

        def intervals(key):
            xs = numbers(key)
            return [b - a for a, b in zip(xs, xs[1:]) if b > a]
        flips = [x / 1000 for x in intervals('flip_kernel_us')]
        stages = {k: summary(numbers(k)) for k in rows[0] if k.endswith(('_wall_ms', '_cpu_ms'))} if rows else {}
        traces.append(dict(file=path.name, loops=len(rows), flip_intervals_ms=summary(flips), flip_intervals_over_25ms=sum((x > 25 for x in flips)), flip_interval_samples=len(flips), submit_intervals_ms=summary(intervals('submit_at_ms')), stages=stages))
    return traces

def before_export(result, last_tick):
    """Only keep windows whose ending status precedes the final recorded update.

    CPU samples precede their status request. Completed KMS traces are read after
    that request, but their captured loops have already finished. Boundary and
    final partial traces cannot prove that export has not started, so omit them.
    """
    ticks = {s['sample']: json.loads(s['pizza'])['tick']
             for s in result['statuses'] if s.get('pizza')}
    intervals = [x for x in result['cpu']['intervals']
                 if x['index'] in ticks and x['index'] - 1 in ticks
                 and ticks[x['index']] < last_tick
                 and ticks[x['index'] - 1] < last_tick]
    traces = []
    for trace in result['render']:
        suffix = Path(trace['file']).stem.removeprefix('trace-')
        if suffix.isdigit() and int(suffix) in ticks and ticks[int(suffix)] < last_tick:
            traces.append(trace)
    return {'cpu': cpu_intervals(intervals), 'render': traces,
            'last_simulation_tick': last_tick,
            'scope': 'Conservative status brackets before JSON export; not an exact update/frame join.'}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('path', type=Path)
    args = parser.parse_args()
    p = args.path
    if p.is_file():
        result = simulation(p)
    else:
        result = {'simulation': simulation(p / 'simulation.json') if (p / 'simulation.json').exists() else None, 'cpu': cpu(p / 'cpu.txt'), 'render': render(sorted(p.glob('trace-*.txt')))}
        statuses = []
        for f in sorted(p.glob('status-*.txt'), key=lambda x: int(x.stem.split('-')[1])):
            fields = dict((line.split('=', 1) for line in f.read_text().splitlines() if '=' in line))
            values = {k: fields.get(k) for k in ['fps', 'ups', 'frames_total', 'updates_total', 'performance_target', 'renderer', 'pizza', 'scene_adapter', 'presentation_path']}
            values['sample'] = int(f.stem.split('-')[1])
            statuses.append(values)
        result['statuses'] = statuses
        if result['simulation']:
            last_tick = result['simulation']['windows'][-1]['last_tick']
            result['before_export'] = before_export(result, last_tick)
    print(json.dumps(result, indent=2))
if __name__ == '__main__':
    main()
