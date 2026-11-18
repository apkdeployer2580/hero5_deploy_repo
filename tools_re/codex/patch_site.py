# -*- coding: utf-8 -*-
"""Rewrite the drop column of the published 장비 도감 from the real tables."""
import json, re, pathlib, hl5data as H, drops

DIFF_ORDER = {"헬": 0, "하드": 1, "노말": 2}
JIN_INDEX = drops.JIN_INDEX

# How many of the page's items have a drop source. The footer prose that once
# stated it is gone (operator, 2026-09-07), but the check stays: recomputed
# from the tables on every run, so a table that moves stops the pipeline here
# instead of publishing a silently different set.
#
# 217 -> 235 on 2026-09-08: `lostdrops.rs` reopened the twenty rows the
# shipped tables strand behind a zero-width band. Eighteen of the twenty land
# here; the other two (쟈칼의발톱, 시바스아뮬렛) are 에픽, and this codex lists
# 영웅 and above.
# 235 -> 266 on 2026-10-11: 33 more rows, minus two epic accessories.
FOOTER_SOURCES = 266

# Every reader of `it.d` in the page's own script. `d` stopped being a list of
# strings and became a list of `[몬스터, 확률]` pairs, so a reader this script
# does not know about would render `노말 루퍼스,4.9%` — or, if it only tests
# `.length`, keep working and hide the ones that do not. The template is a live
# page with no source anywhere, so this list is the only proof we found them
# all. Anything else, including the legacy `dl`, stops the run.
D_READERS = [
    "it.d.length",                          # "드랍 있는 것만" chip — type-blind
    "it.d.map(function(m){return m[0];})",  # search haystack, edited below
    "it.d,it.ln)",                          # the drop cell, edited below
]

def audit_d_readers(out):
    m = re.search(r'(<script[^>]*id="data"[^>]*>)(.*?)(</script>)', out, re.S)
    js = out[:m.start(2)] + out[m.end(2):]      # the data block is JSON, not code
    for hit in re.finditer(r"""it(?:\.dl?\b|\[\s*['"]dl?['"]\s*\])[^\n]{0,45}""", js):
        if not any(hit.group(0).startswith(r) for r in D_READERS):
            raise SystemExit("unknown reader of it.d: " + hit.group(0)[:60])
    # `render()` sorts with `a[sortKey]`, which reads whichever field the
    # clicked header names — a dynamic reader no scan of `it.d` can see. The
    # drop column is not sortable; if it ever becomes so, `x - y` over a pair
    # is NaN and the order goes quietly wrong.
    for key in re.findall(r'data-s="([^"]+)"', out):
        if key in ("d", "dl"):
            raise SystemExit(f"column `{key}` became sortable; a[sortKey] would compare pairs")

def pct(p):
    v = p * 100.0
    if v >= 10:  return f"{v:.0f}%"
    if v >= 1:   return f"{v:.1f}%"
    if v >= 0.1: return f"{v:.2f}%"
    # Never round a real drop down to `0.000%`: the row is on the list because
    # the item does drop, and a zero would say the opposite.
    if v < 0.0005: return "<0.001%"
    return f"{v:.3f}%"

def to_list(bucket):
    def key(e):
        lab, inf = e
        return (-inf["p"], DIFF_ORDER.get(lab.split(" ", 1)[0], 9), lab)
    return [[lab, pct(inf["p"])] for lab, inf in sorted(bucket.items(), key=key)]

def sub(s, old, new):
    """Replace once, and be a no-op if this edit is already in the page.

    The template is the *live* site, so a second run reads back what the first
    one published. The data block is regenerated wholesale either way; only
    these prose/markup edits need to be idempotent."""
    if new in s:
        return s          # already applied; `old` may still be a prefix of `new`
    n = s.count(old)
    assert n == 1, f"expected 1 hit, got {n}: {old[:70]!r}"
    return s.replace(old, new)

