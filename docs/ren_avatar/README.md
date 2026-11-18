# 정령 렌의 아바타

정령 페이지(기본/장비)의 `*아바타` 버튼으로 여는 렌의 옷장에 들어가는 열 벌.
코드는 `hl5_core/src/spirit_avatar.rs`, 서술은 `ANALYSIS.md` §61.

| 파일 | 무엇 |
|---|---|
| `references/NN_*.png` | 사용자가 준 레퍼런스 10장 (무기는 그리지 않았다) |
| `native/overview.png` | 원래 렌과 열 벌, 앞·옆·뒤 대기 (3배) |
| `native/overview_before_after.png` | v4 품질 작업 전(위)·후(아래) — 모자 셋을 게임 모자로, 뒷머리 커튼 v3, 유리 헬멧, 눈사람 음영, 날개 대비 (`ANALYSIS.md` §63) |
| `native/hats_hero_finish_before_after.png` | 주인공 모자 다섯, 마감 패스 전(위)·후(아래) |
| `native/hats_ren_worn_before_after.png` | 렌 모자 셋, 얹힌 것(위)·쓴 것(아래) — 챙은 속눈썹 줄, 크라운은 렌 두개골 높이 (`ANALYSIS.md` §64) |
| `native/ingame_wardrobe.png` | 헤드리스 재생에서 한 벌씩 입힌 정령 페이지 (2배) |
| `native/NN/zoom.png` | 한 벌: 원래 렌 / 그 옷(7배), 걷기, 시전·앉기·만세·소환, 실제 크기, 레퍼런스 |
| `native/NN/img0_32N.mgr`, `img1_32N.mgr` | 내보낸 시트 (런타임은 같은 것을 플레이어 아카이브로 합성한다) |

다시 만들기:

```powershell
python costumes/ren_studio.py build        # sets/ren_*.py, catalog, native/NN/*.mgr
python costumes/build.py                   # hl5_core/src/costume_sets.rs
python costumes/ren_studio.py sheet docs/ren_avatar/native/overview.png
python costumes/ren_studio.py zoom 3 docs/ren_avatar/native/03/zoom.png
cargo run -q -p hl5_probe -- --boot --avatar --script tools_re/scripts/spirit-avatar.txt
```
