#!/usr/bin/env python3
"""Report Clock control panels whose buttons overlap, which hides whichever one the UI paints first."""
import argparse
import re
from pathlib import Path

PANELS={'ingame-clock':'ingame-clock-visible','launcher-clock':'launcher-scenario == "clock"'}
WIDTHS=(480,800,1280)
CHILD=re.compile(r'^\s*(?:if [^:]+: )?(MenuButton|ChoiceRow)\s*\{\s*$')


def block(lines,start):
    """Return the half-open line range of the brace-matched block opening at start."""
    depth=0;end=start
    while end<len(lines):
        depth+=lines[end].count('{')-lines[end].count('}');end+=1
        if depth==0:break
    return start,end


def panels(lines):
    """Yield (name, start, end) for each named panel holding a grid of controls."""
    for name,marker in PANELS.items():
        for index,line in enumerate(lines):
            if marker not in line or not line.rstrip().endswith('{') or 'if ' not in line:
                continue
            start,end=block(lines,index)
            if sum(bool(CHILD.match(l)) for l in lines[start:end])>=10:
                yield name,start,end


def measure(expression,width):
    """Evaluate the px arithmetic these panels use, or None when it is conditional."""
    try:
        return float(eval(expression.replace('parent.width','W').replace('px',''),{'__builtins__':{}},{'W':width}))
    except Exception:
        return None


def boxes(lines,start,end,width):
    """Return every measurable control in the panel as (label, x, y, width, height, line)."""
    result=[];index=start
    while index<end:
        if not CHILD.match(lines[index]):
            index+=1;continue
        open_at,close_at=block(lines,index);body='\n'.join(lines[open_at:close_at])
        read=lambda name:(lambda m:m.group(1).strip() if m else '')(re.search(r'\n\s*%s:\s*([^;]+);'%name,body))
        geometry=[measure(read(name),width) for name in ('x','y','width','height')]
        label=(re.search(r'\n\s*text:\s*"([^"\\]*)',body) or re.search(r'\n\s*label:\s*"?([^";]*)',body))
        if None not in geometry:
            result.append(((label.group(1).split('\\n')[0].strip() if label else '?'),*geometry,open_at+1))
        index=close_at
    return result


def overlaps(items):
    """Return every pair of controls sharing screen area."""
    found=[]
    for first in range(len(items)):
        for second in range(first+1,len(items)):
            a,b=items[first],items[second]
            if a[1]<b[1]+b[3] and b[1]<a[1]+a[3] and a[2]<b[2]+b[4] and b[2]<a[2]+a[4]:
                found.append((a,b))
    return found


def audit(path,widths=WIDTHS):
    """Return {panel: [(width, first, second)]} for every overlap found."""
    lines=Path(path).read_text().split('\n');result={}
    for name,start,end in panels(lines):
        result[name]=[(width,a,b) for width in widths for a,b in overlaps(boxes(lines,start,end,width))]
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--slint',type=Path,default=Path(__file__).parents[1]/'crates/engine-client/ui/main.slint')
    args=parser.parse_args()
    report=audit(args.slint)
    assert report, 'no clock control panels were found, so nothing was audited'
    for name,found in sorted(report.items()):
        print('%s: %d overlap(s)'%(name,len(found)))
        for width,a,b in found:
            print('  at parent.width=%d: %s (line %d) over %s (line %d)'%(width,a[0],a[5],b[0],b[5]))
    assert not any(report.values()), 'clock controls overlap'
    print('every clock control has its own place at widths %s'%(', '.join(str(w) for w in WIDTHS)))


if __name__=='__main__':
    main()
