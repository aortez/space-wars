#!/usr/bin/env python3
"""Read saved match-guidance training progress without contacting the trainer.

Writes summary.json, fitness.png and an optionally refreshing index.html to a
separate directory. Supports the numeric fitness structs used by the headless
match-guidance experiments, not managed TrainingJob saves. Plotting needs
matplotlib; checkpoint inspection and tests use only the standard library.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import html
import io
import json
import math
import os
from pathlib import Path
import sys
import tempfile
import time


MATCH_ORDER = (
    'retained_completed', 'approach_completed', 'approach_speed_points',
    'approach_position_points', 'retained_position_points',
    'retained_velocity_points', 'spare_ticks',
)
PROTECTED_ORDER = (
    'precision_completed', 'protected_completed', 'additional_completed',
    'approach_speed_points', 'approach_position_points',
    'precision_position_points', 'precision_velocity_points', 'spare_ticks',
)
GRADED_ORDER = PROTECTED_ORDER[:3] + ('brake_progress_points',) + PROTECTED_ORDER[3:]
PROGRESS_FIRST_ORDER = PROTECTED_ORDER[:5] + ('brake_progress_points',) + PROTECTED_ORDER[5:]
REPAIR_ORDER = (
    'approach_completed', 'retained_completed', 'retained_position_points',
    'retained_velocity_points', 'approach_speed_points',
    'approach_position_points', 'spare_ticks',
)
JOINT_ORDER = (
    'focal_completed', 'retained_completed', 'precision_completed',
    'retained_match_completed', 'precision_position_points',
    'precision_velocity_points', 'approach_speed_points',
    'approach_position_points', 'spare_ticks',
)
FOCUS_COMPLETION_ORDER = (
    'protected_completed', 'remaining_completed', 'focal_completed',
    'retained_completed', 'precision_completed', 'retained_match_completed',
)
FOCUS_BROAD_PROGRESS = (
    'precision_position_points', 'precision_velocity_points',
    'approach_speed_points', 'approach_position_points', 'spare_ticks',
)
FOCUS_REMAINING_PROGRESS = ('remaining_speed_points', 'remaining_position_points')
FOCUS_BROAD_ORDER = FOCUS_COMPLETION_ORDER + FOCUS_BROAD_PROGRESS + FOCUS_REMAINING_PROGRESS
FOCUS_TARGETED_ORDER = FOCUS_COMPLETION_ORDER + FOCUS_REMAINING_PROGRESS + FOCUS_BROAD_PROGRESS
# Rust compares these fields in this order. Do not infer selection order
# from a JSON object's key order or invent an aggregate scalar fitness.
KNOWN_ORDERS = (MATCH_ORDER, PROTECTED_ORDER, GRADED_ORDER, PROGRESS_FIRST_ORDER,
                REPAIR_ORDER, JOINT_ORDER, FOCUS_BROAD_ORDER, FOCUS_TARGETED_ORDER)
MAX_CHECKPOINT_BYTES = 64 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def utc(timestamp):
    return datetime.fromtimestamp(timestamp, timezone.utc).isoformat(timespec='seconds')


def quantile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    low, high = math.floor(position), math.ceil(position)
    return ordered[low] + (ordered[high] - ordered[low]) * (position - low)


def fitness_order(run):
    samples = [row['best_fitness'] for row in run['history']]
    samples += [score for score in run['fitness'] if score is not None]
    configuration = run['task']['configuration']
    plan_text = configuration.get('plan_utf8')
    declared = configuration.get('fitness_order')
    if declared is None:
        declared = json.loads(plan_text).get('fitness_order') if plan_text else None
    if not samples and declared is None:
        return ()
    fields = set(samples[0]) if samples else set(declared)
    order = (tuple(declared) if declared is not None else
             next((order for order in KNOWN_ORDERS if set(order) == fields), None))
    require(order in KNOWN_ORDERS and set(order) == fields,
            'unsupported fitness schema or declared order; add its Rust selection order first')
    require(declared is None or tuple(declared) == order,
            'recipe fitness order differs from the supported Rust fitness order')
    return order


def score_key(score, order):
    require(isinstance(score, dict) and set(score) == set(order), 'inconsistent fitness fields')
    require(all(type(value) is int and value >= 0 for value in score.values()),
            'fitness must contain nonnegative integer scores')
    return tuple(score[field] for field in order)


def summarize(document):
    """Validate progress and retain scores in their actual selection order."""
    require(document['version'] == 1, 'unsupported checkpoint version')
    run = document['run']
    config, history = run['config'], run['history']
    population, budget = config['population'], config['generations']
    require(type(population) is int and population > 0, 'invalid population size')
    require(type(budget) is int and budget > 0, 'invalid generation budget')
    require(len(history) <= budget, 'history exceeds the generation budget')
    require(len(run['fitness']) == population and len(run['population']) == population,
            'current population size differs from the recipe')
    complete = len(history) == budget
    require(run['generation'] == len(history) - int(complete), 'inconsistent generation counter')
    evaluated = sum(score is not None for score in run['fitness'])
    require(evaluated == population if complete else evaluated < population,
            'inconsistent completed-generation boundary')
    order = fitness_order(run)
    for score in run['fitness']:
        if score is not None:
            score_key(score, order)

    rows = []
    best_key = None
    best = None
    last_improvement = None
    for index, entry in enumerate(history):
        require(entry['generation'] == index, 'history has a missing or repeated generation')
        scores, ids = entry['fitness'], entry['population_ids']
        require(len(scores) == population and len(ids) == population,
                'history population size differs from the recipe')
        require(all(isinstance(genome, str) and genome for genome in ids), 'invalid genome ID')
        keys = [score_key(score, order) for score in scores]
        winner = min(range(population), key=lambda slot: (
            tuple(-value for value in keys[slot]), ids[slot], slot))
        require(entry['best_id'] == ids[winner] and entry['best_fitness'] == scores[winner],
                'saved winner differs from the ordered population fitness')
        row = {
            'generation': index + 1,
            'selected_genome': entry['best_id'],
            'selected_fitness': entry['best_fitness'],
            'population': {
                field: dict(zip(('q25', 'median', 'q75'),
                                (quantile([score[field] for score in scores], q)
                                 for q in (0.25, 0.5, 0.75))))
                for field in order
            },
        }
        rows.append(row)
        if best_key is None or keys[winner] > best_key:
            best_key = keys[winner]
            best = row
            last_improvement = index + 1
        elif keys[winner] == best_key and entry['best_id'] < best['selected_genome']:
            best = row
    if complete:
        require(run['fitness'] == history[-1]['fitness'],
                'final population scores differ from final history')
    return {
        'schema': 'guidance_training_observation_v1',
        'runtime': run['runtime'],
        'task': run['task']['id'],
        'contract': run['task']['contract'],
        'task_configuration_sha256': hashlib.sha256(json.dumps(
            run['task']['configuration'], sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
        'config': config,
        'complete': complete,
        'completed_generations': len(history),
        'pending_generation': None if complete else len(history) + 1,
        'pending_candidates_evaluated': 0 if complete else evaluated,
        'saved_candidate_evaluations': len(history) * population + (0 if complete else evaluated),
        'fitness_order': list(order),
        'last_improvement_generation': last_improvement,
        'generations_since_improvement': None if best is None else len(history) - last_improvement,
        'best_so_far': best,
        'history': rows,
        'validation': None,
    }


def read_checkpoint(path):
    # Read and stat the same opened file: the trainer replaces checkpoints atomically.
    with path.open('rb') as source:
        payload = source.read(MAX_CHECKPOINT_BYTES + 1)
        modified = os.fstat(source.fileno()).st_mtime
    require(len(payload) <= MAX_CHECKPOINT_BYTES, 'checkpoint exceeds the 64 MiB observer limit')
    document = json.loads(payload)
    summary = summarize(document)
    summary.update(
        source=str(path.resolve()),
        source_sha256=hashlib.sha256(payload).hexdigest(),
        source_modified_utc=utc(modified),
        observed_utc=utc(time.time()),
        declared_run_sha256=document['sha256'],
        declared_run_sha256_verified=False,
    )
    return summary


def plot(summary):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.lines import Line2D
    from matplotlib.patches import Patch
    from matplotlib.ticker import MaxNLocator

    fields = summary['fitness_order']
    # The primary success/reward terms stay prominent; the JSON retains all terms.
    shown = [field for field in fields if field.endswith('_completed') or field == 'brake_progress_points']
    count = max(1, len(shown))
    columns = min(2, count)
    fig, axes = plt.subplots(math.ceil(count / columns), columns, figsize=(12, 7.4),
                             squeeze=False)
    rows = summary['history']
    generations = [row['generation'] for row in rows]
    for axis, field in zip(axes.flat, shown):
        axis.plot(generations, [row['selected_fitness'][field] for row in rows],
                  color='#bc4e0b', lw=2, marker='.', zorder=3)
        axis.plot(generations, [row['population'][field]['median'] for row in rows],
                  color='#24758c', lw=1.7, marker='.')
        axis.fill_between(generations, [row['population'][field]['q25'] for row in rows],
                          [row['population'][field]['q75'] for row in rows],
                          color='#24758c', alpha=0.16)
        axis.set_title(field.replace('_', ' ').capitalize(), loc='left', fontsize=11)
        axis.set_xlabel('Completed generation')
        axis.set_xlim(0.5, max(2, len(rows)) + 0.5)
        axis.xaxis.set_major_locator(MaxNLocator(integer=True))
        axis.grid(alpha=0.18)
        if not rows:
            axis.text(0.5, 0.5, 'No completed generation yet', ha='center', transform=axis.transAxes)
    for axis in list(axes.flat)[len(shown):]:
        axis.axis('off')
    if not shown:
        axes.flat[0].text(0.5, 0.5, 'No completed generation yet', ha='center', va='center')
    progress = f"{summary['completed_generations']}/{summary['config']['generations']} generations saved"
    state = 'Complete' if summary['complete'] else (
        f"Next population: {summary['pending_candidates_evaluated']}/"
        f"{summary['config']['population']} candidates evaluated")
    fig.suptitle(f"Saved match-guidance training progress\n{progress}\n{state}", fontsize=13, y=0.97)
    fig.legend(handles=[Line2D([], [], color='#bc4e0b', lw=2, label='Selected winner'),
                        Line2D([], [], color='#24758c', lw=1.7, label='Population median'),
                        Patch(color='#24758c', alpha=0.16, label='Population middle 50%')],
               loc='lower center', ncol=3, bbox_to_anchor=(0.5, 0.07), frameon=False)
    fig.text(0.5, 0.04, 'Training scores only · selection uses all ordered fitness terms · process liveness unknown',
             ha='center', fontsize=9, color='#465563')
    fig.text(0.5, 0.018, f"{summary['task']} · checkpoint {summary['source_sha256'][:12]}",
             ha='center', fontsize=8, color='#465563')
    fig.subplots_adjust(top=0.8, bottom=0.2, hspace=0.45, wspace=0.2)
    out = io.BytesIO()
    fig.savefig(out, format='png', dpi=140)
    plt.close(fig)
    return out.getvalue()


def page(summary, png, refresh):
    escape = html.escape
    best = summary['best_so_far']
    improvement = 'No completed generation yet.' if best is None else (
        f"Last ordered-fitness improvement: generation {summary['last_improvement_generation']}; "
        f"{summary['generations_since_improvement']} completed generations since then.")
    identity = '' if best is None else (
        f"<p>Best-so-far genome: <code>{escape(best['selected_genome'])}</code> "
        f"(generation {best['generation']}).</p>")
    return html_document(f"""
