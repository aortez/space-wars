#!/usr/bin/env python3
"""Cut films, clips and an event contact sheet out of a world_model_bot --shots directory."""
import argparse
from fractions import Fraction
import glob
import os
from pathlib import Path
import re
import subprocess
import tempfile

FPS=60.0
TOP_CROP=8
COLUMNS=32
ROWS=27
DOTS={
    'load_bearing':(0,255,0),
    'opaque':(255,0,0),
    'player':(60,130,255),
}
FONTS=[
    '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf',
    '/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf',
]
MODEL=re.compile(r'model-(\d{6})-x\d{5}\.png$')
EVENT=re.compile(r'event-[a-z-]*f(\d+)-x\d+\.png$')


def run(cmd):
    print('$ '+' '.join(str(c) for c in cmd))
    subprocess.run([str(c) for c in cmd],check=True)


def raw(cmd):
    return subprocess.run([str(c) for c in cmd],capture_output=True,check=True).stdout


def pngs(directory,pattern,by_frame=False):
    """Series order matters: ffmpeg's glob sorts by name, so a frame-numbered series
    has to be ordered by the number for the film to run forwards."""
    found=sorted(glob.glob(str(Path(directory)/pattern)))
    if by_frame: return sorted(found,key=lambda p:int(EVENT.search(os.path.basename(p)).group(1)))
    return found


def upscale(scale):
    return 'scale=iw*%d:ih*%d:flags=neighbor'%(scale,scale)


def encode(files,out,fps,scale,crf):
    """Film one numbered PNG series; the glob stays quoted so ffmpeg's demuxer expands it."""
    assert files, 'nothing matched the series behind %s'%out
    directory,leaf=os.path.split(files[0])
    prefix=leaf.split('-')[0]
    run(['ffmpeg','-y','-loglevel','error','-framerate',Fraction(fps).limit_denominator(1000),
         '-pattern_type','glob','-i','%s/%s-*.png'%(directory,prefix),'-vf',upscale(scale),
         '-c:v','libx264','-crf',crf,'-pix_fmt','yuv420p',out])


def concat(files,fps):
    """A listing for a chosen subset, which is how a clip or a sheet picks frames the
    glob cannot name. The demuxer wants a decimal duration; 1/60 reads as garbage."""
    handle=tempfile.NamedTemporaryFile('w',suffix='.txt',delete=False)
    handle.write('ffconcat version 1.0\n')
    for path in files: handle.write("file '%s'\nduration %.6f\n"%(os.path.abspath(path),1.0/fps))
    handle.write("file '%s'\n"%os.path.abspath(files[-1]))
    handle.close()
    return handle.name


def encoder(out,crf):
    """The sheet is a still and the films are not; naming a still .png and feeding it
    libx264 writes an H.264 stream under a picture's name, which nothing will open."""
    if str(out).endswith('.png'): return ['-c:v','png']
    return ['-c:v','libx264','-crf',crf,'-pix_fmt','yuv420p']


def encode_list(files,out,fps,scale,crf,vf='',extra=()):
    listing=concat(files,fps)
    try:
        run(['ffmpeg','-y','-loglevel','error','-f','concat','-safe','0','-i',listing,'-vf',
             upscale(scale)+vf,'-vsync','cfr','-r',Fraction(fps).limit_denominator(1000),
             *encoder(out,crf),*extra,out])
    finally:
        os.remove(listing)


def clip(models,lo,hi,out,scale,crf,fps):
    """A short film over the emulator frames lo..hi, from the overlay series."""
    # The event stills live in the same directory with names like
    # model-event-stuck-f1802-x898.png, so an unnumbered name is skipped, not fatal.
    picked=[p for p in models if (m:=MODEL.match(os.path.basename(p))) and lo<=int(m.group(1))<=hi]
    assert picked, 'no model frames in %d..%d'%(lo,hi)
    encode_list(picked,out,fps,scale,crf)
    return len(picked)


