#!/usr/bin/env python3
"""SUPERSEDED (2026-10-14): the first, procedural pass at the Astra costumes.
Kept only so v1 can be regenerated; running it overwrites sets/astra_*.py and
catalog.txt with the old art. The current generator is astra_studio.py.

Author Astra costumes in Hero5's native pixel grid, then export MGR/PNG.

This is source art, not a resizer for the concept PNGs. Shapes are rasterized
on an integer grid; limb silhouettes follow the archive's animation pieces.
All opaque pixels are newly assigned from each costume's small palette.
Run from the repository root: python costumes/astra_native.py
"""
import json
import math
import pprint
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
sys.path.insert(0, str(ROOT / 'costumes'))
import mgr
import atelier

OUT = ROOT / 'docs/astra_avatar/native'
CAT = json.loads((OUT.parent / 'catalog.json').read_text(encoding='utf-8'))
# Four tones per fabric: outline, shadow, midtone, highlight.
FABRICS = [
    ('17141e','292631','45404d','736675'),
    ('17131d','282031','403349','675473'),
    ('282330','9b99b0','dedce2','fff4df'),
    ('202332','4b5269','8c98ae','dce1e7'),
    ('201c38','39305e','594986','8977ad'),
    ('111727','1c2842','303f63','5a7395'),
    ('121525','20243c','343d59','59637c'),
    ('211522','4f1b35','a92b45','ed4c50'),
    ('15213a','1e3e69','2c6298','558fc0'),
    ('251e22','51404a','90684d','c79c6a'),
    ('1b1521','2c2332','483348','765367'),
]


class Grid:
    """Sparse integer-pixel authoring canvas in original frame coordinates."""
    def __init__(self):
        self.p = {}

    def dot(self, x, y, c):
        self.p[int(x), int(y)] = c

    def rect(self, x0, y0, x1, y1, c):
        for y in range(int(y0), int(y1) + 1):
            for x in range(int(x0), int(x1) + 1):
                self.dot(x, y, c)

    def line(self, x0, y0, x1, y1, c):
        n = max(abs(x1-x0), abs(y1-y0), 1)
        for i in range(n+1):
            self.dot(round(x0+(x1-x0)*i/n), round(y0+(y1-y0)*i/n), c)

    def poly(self, pts, c):
        lo, hi = min(y for x,y in pts), max(y for x,y in pts)
        for y in range(lo, hi+1):
            hits = []
            for (x0,y0),(x1,y1) in zip(pts, pts[1:]+pts[:1]):
                if min(y0,y1) <= y < max(y0,y1):
                    hits.append(x0+(y-y0)*(x1-x0)/(y1-y0))
            hits.sort()
            for a,b in zip(hits[::2],hits[1::2]):
                self.rect(round(a),y,round(b),y,c)
        for a,b in zip(pts,pts[1:]+pts[:1]):
            self.line(*a,*b,c)

    def spec(self, width, height):
        xs = [0,width-1]+[x for x,y in self.p]
        ys = [0,height-1]+[y for x,y in self.p]
        x0,x1,y0,y1 = min(xs),max(xs),min(ys),max(ys)
        return {'at': (x0,y0), 'clear_above': height,
                'grid': [''.join(self.p.get((x,y),'-') for x in range(x0,x1+1)) for y in range(y0,y1+1)]}


def palette(n):
    cols = dict(zip('KDMH',FABRICS[n-1]))
    cols.update(S='f3c09b',s='b97c67',E='fff1d7',B='272332',
                R='b52c43',r='652239',G='d8a24e',g='85603d',Y='fff0a1',
                W='f0edf1',w='a4a6bd',V='67627e',T='4aafb1',P='e9798e')
    if n==1: cols.update(A='f38529',a='b54328',L='ffcf4f')
    else: cols.update(A='e5e5eb',a='9a97aa',L='fff7e8')
    return {k:'#'+v for k,v in cols.items()}