# The two drop-prose blocks this footer has carried. `main()` removes whichever
# one the template still has; see the comment at the call site.
FOOTER_PUBLISHED = """  <p><b>드랍 데이터를 전부 다시 뽑았다 (2026-09-07).</b> 이전 표는 드랍 레코드 13바이트에서
  <b>(부위, 번호) 쌍 하나</b>만 읽어 만든 것이었다. 실제로는 한 레코드에 그 쌍이 <b>셋</b> 들어
  있고, 몬스터가 굴린 난수가 어느 구간에 떨어지느냐에 따라 다른 쌍이 쓰인다 — <b>영웅 구간</b>,
  <b>전설 구간</b>, <b>세트 구간</b>. 옛 표는 전설 구간만 담고 있었다. 그래서 노말
  <b>대형 경비로봇</b>·<b>루퍼스</b>가 실제로 떨구는 다크서클릿·빙해의투구가 <b>드랍 없음</b>으로
  보였다. 드랍처가 있는 장비는 <b>114종에서 217종</b>으로 늘었다.</p>

  <p>이름 앞의 <span class="mob"><i>노말</i></span><span class="mob"><i>하드</i></span><span class="mob"><i>헬</i></span> 은 난이도다 —
  같은 몬스터라도 난이도가 다르면 떨구는 것이 다르다. 뒤의 <span class="mob"><b>4.9%</b></span> 는
  그 몬스터 <b>한 마리를 잡을 때</b> 그 장비가 나올 확률이고, 몬스터마다 다른 굴림 횟수(보통
  1~2회, 보스는 4회)와 한 구간에 네 칸이 있다는 것까지 반영한 값이다. <b>확률은 원작
  그대로이고 두 갈래가 같은 값을 쓴다</b> — 이 앱은 드랍률을 건드리지 않는다. 목록은 확률이
  높은 순이고, <b>외 N</b> 에 마우스를 올리면 전체가 뜬다.</p>

  <p><b>등급이 강등되어 나오는 드랍은 이 표에 없다.</b> 몬스터에게는 자기 레벨대의 기본 장비를
  노멀·레어·에픽으로 찍어 주는 구간이 셋 더 있지만, 그 구간이 겨누는 324개 행은 <b>전부 노멀
  등급</b>이라 이 도감(영웅 이상)과 한 줄도 겹치지 않는다.</p>

  <p><span class="line main">main 전용</span> <span class="line vanilla">vanilla 전용</span> 은 APK 갈래다.
  헬 보스 여덟은 자기 드랍과 별개로 전설 구간을 한 번 더 굴리는데, 같은 보스·같은 구간·같은
  횟수라 <b>두 갈래의 확률이 완전히 같고 나오는 물건만 다르다</b> — main 은
  <code>(진)</code> 무기, vanilla 는 그 원본인
  알케인:혼돈·펜리르:야성검·데스크림존·궁니르:번개다. 그 원본 넷은 출하 드랍표에 한 칸도 없어서
  main 에서는 대장장이로만 나온다.</p>"""

FOOTER_SHIPPED = """  <p><b>몬스터 드랍 없음</b>은 그 장비를 떨구는 몬스터가 없다는 뜻이다. 몬스터가 표의 등급을
  그대로 주는 경로는 몬스터마다 정해진 드랍 목록 하나뿐이고, 거기에 없는 장비는 대장장이 조합·
  상점·유물함으로만 나온다. 이름 앞의 <span class="mob"><i>노말</i></span><span class="mob"><i>하드</i></span><span class="mob"><i>헬</i></span> 은 난이도다 —
  같은 몬스터라도 난이도가 다르면 떨구는 것이 다르다. <b>외 N</b> 에 마우스를 올리면 전체 목록이 뜬다.</p>"""

# What must be true of the footer afterwards. `drop()` alone cannot tell "the
# template never had it" from "the template changed and we removed nothing", so
# the outcome is asserted instead of the edit.
FOOTER_GONE = ["드랍 데이터를 전부 다시 뽑았다", "몬스터 드랍 없음</b>은",
               "등급이 강등되어", "은 APK 갈래다"]
FOOTER_KEPT = ["점선으로 된", "착용 불가"]

def drop(s, block):
    """Take a block out if the template still has it. Absence is a no-op."""
    n = s.count(block)
    if n == 0:
        return s
    assert n == 1, f"expected 1 hit, got {n}: {block[:70]!r}"
    # Swallow the newline it sat on, so the paragraphs that stay do not inherit
    # a blank line where it used to be.
    if block + "\n" in s:
        return s.replace(block + "\n", "", 1)
    return s.replace(block, "", 1)

def audit_footer(out):
    a = out.index("<footer>")
    foot = out[a:out.index("</footer>", a)]
    for m in FOOTER_GONE:
        if m in foot:
            raise SystemExit(f"footer still carries the drop prose: {m}")
    for m in FOOTER_KEPT:
        if foot.count(m) != 1:
            raise SystemExit(f"footer lost its own notice: {m}")

