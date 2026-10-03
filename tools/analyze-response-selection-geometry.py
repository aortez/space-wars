#!/usr/bin/env python3
"""Retain the known source geometries and physical braking regression evidence."""
import argparse
import gzip
import hashlib
import importlib.util
import json
import math
from pathlib import Path

spec = importlib.util.spec_from_file_location('response', Path(__file__).with_name('validate-projectile-response.py'))
R = importlib.util.module_from_spec(spec); spec.loader.exec_module(R)
from projectile_brake_selection import select


def geometry(probe):
    o = probe['observation']; f = o['local']['combat']['recovery']['flight']; p = f['pilot']
    ship = p['ship']; frame = p['planet']['motion']; limits = f['flight']['limits']
    offset = [ship['position'][k]-frame['position'][k] for k in 'xy']
    fv = [frame['velocity']['x']-frame['spin']*offset[1], frame['velocity']['y']+frame['spin']*offset[0]]
    relative = [ship['velocity'][k]-fv[i] for i,k in enumerate('xy')]; speed=math.hypot(*relative)
    brake = [-x/speed*min(speed*limits['brake_gain'],limits['brake_acceleration']) for x in relative]
    q = next(q for q in probe['diagnostic']['projectiles'] if q['id']==probe['attempt']['threat']['id'])
    r = [q['relative_position'][k] for k in 'xy']; distance=math.hypot(*r)
    return dict(frame_velocity=fv, relative_velocity=relative, brake_acceleration=brake,
                brake_toward_projectile=sum(x*y for x,y in zip(brake,r))/distance,
                entry_seconds=probe['attempt']['threat']['entry_seconds'],
                proposed_selection=select(o,probe['diagnostic'],probe['original_actions'],R.P.linear_approach))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();assert not args.out.exists()
    records={};sources={};raw={}
    for name,keys in [('projectile-response',['approach-shared-armed-world1-p1-powered','speed-shared-armed-world1-p1-powered']),
                      ('ordinary-transfer-response',['health-shared-armed-world0-p1-powered','shared-armed-world3-p1-powered'])]:
        path=Path('docs/data')/(name+'-v1.json');m=json.loads(path.read_text());archive=m['archive']['path']
        assert R.digest(archive)==m['archive']['sha256'];a=json.loads(gzip.decompress(Path(archive).read_bytes()))
        sources[str(path)]=R.digest(path);sources[archive]=R.digest(archive)
        for key in keys:
            probe=a['runs'][key+'-observe']['response_witnesses']['rows'][0]['probe']
            modes={}
            for mode in ['observe','brake','left','right']:
                run=m['runs'].get(key+'-'+mode)
                if run is None:continue
                modes[mode]={k:run[k] for k in ['attempt','physical','round','contact_receipts','damage_receipts',
                                               'triggered_projectile_contacts','final_recovery']}
                modes[mode]['ship_losses']=[dict(tick=l['tick'],damage=l['damage'],location=l['previous_pilot']['location'])
                                            for l in run['ship_losses']]
            records[key]=dict(geometry=geometry(probe),source=probe, outcomes=modes)
            if key.startswith('approach-'):
                for mode in ['observe','brake']:
                    run=a['runner_summary']['runs'][key+'-'+mode];p=R.root_of(run)/'capture-evidence.jsonl'
                    assert R.digest(p)==run['hashes'][p.name];raw[str(p)]=run['hashes'][p.name]
                    samples=[]
                    for row in R.rows(p):
                        t=row['pilot']['tick']
                        if t>9175:break
                        if row['seat']==0 and 9160<=t<=9175:
                            samples.append(dict(tick=t,ship=row['pilot']['ship'],health=row['pilot']['ship_health'],actions=row['actions']))
                    records[key]['outcomes'][mode]['precontact_track']=samples
    paths=[Path(__file__),Path(__file__).with_name('projectile_brake_selection.py')]
    out=dict(schema=1,complete=True,sources=sources,raw_files=raw,tools={str(p):R.digest(p) for p in paths},cases=records)
    args.out.write_text(json.dumps(out,indent=2,allow_nan=False)+'\n')
    for k,v in records.items():print(k,v['geometry'])


if __name__=='__main__':main()