def sheet(events,out,cols,scale):
    """One picture of every event still in the order they happened, numbered by
    position; the legend that says which number is which file is printed."""
    assert events, 'no event stills, so there is nothing to sheet'
    rows=-(-len(events)//cols)
    vf=''
    if next((f for f in FONTS if os.path.exists(f)),None):
        vf+=(",drawtext=fontfile=%s:text='%%{n}':x=6:y=main_h-30:fontsize=28"%font()
             +":fontcolor=yellow:box=1:boxcolor=black@0.6")
    vf+=',tile=%dx%d:padding=8:color=0x121216'%(cols,rows)
    encode_list(events,out,1.0,scale,24,vf=vf,extra=('-frames:v','1'))
    return rows


def font():
    return next((f for f in FONTS if os.path.exists(f)),None)


def dots(buffer,scale):
    """Count the model's dots in a raw rgb24 frame, at the cell mapping paint_model
    documents: column * 8 + 1 across, row * 8 + the status-bar crop + 1 down."""
    width=256*scale
    counts=dict.fromkeys(DOTS,0)
    for column in range(COLUMNS):
        for row in range(ROWS):
            offset=((row*8+TOP_CROP+1)*scale*width+(column*8+1)*scale)*3
            pixel=buffer[offset:offset+3]
            if len(pixel)<3: continue
            # Tight, because the NES palette is full of browns one channel away from
            # the red dot; a loose bound counts scenery as the model's own opinion.
            best=min(DOTS,key=lambda k:sum((a-b)**2 for a,b in zip(DOTS[k],pixel)))
            if max(abs(int(a)-int(b)) for a,b in zip(DOTS[best],pixel))<=32: counts[best]+=1
    return counts


def png_raw(path,scale):
    return raw(['ffmpeg','-v','error','-i',path,'-vf',upscale(scale),'-f','rawvideo',
                '-pix_fmt','rgb24','-'])


def film_raw(path,index):
    return raw(['ffmpeg','-v','error','-i',path,'-vf','select=eq(n\\,%d)'%index,'-frames:v','1',
                '-f','rawvideo','-pix_fmt','rgb24','-'])


def verify(png,film,index,scale):
    """The overlay has to survive the encoder, or the film is decoration and not
    evidence for anything. x264 is lossy on a one-pixel dot, so a tenth is allowed."""
    before=dots(png_raw(png,scale),scale)
    after=dots(film_raw(film,index),scale)
    print('%s frame %d: dots %s in, %s out'%(os.path.basename(film),index,before,after))
    assert sum(after.values())>0, 'no model dots survived the encode at all'
    for kind,count in before.items():
        assert abs(after[kind]-count)<=max(1,count//10), \
            '%s dots did not survive the encode: %d to %d'%(kind,count,after[kind])
    return after


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('shots',type=Path,help="a directory written by world_model_bot --shots DIR --overlay")
    parser.add_argument('--out',type=Path,help='where to write; defaults to the shots directory')
    parser.add_argument('--name',help='output basename; defaults to the shots directory name')
    parser.add_argument('--every',type=int,default=2,help="the --every the bot ran with, so the plain film keeps real time")
    parser.add_argument('--fps',type=float,default=FPS,help='the emulator rate, 60 for NTSC')
    parser.add_argument('--scale',type=int,default=2,help='integer upscale, nearest neighbour')
    parser.add_argument('--crf',type=int,default=26,help='x264 quality; lower is larger')
    parser.add_argument('--clip',action='append',default=[],metavar='LO-HI',
                        help='overlay clip over these emulator frames, repeatable')
    parser.add_argument('--sheet-cols',type=int,default=4,help='event stills per row on the contact sheet')
    parser.add_argument('--no-verify',action='store_true',help='skip decoding a film frame back')
    args=parser.parse_args()
    shots=Path(args.shots)
    assert shots.is_dir(), '%s is not a directory; run world_model_bot with --shots first'%shots
    out=args.out or shots
    out.mkdir(parents=True,exist_ok=True)
    name=args.name or shots.name
    frames=pngs(shots,'frame-*.png')
    models=pngs(shots,'model-*.png')
    events=pngs(shots,'event-*.png',by_frame=True)
    assert frames or models, '%s holds no frame-*.png or model-*.png'%shots
    written=[]
    if frames:
        # The bot thinned this series to every Nth frame, so the film runs at fps/N
        # to stay in real time; the overlay series is every frame and does not.
        encode(frames,out/('%s.mp4'%name),args.fps/args.every,args.scale,args.crf)
        written.append(out/('%s.mp4'%name))
    if models:
        encode(models,out/('%s-model.mp4'%name),args.fps,args.scale,args.crf)
        written.append(out/('%s-model.mp4'%name))
    for spec in args.clip:
        lo,hi=(int(part) for part in spec.split('-'))
        label='%s-clip-%d-%d'%(name,lo,hi)
        count=clip(models,lo,hi,out/('%s.mp4'%label),args.scale,min(args.crf,24),args.fps)
        print('%s: %d frames, %.1fs'%(label,count,count/args.fps))
        written.append(out/('%s.mp4'%label))
    if events:
        sheet(events,out/('%s-events.png'%name),args.sheet_cols,args.scale)
        written.append(out/('%s-events.png'%name))
        print('contact sheet legend, left to right then top to bottom:')
        for index,path in enumerate(events): print('  %2d  %s'%(index,os.path.basename(path)))
    if models and not args.no_verify:
        index=len(models)-2 if len(models)>2 else 0
        verify(models[index],out/('%s-model.mp4'%name),index,args.scale)
    print('wrote %d file(s) to %s'%(len(written),out))


if __name__=='__main__':
    main()