<h1>Saved training progress</h1>
<p><strong>{escape(summary['task'])}</strong></p>
<p>Checkpoint: {escape(summary['source_modified_utc'])}. Read: {escape(summary['observed_utc'])}.</p>
<p>{improvement} {summary['saved_candidate_evaluations']:,} candidate evaluations saved.</p>
{identity}
<img alt="Fitness history for completed generations" src="data:image/png;base64,{base64.b64encode(png).decode()}">
<p>Scores are from training. Validation is not present in this checkpoint. An incomplete checkpoint
does not establish whether the trainer is running. Refreshing this page does not run evaluations.</p>
<p>All ordered terms and provenance: <a href="summary.json">summary.json</a>.
Export: <a href="fitness.png">fitness.png</a>.</p>
<p>Source: <code>{escape(summary['source'])}</code><br>
File SHA-256: <code>{summary['source_sha256']}</code></p>
""", refresh)


def html_document(body, refresh):
    meta = f'<meta http-equiv="refresh" content="{refresh:g}">' if refresh else ''
    return f"""<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">{meta}
<title>Saved training progress</title>
<style>body{{font:16px system-ui,sans-serif;max-width:1200px;margin:24px auto;padding:0 20px;
color:#253440;background:#fafcfd}}img{{width:100%;height:auto}}code{{overflow-wrap:anywhere}}
p{{line-height:1.5}}a{{color:#176383}}</style><body>{body}</body></html>"""


def atomic_write(path, payload):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f'.{path.name}.', delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(payload)
            output.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def observe(checkpoint, output, watch=0):
    checkpoint, output = checkpoint.resolve(), output.resolve()
    require(not output.is_relative_to(checkpoint.parent),
            'output must be outside the checkpoint directory; keep experiment packages unchanged')
    output.mkdir(parents=True, exist_ok=True)
    previous_hash, png = None, None
    while True:
        try:
            summary = read_checkpoint(checkpoint)
            if summary['source_sha256'] != previous_hash:
                png = plot(summary)
                atomic_write(output / 'fitness.png', png)
                print(f"{summary['observed_utc']}: {summary['completed_generations']} generations; "
                      f"{summary['saved_candidate_evaluations']} candidate evaluations saved", flush=True)
            atomic_write(output / 'summary.json', (json.dumps(summary, indent=2) + '\n').encode())
            atomic_write(output / 'index.html', page(summary, png, 0 if summary['complete'] else watch).encode())
            previous_hash = summary['source_sha256']
            if not watch or summary['complete']:
                return
        except (OSError, ValueError, KeyError, TypeError) as error:
            message = f'{type(error).__name__}: {error}'
            atomic_write(output / 'index.html', html_document(
                f'<h1>Checkpoint unavailable</h1><p>{html.escape(message)}</p>'
                f'<p>Read attempt: {utc(time.time())}. Any previous exports are stale.</p>', watch).encode())
            print(message, file=sys.stderr, flush=True)
            if not watch:
                raise
        time.sleep(watch)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('checkpoint', type=Path)
    parser.add_argument('--output', type=Path, required=True,
                        help='separate observation directory outside the experiment package')
    parser.add_argument('--watch', type=float, default=0, metavar='SECONDS',
                        help='refresh until complete or interrupted (at least 1 second)')
    args = parser.parse_args()
    if not math.isfinite(args.watch) or (args.watch != 0 and args.watch < 1):
        parser.error('--watch must be zero or at least 1 second')
    try:
        observe(args.checkpoint, args.output, args.watch)
    except KeyboardInterrupt:
        print('Observer stopped; training was not contacted.', file=sys.stderr)
    except (OSError, ValueError, KeyError, TypeError, ImportError) as error:
        parser.exit(1, f'{type(error).__name__}: {error}\n')


if __name__ == '__main__':
    main()