def main():
    src = pathlib.Path("codex_live.html").read_text(encoding="utf-8")
    m = re.search(r'(<script[^>]*id="data"[^>]*>)(.*?)(</script>)', src, re.S)
    D = json.loads(m.group(2))
    own, _demoted = drops.build()

    byname = {}
    for k in range(0, 19):
        for r in H.items(k):
            byname.setdefault((k, r["name"]), r["index"])

    was = sum(1 for it in D["items"] if it.get("d"))
    entries = 0
    for it in D["items"]:
        idx = JIN_INDEX if it.get("host") else byname.get((it["kk"], it["n"]))
        o = own.get((it["kk"], idx), {}) if idx is not None else {}
        it["d"] = to_list(o)
        it.pop("dl", None)     # a legacy key; `audit_d_readers` proves nothing reads it
        lines = {inf["ln"] for inf in o.values()}
        it["ln"] = lines.pop() if len(lines) == 1 and None not in lines else ""
        entries += len(it["d"])
    now = sum(1 for it in D["items"] if it["d"])
    assert now == FOOTER_SOURCES, f"footer says {FOOTER_SOURCES}종, the tables give {now}"

    out = src[:m.start(2)] + json.dumps(D, ensure_ascii=False, separators=(",", ":")) + src[m.end(2):]

    # ---- CSS: the per-monster rate, and the content-line badge -------------
    out = sub(out, ".none{font-size:11.5px;color:var(--ink-3)}",
        ".none{font-size:11.5px;color:var(--ink-3)}\n"
        ".mob b{font-family:\"IBM Plex Mono\",monospace;font-weight:600;font-size:10px;\n"
        "  margin-left:3px;color:var(--ink-3)}\n"
        ".line{font-family:\"IBM Plex Mono\",monospace;font-size:10px;letter-spacing:.04em;\n"
        "  padding:1px 5px;border-radius:99px;border:1px solid var(--line);\n"
        "  background:var(--panel-2);color:var(--ink-2);white-space:nowrap}\n"
        ".line.main{border-color:var(--leg);color:var(--leg)}\n"
        ".line.vanilla{border-color:var(--set);color:var(--set)}")

    # ---- markup: one more filter chip -------------------------------------
    out = sub(out,
        '<button class="chip" data-f="h" data-v="1" aria-pressed="false">호스트 추가</button>',
        '<button class="chip" data-f="h" data-v="1" aria-pressed="false">호스트 추가</button>\n'
        '    <button class="chip" data-f="l" data-v="1" aria-pressed="false">갈래 전용</button>')

    # ---- script: rate in the cell, line badge, search over names ----------
    out = sub(out,
        "  var F={g:new Set(),k:new Set(),c:new Set(),h:new Set(),d:new Set()}, sortKey='lv', sortDir=1;",
        "  var F={g:new Set(),k:new Set(),c:new Set(),h:new Set(),d:new Set(),l:new Set()}, sortKey='lv', sortDir=1;")
    out = sub(out, """  function dropCell(list){
    if(!list||!list.length) return '<span class="none">몬스터 드랍 없음</span>';
    var head=list.slice(0,SHOWN).map(function(m){
      var i=m.indexOf(' ');
      return '<span class="mob"><i>'+esc(m.slice(0,i))+'</i>'+esc(m.slice(i+1))+'</span>';
    }).join('');
    var rest=list.length-SHOWN;
    if(rest>0) head+='<span class="more">외 '+rest+'</span>';
    return '<div class="mobs" title="'+esc(list.join(', '))+'">'+head+'</div>';
  }""", """  var LN={main:'main 전용',vanilla:'vanilla 전용'};
  function dropCell(list, ln){
    var tag = ln ? '<span class="line '+ln+'">'+LN[ln]+'</span>' : '';
    if(!list||!list.length)
      return '<span class="none">몬스터 드랍 없음</span>'+tag;
    var head=list.slice(0,SHOWN).map(function(m){
      var s=m[0], i=s.indexOf(' ');
      return '<span class="mob"><i>'+esc(s.slice(0,i))+'</i>'+esc(s.slice(i+1))
            +'<b>'+esc(m[1])+'</b></span>';
    }).join('');
    var rest=list.length-SHOWN;
    if(rest>0) head+='<span class="more">외 '+rest+'</span>';
    var tip=list.map(function(m){return m[0]+' '+m[1];}).join(', ');
    return '<div class="mobs" title="'+esc(tip)+'">'+head+tag+'</div>';
  }""")
    out = sub(out, "    if(F.h.size && !it.host) return false;",
                   "    if(F.h.size && !it.host) return false;\n"
                   "    if(F.l.size && !it.ln) return false;")
    out = sub(out, "      var hay=(it.n+' '+it.gl+' '+it.k+' '+it.o.join(' ')+' '+it.d.join(' ')).toLowerCase();",
                   "      var mobs=it.d.map(function(m){return m[0];}).join(' ');\n"
                   "      var hay=(it.n+' '+it.gl+' '+it.k+' '+it.o.join(' ')+' '+mobs+' '+it.ln).toLowerCase();")
    out = sub(out, "        +'<td class=\"drop\">'+dropCell(it.d)+'</td></tr>';",
                   "        +'<td class=\"drop\">'+dropCell(it.d,it.ln)+'</td></tr>';")

    # ---- copy: the footer's drop prose comes out (operator, 2026-09-07) ---
    # Two blocks have stood in this footer: the page's original single
    # paragraph, and the four this script published earlier the same day. The
    # table says it all in the column itself, so both go. The two notices under
    # them (점선 +랜덤옵션, 착용 불가) are the page's own and stay — the checks
    # in `audit_footer` are what keep this from quietly taking the wrong thing.
    for block in (FOOTER_PUBLISHED, FOOTER_SHIPPED):
        out = drop(out, block)
    audit_footer(out)

    audit_d_readers(out)
    pathlib.Path("codex_new.html").write_text(out, encoding="utf-8")
    print(f"드랍처 있는 장비: {was} -> {now} / {len(D['items'])}, 드랍 항목 {entries}개")
    print(f"갈래 전용: {sum(1 for it in D['items'] if it['ln'])}종")
    print(f"{len(src)} B -> {len(out)} B")

main()
