#!/usr/bin/env python3
"""Archive every frozen response trial and its full-match physical follow-through."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from itertools import chain
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('responses', Path(__file__).with_name('validate-projectile-response.py'))
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


def receipts(changes, final, start):
    contacts, damage = {}, {}
    for d in [r['damage'] for r in changes] + [final]:
        tick = d['last_contact_tick']
        if tick is not None and tick >= start:
            contacts[tick] = dict(tick=tick, source=d['last_contact_source'],
                                  spawn_tick=d['last_contact_spawn_tick'])
        tick = d['last_damage_tick']
        if tick is not None and tick >= start and tick not in damage:
            damage[tick] = dict(tick=tick, source=d['last_source'],
                                amount=d['last_damage_percent'], ship_lost=d['last_ship_lost'])
    return list(contacts.values()), list(damage.values())


def ship_losses(pilots, damage_by_tick):
    """Destroyed ships keep form='ship' when their pilot is already on foot."""
    losses, previous = [], None
    for p in pilots:
        if previous is not None and p['recovery']['ships_lost'] > previous['recovery']['ships_lost']:
            assert p['recovery']['ships_lost'] == previous['recovery']['ships_lost']+1
            losses.append(dict(tick=p['tick'], pilot=p, previous_pilot=previous,
                               damage=damage_by_tick[p['tick']]))
        previous = p
    return losses


def analyze(entry, run):
    root = R.root_of(run)
    report = json.loads((root/'report.json').read_text())
    seat = run['item']['seat']
    visits = run['visits'][str(seat)]
    physical = {field:sum(v['physical'][field] is not None for v in visits)
                for field in ['landed', 'exited', 'claimed', 'boarded', 'departed']}
    summary = dict(mode=entry['mode'], source=entry['source'], elapsed_ticks=report['elapsed_ticks'],
                   physical=physical, round=report['round'],
                   escape_travel=report['missions'][seat]['escape_travel'],
                   allocation=run['allocation'], diagnostics=run['projectiles'],
                   parity=run.get('parity'))
    evidence = dict(report_without_samples_or_events={k:v for k,v in report.items() if k not in ('samples','events')},
                    report_checkpoints=report['samples'][-1:], visits=run['visits'], visit_audit=run['visit_audit'])
    if entry['mode'] == 'none':
        return summary, evidence
    response = run['response']; a = response['attempt']
    assert a is not None
    start = a['started_tick']; spawn = a['threat']['spawn_tick']; identity = a['threat']['id']
    witness = json.loads((root/'projectile-response-witnesses.json').read_text())
    damage_by_tick = {r['tick']:r['damage'] for r in response['damage_changes']}
    damage_by_tick[report['elapsed_ticks']] = witness['final']['damage']
    pilots = (r['pilot'] for r in R.rows(root/'capture-evidence.jsonl')
              if r['seat']==seat and r['pilot']['tick']>=start)
    losses = ship_losses(chain(pilots,[report['final_pilots'][seat]]),damage_by_tick)
    first_loss = losses[0] if losses else None
    if response['first_ship_loss'] is not None:
        assert first_loss is not None and first_loss['tick'] == response['first_ship_loss']['tick']
    ticks = {start, a['finished_tick'], report['elapsed_ticks'],
             report['missions'][seat]['escape_travel']['last']['finished_tick']}
    ticks.update(loss['tick'] for loss in losses)
    samples = {min(range(len(report['samples'])),
                   key=lambda i:abs(report['samples'][i]['pilots'][seat]['tick']-tick))
               for tick in ticks if tick is not None}
    evidence['report_checkpoints'] = [report['samples'][i] for i in sorted(samples)]
    contacts, damage = receipts(response['damage_changes'], witness['final']['damage'], start)
    track, same_launch_ids = [], set()
    for row in R.rows(root/'projectiles.jsonl'):
        if row['tick'] < start or row['diagnostic'] is None:
            continue
        d = row['diagnostic']
        for p in d['projectiles']:
            if p['spawn_tick'] == spawn:
                same_launch_ids.add(p['id'])
            if p['id'] == identity:
                assert p['spawn_tick'] == spawn
                track.append(dict(tick=row['tick'], seat=row['seat'], projectile=p,
                                  observer=d['observer'], vehicle=d['vehicle'], ship_form=d['ship_form']))
    assert same_launch_ids == {identity}, 'ambiguous observed launch metadata'
    after_visits = [v for v in visits if v['selected_tick'] >= a['source']['started_tick']]
    summary.update(attempt=a, first_action_change=response['first_action_change'],
                   exact_prefix_rows=response['exact_prefix_rows'], first_ship_loss=first_loss,
                   ship_losses=losses, final_recovery=report['final_pilots'][seat]['recovery'],
                   contact_receipts=contacts, damage_receipts=damage,
                   triggered_projectile_contacts=[c for c in contacts if c['source']=='cannon' and c['spawn_tick']==spawn],
                   triggered_projectile_observed_ids=sorted(same_launch_ids),
                   post_transfer_visits=after_visits,
                   post_transfer_claims=sum(v['physical']['claimed'] is not None for v in after_visits),
                   post_transfer_departures=sum(v['physical']['departed'] is not None for v in after_visits))
    evidence.update(response_witnesses=witness, triggered_projectile_track=track)
    return summary, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    s = json.loads(args.summary.read_text()); assert s['complete']
    assert R.digest(s['binary']['path']) == s['binary']['sha256']
    assert R.digest(s['prior']['path']) == s['prior']['sha256']
    frozen_runner = subprocess.check_output(['git','show',s['source_commit']+':tools/validate-projectile-response.py'])
    assert hashlib.sha256(frozen_runner).hexdigest() == s['runner_sha256']
    archive = args.out.with_suffix(args.out.suffix+'.gz')
    assert not args.out.exists() and not archive.exists()
    records, evidence, raw = {}, {}, {}
    for entry in s['plan']:
        run = s['runs'][entry['key']]
        assert 'error' not in run
        for name, expected in run['hashes'].items():
            path = R.root_of(run)/name
            assert R.digest(path) == expected
            raw[str(path)] = expected
        records[entry['key']], evidence[entry['key']] = analyze(entry, run)
    payload = dict(schema=1, runner_summary=s, runs=evidence)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(payload, sort_keys=True, allow_nan=False)+'\n').encode(), mtime=0))
    manifest = dict(schema=1, complete=True, source_commit=s['source_commit'], binary=s['binary'],
                    sources={str(args.summary):R.digest(args.summary),s['prior']['path']:s['prior']['sha256']},
                    analyzer_sha256=R.digest(__file__), runner_sha256=R.digest(R.__file__),
                    frozen_runner_sha256=s['runner_sha256'],
                    archive=dict(path=str(archive),sha256=R.digest(archive)), raw_files=raw, runs=records)
    with args.out.open('x') as stream:
        stream.write(json.dumps(manifest,indent=2,allow_nan=False)+'\n')
    print(f'Archived {len(records)} complete trials and {len(raw)} raw file hashes: {args.out}')


if __name__ == '__main__':main()