def head(n, i, w, h):
    q=Grid(); cx=w//2; back=i==3; side=i in (1,4,6)
    # The face stays in the original head's lower half; all hats share its neck anchor.
    q.poly([(cx-5,6),(cx+5,6),(cx+6,12),(cx+3,17),(cx-2,18),(cx-5,14)],'K')
    q.poly([(cx-4,7),(cx+4,7),(cx+5,13),(cx+2,16),(cx-2,16),(cx-4,13)],'S')
    q.line(cx-3,14,cx,16,'s')
    if not back:
        q.rect(cx+(1 if side else -3),11,cx+(2 if side else -2),12,'B')
        if not side:q.rect(cx+3,11,cx+4,12,'B')
        q.dot(cx+2,15,'s')
    if n in (1,3,5,6):
        q.poly([(cx-7,10),(cx-7,4),(cx-4,1),(cx+4,1),(cx+7,5),(cx+6,10),(cx+3,7),(cx+1,11),(cx-1,7),(cx-4,10)],'a')
        q.poly([(cx-6,6),(cx-5,2),(cx-2,3),(cx,0),(cx+3,3),(cx+6,4),(cx+3,6),(cx+1,9),(cx,5),(cx-3,8)],'A')
        q.line(cx-4,4,cx-1,3,'L');q.line(cx+1,2,cx+4,4,'L')
        if back:q.poly([(cx-6,5),(cx+6,5),(cx+6,15),(cx+2,20),(cx-4,18)],'a');q.line(cx-4,7,cx-3,17,'A')
        if n==1:
            for x,y in [(cx-6,2),(cx-2,-2),(cx+3,-1)]:
                q.poly([(x-2,y+5),(x,y),(x+3,y+5)],'A');q.dot(x,y+2,'L')
        if n in (3,6):q.poly([(cx-6,4),(cx-7,-1),(cx-2,3),(cx+1,-2),(cx+4,3),(cx+7,1),(cx+6,7)],'A')
        if n==5:
            for s in (-1,1):
                x=cx+s*6
                q.poly([(x-s*2,4),(x+s*3,1),(x+s*3,-4),(x+s,-6),(x+s,-2),(x-s*2,0)],'K')
                q.line(x,0,x+s*2,-2,'H')
                q.poly([(cx+s*5,7),(cx+s*8,9),(cx+s*8,20),(cx+s*4,23)],'a')
                q.line(cx+s*6,9,cx+s*6,19,'A')
            if not back:q.rect(cx-2,9,cx+4,14,'W');q.dot(cx+3,11,'T');q.dot(cx,11,'B')
        if n==6 and not back:q.rect(cx-5,13,cx+5,18,'D');q.line(cx-3,15,cx+4,15,'H');q.dot(cx+3,17,'w')
    elif n in (2,9):
        if n==2:
            q.rect(cx-6,7,cx-4,20,'a');q.rect(cx+4,7,cx+6,19,'A')
            if not back:q.rect(cx-3,9,cx+4,15,'W');q.dot(cx+2,11,'R');q.line(cx-1,13,cx+1,14,'V')
        q.poly([(cx-12,6),(cx-1,-3),(cx+4,-2),(cx+12,6),(cx+7,9),(cx-7,9)],'K')
        q.poly([(cx-10,6),(cx,0),(cx+3,0),(cx+10,6),(cx+6,7),(cx-7,7)],'M')
        q.line(cx,0,cx-3,6,'H');q.line(cx+2,1,cx+6,6,'D')
        q.line(cx-10,7,cx+9,7,'R' if n==9 else 'H')
        if n==2:
            for x in (cx-9,cx+8):
                q.line(x,8,x,15,'r');q.dot(x,12,'Y');q.dot(x,16,'R')
        if back:q.rect(cx-4,10,cx+4,18,'D')
    elif n==4:
        q.poly([(cx-6,4),(cx-2,0),(cx+3,1),(cx+7,6),(cx+5,15),(cx,19),(cx-6,14)],'K')
        q.poly([(cx-5,4),(cx,1),(cx+4,3),(cx+5,7),(cx,11),(cx-5,8)],'H')
        q.poly([(cx-5,10),(cx,13),(cx+5,9),(cx+3,15),(cx,17)],'D')
        q.line(cx-4,11,cx,13,'V');q.dot(cx+3,11,'R')
        if back:q.rect(cx-4,7,cx+4,15,'M');q.line(cx,3,cx,16,'H')
    elif n in (7,8):
        q.rect(cx-6,6,cx+5,9,'B')
        q.poly([(cx-8,3),(cx-6,-1),(cx+7,-1),(cx+9,3),(cx+5,7),(cx-6,7)],'K')
        q.rect(cx-6,1,cx+6,4,'M' if n==7 else 'D')
        q.line(cx-7,5,cx+7,5,'G');q.line(cx-2,1,cx,3,'Y');q.line(cx,3,cx+2,0,'G')
        q.rect(cx-4,7,cx+7,8,'K')
        if n==7 and not back:q.rect(cx-4,13,cx+4,16,'D');q.line(cx-2,13,cx+3,14,'w')
        if back:q.rect(cx-5,9,cx+4,17,'B')
    elif n==10:
        q.poly([(cx-10,5),(cx-9,0),(cx-6,1),(cx-4,-3),(cx,0),(cx+5,-2),(cx+8,3),(cx+10,7),(cx+9,15),(cx+5,19),(cx-6,18),(cx-10,13)],'K')
        q.poly([(cx-8,5),(cx-5,2),(cx+5,2),(cx+8,6),(cx+8,14),(cx+4,17),(cx-5,16),(cx-8,12)],'w')
        q.poly([(cx-7,4),(cx-8,0),(cx-3,2)],'P');q.poly([(cx+4,2),(cx+8,0),(cx+7,5)],'P')
        if not back:
            sx=cx+(5 if side else 1)
            q.rect(sx-4,10,sx+4,15,'r');q.rect(sx-3,10,sx+3,14,'P')
            q.rect(sx-1,11,sx,13,'R');q.dot(sx+2,12,'r')
            q.rect(cx-5,7,cx-3,9,'B');q.dot(cx-4,7,'W')
            q.line(sx-5,15,sx-4,13,'Y');q.line(sx+4,15,sx+5,13,'Y')
        else:q.line(cx,2,cx+1,16,'V')
    elif n==11:
        q.poly([(cx-8,15),(cx-8,5),(cx-4,-1),(cx+3,-2),(cx+8,5),(cx+7,18),(cx+4,15),(cx+4,7),(cx-1,4),(cx-5,9),(cx-4,17)],'K')
        q.line(cx-6,14,cx-6,6,'R');q.line(cx-6,6,cx-1,1,'R');q.line(cx-1,1,cx+4,4,'R')
        q.line(cx+4,4,cx+6,12,'w')
        q.poly([(cx-3,5),(cx+3,5),(cx+3,9),(cx-1,11)],'B')
        if back:q.poly([(cx-6,5),(cx,0),(cx+6,5),(cx+5,17),(cx-5,17)],'D');q.line(cx-4,5,cx-4,15,'M')
    return q


