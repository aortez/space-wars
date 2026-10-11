#!/usr/bin/env python3
"""Read-only warning coverage in ordinary native transfers of frozen controls."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import gzip
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('response',Path(__file__).with_name('validate-projectile-response.py'))
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)


def eligible(e,probe):
    p=e['pilot'];m=e['mission']
    return (m['goal']=='transfer' and m['target'] is not None and e['capture'] is None
        and not probe['recovery_active'] and not (e['match_context'] or {}).get('finished',False)
        and p['controls_armed'] and p['queries_ready'] and p['ship_available'] and p['ship_form']=='ship'
        and isinstance(p['location'],dict) and 'aboard' in p['location']
        and p['landing']['phase']=='flying' and p['landing']['supported_feet']==0)


def contact_window(episode,contacts,identities):
    spawn=episode['spawn_tick'];same=[c for c in contacts if c['source']=='cannon' and c['spawn_tick']==spawn
                                   and c['tick']>=episode['first_tick']]
    if identities[spawn]!={episode['id']}:
        return dict(ambiguous=True,matching_launch_receipts=same)
    within=[c for c in same if c['tick']<=episode['last_tick']+120]
    first=within[0] if within else None
    return dict(ambiguous=False,within_window=within,later=[c for c in same if c not in within],
                first_warning_lead_ticks=first['tick']-episode['first_tick'] if first else None)


def survey(case,run):
    root=R.root_of(run);seat=case['item']['seat'];episodes=[];active={};identities={};contacts=[]
    probe=iter(R.rows(root/'projectile-response.jsonl'));evidence=iter(R.rows(root/'capture-evidence.jsonl'))
    eligible_ticks=warning_ticks=overlap_ticks=post_escape_ticks=0;first=None;maximum_contact_increment=0
    previous_contact_count=None;last_contact=None
    for dr in R.rows(root/'projectiles.jsonl'):
        e=next(evidence);assert e['seat']==dr['seat'] and e['pilot']['tick']==dr['tick']
        d=dr['diagnostic']
        if d:
            for p in d['projectiles']:identities.setdefault(p['spawn_tick'],set()).add(p['id'])
        if dr['seat']!=seat:continue
        pr=next(probe);tick=dr['tick'];assert pr['seat']==seat and pr['tick']==tick
        damage=pr['damage'];count=damage['debris_contacts']
        if previous_contact_count is not None:maximum_contact_increment=max(maximum_contact_increment,count-previous_contact_count)
        previous_contact_count=count
        if damage['last_contact_tick'] is not None and damage['last_contact_tick']!=last_contact:
            contacts.append(dict(tick=damage['last_contact_tick'],source=damage['last_contact_source'],
                                 spawn_tick=damage['last_contact_spawn_tick'],damage=damage,pilot=e['pilot']))
            last_contact=damage['last_contact_tick']
        if not eligible(e,pr):active={};continue
        eligible_ticks+=1;post_escape_ticks+=int(pr['ready'] is not None)
        assert d is not None
        flagged=[]
        for p in d['projectiles']:
            screen=R.P.linear_approach(p['relative_position'],p['relative_velocity'],d['observer_radius']+p['collision_radius'])
            if screen['entry_seconds'] is None:continue
            flagged.append((p,screen))
        if flagged:warning_ticks+=1
        if any(screen['entry_seconds']==0 for p,screen in flagged):overlap_ticks+=1
        current={}
        for p,screen in flagged:
            key=(p['id'],e['pilot']['vehicle'],e['mission']['goal_since'],e['mission']['target'])
            episode=active.get(key)
            if episode is None:
                episode=dict(id=p['id'],spawn_tick=p['spawn_tick'],owner=p['owner'],
                    own=p['owner']==e['pilot']['owner'],vehicle=e['pilot']['vehicle'],destination=e['mission']['target'],
                    goal_since=e['mission']['goal_since'],first_tick=tick,last_tick=tick,ticks=0,
                    first_entry_seconds=screen['entry_seconds'],first_range=screen['range'],
                    post_escape=pr['ready'] is not None)
                episodes.append(episode)
            episode['last_tick']=tick;episode['ticks']+=1;current[key]=episode
        active=current
        if first is None and flagged:
            selected=R.threat(d)
            first=dict(tick=tick,threat=selected,evidence=e,diagnostic=d,probe=pr)
    final=next(probe);assert next(probe,None) is None and next(evidence,None) is None
    damage=final['damage']
    if previous_contact_count is not None:maximum_contact_increment=max(maximum_contact_increment,damage['debris_contacts']-previous_contact_count)
    if damage['last_contact_tick'] is not None and damage['last_contact_tick']!=last_contact:
        contacts.append(dict(tick=damage['last_contact_tick'],source=damage['last_contact_source'],
                             spawn_tick=damage['last_contact_spawn_tick'],damage=damage,pilot=None))
    for ep in episodes:ep['contact']=contact_window(ep,contacts,identities)
    summary=dict(seat=seat,group=case['group'],eligible_ticks=eligible_ticks,post_escape_ticks=post_escape_ticks,
                 warning_ticks=warning_ticks,overlap_ticks=overlap_ticks,episodes=len(episodes),
                 own_episodes=sum(e['own'] for e in episodes),maximum_contact_increment=maximum_contact_increment,
                 first_warning=None if first is None else dict(tick=first['tick'],threat=first['threat'],
                     owner=next(p['owner'] for p in first['diagnostic']['projectiles'] if p['id']==first['threat']['id']),
                     post_escape=first['probe']['ready'] is not None))
    return summary,dict(first_warning=first,episodes=episodes,contact_receipts=contacts)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();s=json.loads(args.summary.read_text());assert s['complete']
    archive=args.out.with_suffix(args.out.suffix+'.gz');assert not args.out.exists() and not archive.exists()
    raw={}
    for case in s['cases']:
        run=s['runs'][case['key']+'-observe']
        for name in ['capture-evidence.jsonl','projectiles.jsonl','projectile-response.jsonl']:
            path=R.root_of(run)/name;h=run['hashes'][name];assert R.digest(path)==h;raw[str(path)]=h
    records={};evidence={}
    with ThreadPoolExecutor(max_workers=2) as pool:
        futures=[pool.submit(survey,c,s['runs'][c['key']+'-observe']) for c in s['cases']]
        for c,f in zip(s['cases'],futures):
            records[c['key']],evidence[c['key']]=f.result()
            print(c['key']+': surveyed',flush=True)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(dict(schema=1,runs=evidence),sort_keys=True,allow_nan=False)+'\n').encode(),mtime=0))
    result=dict(schema=1,complete=True,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
                source=dict(path=str(args.summary),sha256=R.digest(args.summary)),surveyor_sha256=R.digest(__file__),
                archive=dict(path=str(archive),sha256=R.digest(archive)),raw_files=raw,runs=records)
    with args.out.open('x') as stream:stream.write(json.dumps(result,indent=2,allow_nan=False)+'\n')


if __name__=='__main__':main()
