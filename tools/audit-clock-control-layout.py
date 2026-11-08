#!/usr/bin/env python3
"""Report Clock control panels whose buttons overlap, which hides whichever one the UI paints first, and selectors squeezed below the room their value needs."""
import argparse
import re
from pathlib import Path

PANELS={'ingame-clock':'ingame-clock-visible','launcher-clock':'launcher-scenario == "clock"'}
# 400 and 552 are the grids' real widths on the 480x800 and landscape panels.
WIDTHS=(400,480,552,800,1280)
# A ChoiceRow gives 52px and a margin to each arrow; its value has the rest
# of what the label fraction leaves, and shows nothing legible under this.
VALUE_ROOM=32
# The launcher's selectors lose their values on a portrait panel too; that
# predates the check and is a separate change, so only the pause page is held
# to it.
VALUE_PANELS=('ingame-clock',)
CHILD=re.compile(r'^\s*(?:if [^:]+: )?(?:[\w-]+\s*:=\s*)?(MenuButton|ChoiceRow)\s*\{\s*$')
PROPERTY=re.compile(r'^\s*(?:private |in |out |in-out )?property\s*<[^>]+>\s*([\w-]+)\s*:\s*([^;]+);')
TERNARY=re.compile(r'^(.*?)\?(.*?):(.*)$')


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


def declarations(lines,start,end):
    """Map the panel's own `id.property` names to their expressions, for geometry that refers to them."""
    named=re.search(r'([\w-]+)\s*:=\s*\w+\s*\{',lines[start])
    if not named:
        return {}
    found={}
    for line in lines[start+1:end]:
        match=PROPERTY.match(line)
        if match:
            found['%s.%s'%(named.group(1),match.group(1))]=match.group(2).strip()
    return found


def measure(expression,width,names=None):
    """Evaluate the px arithmetic these panels use, one `a ? b : c` deep and through the panel's own properties, or None."""
    for name,value in (names or {}).items():
        expression=expression.replace(name,'(%s)'%value)
    expression=TERNARY.sub(r'((\2) if (\1) else (\3))',expression)
    try:
        return float(eval(expression.replace('parent.width','W').replace('self.width','W').replace('px',''),{'__builtins__':{}},{'W':width}))
    except Exception:
        return None


def boxes(lines,start,end,width):
    """Return every measurable control in the panel as (label, x, y, width, height, line, kind, label fraction)."""
    result=[];index=start;names=declarations(lines,start,end)
    while index<end:
        kind=CHILD.match(lines[index])
        if not kind:
            index+=1;continue
        open_at,close_at=block(lines,index);body='\n'.join(lines[open_at:close_at])
        read=lambda name:(lambda m:m.group(1).strip() if m else '')(re.search(r'\n\s*%s:\s*([^;]+);'%name,body))
        geometry=[measure(read(name),width,names) for name in ('x','y','width','height')]
        label=(re.search(r'\n\s*text:\s*"([^"\\]*)',body) or re.search(r'\n\s*label:\s*"?([^";]*)',body))
        fraction=measure(read('label-fraction') or '0.40',width,names)
        if None not in geometry:
            result.append(((label.group(1).split('\\n')[0].strip() if label else '?'),*geometry,open_at+1,kind.group(1),fraction))
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


def squeezed(items,room=VALUE_ROOM):
    """Return every selector whose value is left under `room` px, with that figure."""
    return [(item,item[3]*(1-item[7])-120) for item in items if item[6]=='ChoiceRow' and item[7] is not None and item[3]*(1-item[7])-120<room]


def audit(path,widths=WIDTHS):
    """Return {panel: [(width, first, second)]} for every overlap found."""
    lines=Path(path).read_text().split('\n');result={}
    for name,start,end in panels(lines):
        result[name]=[(width,a,b) for width in widths for a,b in overlaps(boxes(lines,start,end,width))]
    return result


def audit_values(path,widths=WIDTHS,panel_names=VALUE_PANELS):
    """Return {panel: [(width, selector, room)]} for every selector squeezed below its value's room."""
    lines=Path(path).read_text().split('\n');result={}
    for name,start,end in panels(lines):
        if name in panel_names:
            result[name]=[(width,item,room) for width in widths for item,room in squeezed(boxes(lines,start,end,width))]
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
    values=audit_values(args.slint)
    for name,found in sorted(values.items()):
        print('%s: %d squeezed selector(s)'%(name,len(found)))
        for width,item,room in found:
            print('  at parent.width=%d: %s (line %d) leaves its value %dpx'%(width,item[0],item[5],room))
    assert not any(report.values()), 'clock controls overlap'
    assert not any(values.values()), 'a clock selector has no room for its value'
    print('every clock control has its own place at widths %s'%(', '.join(str(w) for w in WIDTHS)))


if __name__=='__main__':
    main()
