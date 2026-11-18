"""Compare exported native pixels with the runtime-composed shadows, and
gather the replays' captures into contact sheets.

    python costumes/astra_verify.py <storage of a replay run>

Any one replay's storage holds every costume sheet (hl5_core::costume
composes them all at boot), so one is enough for the pixel check.
"""
import json
import sys
from pathlib import Path
from PIL import Image
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools_re'))
import mgr  # noqa: E402

root = ROOT / 'docs/astra_avatar/native'
entries = json.loads((root / 'manifest.json').read_text(encoding='utf-8'))
storage = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'out/v4-warrior'
shadow = storage / 'data/data/co.kr.eamobile.HeroesLore5/files/c/sp'
for e in entries:
    a = mgr.mgr_decode((root / e['mgr']).read_bytes())
    b = mgr.mgr_decode((shadow / f'img{e["layer"]}' / f'{e["sheet"]:03d}.mgr').read_bytes())
    assert len(a) == len(b), e
    for x, y in zip(a, b):
        ix = mgr.gbm_to_png_image(x, x.palette).convert('RGBA')
        iy = mgr.gbm_to_png_image(y, y.palette).convert('RGBA')
        assert ix.size == iy.size and ix.tobytes() == iy.tobytes(), e
cifs = sorted((storage / 'data/data/co.kr.eamobile.HeroesLore5/files/c/sp/cif').glob('9*.cif'))
assert len(cifs) == 12, cifs

