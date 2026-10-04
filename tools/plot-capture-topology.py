#!/usr/bin/env python3
"""Draw the measured site-0 graph in the failed and successful capture replays."""
import argparse
import importlib.util
import json
import math
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from matplotlib.lines import Line2D

plt.rcParams['svg.hashsalt'] = 'capture-approach-topology-v1'

spec = importlib.util.spec_from_file_location('topology', Path(__file__).with_name('probe-capture-topology.py'))
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)


def plot(study, out):
    summary = json.loads((study/'summary.json').read_text())
    assert summary['complete']
    fig, axes = plt.subplots(1, 2, figsize=(10, 5.4))
    for axis, (name, tick, title) in zip(axes, [
        ('failure-cover-on', 3930, 'Failed approach\nFlag beyond the ship'),
        ('successful-control', 3510, 'Successful control\nFlag on the exit side'),
    ]):
        path = study/name/'cover-probe.json'
        assert T.F.E.digest(path) == summary['runs'][name]['hashes'][path.name]
        row = next(r for r in json.loads(path.read_text())['rows'] if r['world_tick'] == tick)
        topology = row['topology']
        site = next(s for s in topology['sites'] if s['site']['bearing'] == 0)
        nodes = {n['id']: (n['position']['x'], n['position']['y']) for n in topology['base']['nodes']
                 if n['id'] not in site['removed_nodes']}
        edges = [e for i, e in enumerate(topology['base']['edges']) if i not in site['removed_edges']]
        adjacency = {n: set() for n in nodes}
        for edge in edges:
            adjacency[edge['from']].add(edge['to'])
        start = site['with_ship']['route']['outbound']['start_node']
        reached = T.reachable(start, adjacency)
        for connected, color in [(False, '#bcc5ce'), (True, '#168b8c')]:
            segments = [[nodes[e['from']], nodes[e['to']]] for e in edges
                        if (e['from'] in reached and e['to'] in reached) == connected]
            axis.add_collection(LineCollection(segments, colors=color, linewidths=3))
        pilot = row['observation']['local']['combat']['recovery']['flight']['pilot']
        frame = pilot['planet']['motion']
        def local(v):
            x, y = v['x']-frame['position']['x'], v['y']-frame['position']['y']
            c, s = math.cos(frame['angle']), math.sin(frame['angle'])
            return x*c+y*s, -x*s+y*c
        physical_site = next(s for s in pilot['sites'] if s['id'] == site['site'])
        opponent = row['observation']['local']['combat']['target']['motion']['position']
        flag = topology['objective']['position']
        markers = [
            (local(physical_site['vehicle_position']), 'D', '#6850a1', 'Proposed ship', (-55, 75)),
            (local(opponent), 's', '#435269', 'Other ship', (26, 13)),
            (nodes[start], 'o', '#d18122', 'Exit', (27, 66)),
            ((flag['x'], flag['y']), '*', '#c44964', 'Flag', (61, flag['y']-12)),
        ]
        for point, marker, color, label, label_point in markers:
            axis.scatter(*point, marker=marker, c=color, s=100, zorder=4)
            axis.annotate(label, point, xytext=label_point, color=color, fontsize=10,
                          arrowprops=dict(arrowstyle='-', color=color, linewidth=0.8))
        axis.text(0, -6, 'Planet 1\nSheltered landing site 0', ha='center', color='#465563', fontsize=10)
        axis.set(xlim=(-84, 88), ylim=(-80, 85), aspect='equal')
        axis.set_title(title, fontsize=12, pad=8)
        axis.text(0.5, -0.04, f'World tick {tick:,} · planet-local coordinates',
                  transform=axis.transAxes, ha='center', fontsize=9, color='#596777')
        axis.axis('off')
    fig.legend(handles=[Line2D([0], [0], color='#168b8c', lw=3, label='Reachable from the exit'),
                        Line2D([0], [0], color='#bcc5ce', lw=3, label='Other measured ground')],
               loc='lower center', ncol=2, frameon=False, bbox_to_anchor=(0.5, 0.05))
    fig.text(0.5, 0.02, 'Native walk/jump graph with the proposed ship present. Gaps are untraversed; marker sizes are illustrative.',
             ha='center', fontsize=8, color='#596777')
    fig.subplots_adjust(left=0.02, right=0.98, bottom=0.19, top=0.84, wspace=0.05)
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out, metadata={'Date': None})
    fig.savefig(out.with_suffix('.png'), dpi=160)
    plt.close(fig)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    plot(args.study, args.out)