def body(n,i,w,h):
    q=Grid();cx=w//2; back=i==2
    # Only the three torso images are visible in the shipped body sheets.
    coat=n in (1,2,3,5,6,7,8,9)
    if coat:
        end=h+9
        q.poly([(cx-5,1),(cx+5,1),(cx+8,end-3),(cx+5,end),(cx+1,end-2),(cx-3,end),(cx-8,end-2)],'K')
        q.poly([(cx-4,3),(cx+4,3),(cx+6,end-3),(cx+3,end-1),(cx-2,end-3),(cx-6,end-2)],'D')
        q.poly([(cx-4,5),(cx-2,5),(cx-3,end-3),(cx-6,end-2)],'M')
        q.line(cx+3,8,cx+5,end-3,'H')
        if n in (2,8):q.poly([(cx-6,4),(cx-9,end),(cx-4,end-2),(cx-2,8)],'R')
        if n in (3,7,8,9):q.line(cx-6,end-3,cx-2,end-1,'G');q.dot(cx-4,end-5,'Y')
        if n==2:q.line(cx+5,8,cx+9,end,'r')
    q.poly([(cx-5,1),(cx-2,0),(cx+3,0),(cx+5,3),(cx+4,h-1),(cx-4,h-1)],'K')
    q.rect(cx-3,2,cx+3,h-2,'M');q.line(cx-3,3,cx-3,h-3,'H');q.line(cx+3,3,cx+3,h-3,'D')
    if n in (1,10,11):
        q.poly([(cx-2,1),(cx+2,1),(cx+3,h-3),(cx-2,h-3)],'S')
        q.line(cx-1,3,cx+1,5,'s');q.line(cx-1,6,cx+1,7,'s')
        if n in (10,11):q.line(cx-2,2,cx-1,5,'V');q.dot(cx-2,6,'V')
    if n in (2,3,5,9):q.line(cx-3,1,cx+2,6,'R' if n in (3,9) else 'H')
    if n==4:
        q.poly([(cx-4,2),(cx+4,2),(cx+3,7),(cx,9),(cx-3,7)],'H');q.line(cx-3,4,cx,6,'G');q.line(cx,6,cx+3,4,'G')
        q.poly([(cx-2,h-1),(cx+2,h-1),(cx+3,h+10),(cx,h+12),(cx-3,h+10)],'W');q.line(cx,h,cx,h+9,'G')
        for s in (-1,1):
            q.poly([(cx+s*4,3),(cx+s*10,1),(cx+s*11,-5),(cx+s*8,-2),(cx+s*7,-6),(cx+s*5,-1),(cx+s*3,0)],'g')
            q.line(cx+s*4,1,cx+s*9,-2,'G');q.line(cx+s*5,1,cx+s*7,-2,'Y')
    if n in (3,5,6):
        sides=(-1,) if n in (3,5) else (-1,1)
        for s in sides:
            for k in range(5):
                x=cx+s*(3+k//2);y=k-2
                q.poly([(x-2,y),(x,y-2),(x+2,y),(x+1,y+3),(x-1,y+2)],'w')
                q.line(x-1,y,x,y+2,'W')
    if n in (7,8):
        for s in (-1,1):q.rect(cx+s*5-2,0,cx+s*5+2,2,'g');q.line(cx+s*5-2,0,cx+s*5+1,0,'Y')
        q.line(cx-2,2,cx+1,7,'G')
        for y in (3,5,7):q.dot(cx+2,y,'Y')
    if n==6:
        q.rect(cx-3,-1,cx+3,3,'D');q.line(cx-2,0,cx+2,0,'H')
        for y in (4,6,8):q.dot(cx+1,y,'w')
    if back:
        q.rect(cx-2,3,cx+2,h-3,'M');q.line(cx,3,cx,h-3,'D')
        if n in (7,8):q.line(cx-2,3,cx+2,6,'G')
    q.rect(cx-4,h-2,cx+4,h-1,'R' if n in (1,3,9,11) else 'g')
    q.dot(cx+1,h-2,'Y')
    if n==10:
        q.poly([(cx-5,h-2),(cx+5,h-2),(cx+6,h+5),(cx+2,h+3),(cx,h+7),(cx-3,h+3),(cx-6,h+4)],'M')
        q.line(cx-4,h-1,cx+4,h-1,'G');q.line(cx,h,cx,h+4,'H')
    return q


def limb(n,layer,i,im):
    q=Grid();w,h=im.size
    # Preserve articulation silhouettes rather than inventing a new skeleton.
    for y in range(h):
        for x in range(w):
            r,g,b,a=im.getpixel((x,y))
            if a<128:continue
            edge=any(not (0<=xx<w and 0<=yy<h) or im.getpixel((xx,yy))[3]<128 for xx,yy in ((x-1,y),(x+1,y),(x,y-1),(x,y+1)))
            tone='K' if edge else ('H' if (r+g+b)>460 else 'M' if (r+g+b)>260 else 'D')
            if layer==2:
                if n in (1,10,11) and y<h*0.55 and not edge:tone='S' if x<w//2 else 's'
                if y>=h*0.68 and not edge:tone='W' if n in (7,8) else 'R' if n==11 else 'D'
                if n==9 and y>h//3 and not edge:tone='R'
                if n in (3,4,7,8) and y==h//2 and not edge:tone='G'
                if n in (5,6) and y in (h//2,h//2+1) and not edge:tone='W'
            else:
                if n in (10,11) and y>h*0.6 and not edge:tone='W' if y%3==0 else 'V'
                if n==11 and h*0.25<y<h*0.6 and not edge:tone='S'
                if n in (3,4,7,8,9) and y==h-3 and not edge:tone='G'
                if n in (1,2,5) and y<h*0.65 and not edge:tone='D' if x%4==0 else 'M'
            q.dot(x,y,tone)
    if layer==2 and n==11:
        # Glove expands symmetrically around the hand's existing end.
        q.rect(max(0,w//2-2),max(0,h-5),w//2+2,h-2,'r');q.rect(w//2-1,h-5,w//2+1,h-3,'R');q.dot(w//2,h-5,'P')
    return q


def weapon(n,i,im):
    q=Grid();w,h=im.size
    if n!=10:
        # Preserve the original piece's blade axis and hand end, while giving
        # each reference its own silhouette rather than recolouring a sword.
        tx,ty,bx,by=[(2,41,15,4),(1,24,28,4),(1,9,35,4),(1,10,18,4)][i]
        dx,dy=tx-bx,ty-by;length=math.hypot(dx,dy)
        nx,ny=-dy/length,dx/length
        width={1:1,2:1,3:2,4:3,5:4}[n]
        def pt(t,offset):
            curve=(math.sin(t*math.pi)*3 if n==5 else 0)
            return (round(bx+dx*t+nx*(offset+curve)),round(by+dy*t+ny*(offset+curve)))
        q.poly([pt(0,-width),pt(.65,-width),pt(1,0),pt(.72,width),pt(0,width)],'K')
        q.poly([pt(.03,-width+1),pt(.65,-width+1),pt(.94,0),pt(.7,width-1),pt(.03,width-1)],'D' if n in (1,2) else 'w' if n in (3,4) else 'Y')
        q.line(*pt(.05,-width),*pt(.7,-width),'H' if n in (1,2) else 'W')
        q.line(*pt(-.08,0),*pt(0,0),'r' if n in (1,2) else 'g')
        q.line(*pt(.03,-width-2),*pt(.03,width+2),'G' if n in (3,4,5) else 'V')
        if n in (3,4):q.dot(*pt(.03,0),'Y')
        return q
    for y in range(h):
        for x in range(w):
            r,g,b,a=im.getpixel((x,y))
            if a<128:continue
            l=(r+g+b)//3
            tone='K' if l<70 else 'V' if l<120 else 'w' if l<185 else 'W'
            if n in (1,2):tone='K' if l<100 else 'D' if l<170 else 'H'
            if n==5:tone='g' if l<100 else 'w' if l<170 else 'Y'
            # Hilt is the darker band of the archive's oriented piece.
            if 70<=l<120 and n in (3,4,10):tone='G'
            q.dot(x,y,tone)
    return q


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    rows=[];manifest=[]
    for n,a in enumerate(CAT,1):
        dest=OUT/f'{n:02d}_{a["slug"]}';dest.mkdir(exist_ok=True)
        layers=list(range(5))
        for layer in layers:
            hidden=layer==4 and n in (6,7,8,9,11)
            base=f'c/sp/img{layer}/{16 if layer==4 and (n==10 or hidden) else 0:03d}.mgr'
            ims=[mgr.gbm_to_png_image(g,g.palette).convert('RGBA') for g in mgr.mgr_decode(mgr.load_source(base)[1])]
            colours=palette(n);specs={}
            for i,im in enumerate(ims):
                w,h=im.size
                if w==1 and h==1:continue
                q=Grid() if hidden else head(n,i,w,h) if layer==0 else body(n,i,w,h) if layer==1 else limb(n,layer,i,im) if layer<4 else weapon(n,i,im)
                specs[i]=q.spec(w,h)
            slug=f'astra_{n:02d}_{layer}'
            classes=15 if hidden else 2 if layer==4 and n==10 else 9 if layer==4 else 15
            cid=110000+n*10+layer;sheet=300+n
            name=a['name']+' '+('맨손' if hidden else ['머리','의상','장갑','신발','무기'][layer])
            source={'NAME':name,'BASE':base,'CLASSES':classes,'HIDDEN_WEAPON':hidden,'COLOURS':colours,'FRAMES':specs}
            (ROOT/'costumes/sets'/f'{slug}.py').write_text('# Generated by costumes/astra_native.py; edit the authoring source.\n'+''.join(k+' = '+pprint.pformat(v,width=110,sort_dicts=False)+'\n\n' for k,v in source.items()),encoding='utf-8')
            rows.append(f'{cid}    {layer}      {sheet}    {slug}')
            _,composed=atelier.composed(slug,layer)
            path=dest/f'img{layer}_{sheet}.mgr'
            binary=mgr.mgr_encode([mgr.gbm_from_png(im,None,f'{slug}/{i}') for i,im in enumerate(composed)])
            path.write_bytes(binary)
            # Export the RGB565-quantized data that the guest actually receives.
            quant=[mgr.gbm_to_png_image(g,g.palette).convert('RGBA') for g in mgr.mgr_decode(binary)]
            pngdir=dest/f'img{layer}';pngdir.mkdir(exist_ok=True)
            for i,im in enumerate(quant):im.save(pngdir/f'{i:03d}.png')
            manifest.append({'avatar':n,'layer':layer,'id':cid,'sheet':sheet,'classes':classes,'hidden_weapon':hidden,'base':base,'frames':len(quant),'authored_frames':len(specs),'mgr':str(path.relative_to(OUT)).replace('\\','/')})
    catalog=ROOT/'costumes/catalog.txt'
    original=[line for line in catalog.read_text(encoding='utf-8').splitlines() if 'astra_' not in line]
    catalog.write_text('\n'.join(original+rows)+'\n',encoding='utf-8')
    (OUT/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
    print(f'Authored {len(CAT)} avatars / {len(manifest)} layer sheets / {sum(x["authored_frames"] for x in manifest)} native pieces')


if __name__=='__main__':main()