screens = root / 'screenshots'
shots = 0
for cls in ('warrior', 'rogue', 'gunner', 'knight'):
    for kind in ('field', 'character'):
        if not (screens / f'{cls}_01_{kind}.png').exists() and (root / f'ingame_{cls}_{kind}.png').exists():
            shots += 11   # gathered by an earlier run
            continue
        board = Image.new('RGB', (1200, 960), (16, 24, 34))
        for n in range(1, 12):
            p = screens / f'{cls}_{n:02d}_{kind}.png'
            im = Image.open(p)
            assert im.size == (400, 240), p
            board.paste(im, (((n - 1) % 3) * 400, ((n - 1) // 3) * 240))
            shots += 1
        board.save(root / f'ingame_{cls}_{kind}.png', optimize=True)
# Individual captures are kept for the warrior only; the other classes live
# in their contact sheets.
for p in screens.glob('*.png'):
    if not p.name.startswith('warrior_'):
        p.unlink()
    else:
        Image.open(p).save(p, optimize=True)
print(f'{len(entries)}/{len(entries)} exported sheets match runtime pixels; {len(cifs)} rig cif variants served; '
      f'{shots} captures in 8 contact sheets.')

# v3 beside v4, then the gallery page.
v3 = Image.open(root / 'overview_2x_v3.png').convert('RGB')
v4 = Image.open(root / 'overview_2x.png').convert('RGB')
both = Image.new('RGB', (max(v3.width, v4.width), v3.height + v4.height + 8), (12, 16, 24))
both.paste(v3, (0, 0))
both.paste(v4, (0, v3.height + 8))
both.save(root / 'overview_v3_vs_v4.png', optimize=True)

catalog = json.loads((root.parent / 'catalog.json').read_text(encoding='utf-8'))
WEAPON_WORDS = '워리어 검 · 로그 쌍단검 · 건슬링어 총 · 나이트 창과 방패'
cards = []
for n, a in enumerate(catalog, 1):
    folder = f'{n:02d}_{a["slug"]}'
    gif = 'motion_rogue.gif' if n == 10 else 'motion_warrior.gif'
    mgrs = ' · '.join(f'<a href="{folder}/{Path(e["mgr"]).name}">{Path(e["mgr"]).stem}</a>'
                      for e in entries if e['avatar'] == n)
    cards.append(
        f'<article><h2>{n:02d} · {a["name"]}</h2>'
        f'<img class="shot" src="screenshots/warrior_{n:02d}_character.png" alt="{a["name"]} 캐릭터 메뉴">'
        f'<img class="shot" src="screenshots/warrior_{n:02d}_field.png" alt="{a["name"]} 필드">'
        f'<p class="caption">{WEAPON_WORDS} — 직업마다 이 아바타의 무기가 나가고, 공격·스킬 이펙트가 아바타 색으로 바뀝니다.</p>'
        f'<details><summary>움직임(게임 틱 40ms)과 무기·이펙트</summary>'
        f'<p class="caption">행: 대기 · 걷기 · 무장 대기 · 무장 걷기 / 열: 아래 · 오른쪽 · 위 · 왼쪽. '
        f'리그 CIF 변형으로 머리칼·자락이 프레임마다 다른 그림입니다.</p>'
        f'<img class="gif" loading="lazy" src="{folder}/{gif}" alt="움직임">'
        f'<a href="{folder}/motion_strip.png">프레임별 PNG</a>'
        f'<img class="gif" loading="lazy" src="{folder}/weapons.png" alt="무기와 이펙트">'
        f'<p class="caption">{mgrs}</p></details></article>')
sheets = ''.join(
    f'<h3>{label}</h3><img class="overview" loading="lazy" src="ingame_{cls}_field.png" alt="{label} 필드">'
    f'<img class="overview" loading="lazy" src="ingame_{cls}_character.png" alt="{label} 캐릭터">'
    for cls, label in (('warrior', '워리어'), ('rogue', '로그'), ('gunner', '건슬링어'), ('knight', '나이트')))
html = ('<!doctype html><html lang="ko"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
        '<title>Hero5 아바타 v4</title><style>:root{color-scheme:dark}body{margin:0;background:#111b28;color:#eaf0f6;'
        'font:16px/1.65 system-ui}main{max-width:1320px;margin:auto;padding:24px 16px}h1{margin:0}header p{color:#b4c6d9}'
        'a{color:#94d2ff}img{image-rendering:pixelated;max-width:100%}.overview{width:100%;background:#18212e;margin:6px 0}'
        '.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(380px,100%),1fr));gap:20px}article{padding:18px;'
        'background:#1b293b;border:1px solid #354c67;border-radius:12px}h2{font-size:19px;margin-top:0}.shot{width:100%;'
        'display:block;margin:8px 0}.caption{font-size:13px;color:#a8bdd2}details{margin-top:14px}summary{cursor:pointer;'
        'color:#eed197}.gif{width:100%;background:#18212e;margin:6px 0}nav{margin:20px 0}</style><main><header>'
        '<h1>HERO5 · 아바타 11종 v4</h1><p>머리칼·투구·모자는 게임 자신의 머리(img0)를 재배색해 이식, 옷감은 주름·털·짐승 털 재질, '
        '머리·몸통은 리그 포즈(대기·걷기 프레임마다 다른 그림), 눈은 머리칼과 분리된 고유 색, '
        '무기는 직업별(검·쌍단검·총·창+방패), 이펙트는 아바타 색. 아래 캡처는 hl5_probe 로 실행한 실제 게임 프레임버퍼이며 '
        'Android 단말 캡처는 아닙니다.</p></header><nav><a href="README.md">제작·검증 기록</a> · '
        '<a href="manifest.json">리소스 목록</a> · <a href="../index.html">콘셉트 초안</a></nav>'
        '<img class="overview" src="overview_v3_vs_v4.png" alt="v3(위)와 v4(아래)">'
        '<p class="caption">위 v3, 아래 v4 — 게임의 CIF 배치로 조합한 대기 자세, 2배.</p>'
        f'<h2>직업별 실제 게임 화면</h2>{sheets}<h2>아바타별</h2><section class="grid">{"".join(cards)}</section></main></html>')
(root / 'index.html').write_text(html, encoding='utf-8')
print('gallery written')
