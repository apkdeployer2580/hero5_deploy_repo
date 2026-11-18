# 아바타 의상 (costumes)

캐릭터 페이지 `*아바타` 옷장(`hl5_core/src/avatar.rs`)에 들어가는, 이 프로젝트가
직접 그린 의상의 **설계 원본**이다. 옷장의 나머지(게임 자신의 세트·전설 외형)는
아이템 표에서 부팅 때 만들어지므로 여기 없다.

## 원칙: 게임 픽셀은 여기 없다

`payload/`(라이선스 데이터)는 저장소 밖이다. 그래서 의상은 **게임 시트 위에 얹는
격자**로만 저장하고, 합성은 `hl5_core::costume` 이 부팅 때 플레이어가 가진
아카이브로 한다 — 결과는 `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/
c/sp/img<N>/<번호>.mgr` 섀도로 서빙된다(`LoadRes` 가 호스트 FS 를 먼저 본다).
원본 시트는 여기 없지만, 재배색한 조각(팔·다리)과 게임 머리에서 이식한 머리칼·모자
(`donor.py`)는 그 결과가 격자 글자로 들어간다.

## 파일

| 파일 | 무엇 |
|---|---|
| `catalog.txt` | `id layer sheet slug` 한 줄씩. **id 는 재사용 금지**(`hl5_avatar.txt` 가 기록한다), sheet 는 **256 이상**(몬스터·NPC 경로는 부호 있는 바이트라 절대 못 가리킨다) |
| `sets/<slug>.py` | 설계: `NAME`(한글), `COLOURS`(문자 → `#rrggbb`), `FRAMES`(이미지 번호 → `at`, `clear_above`, `grid`, 원본 끝 너머에 덧붙이는 그림은 `src` = 그릴 바탕 이미지). 아스트라 생성물은 `RIG`(리그 시트), `FX`(무기와 함께 입는 이펙트 시트 번호), `RECOLOUR`(이펙트 시트: 565→565 대응표와 유지할 그림)도 단다 |
| `build.py` | `sets/` → `hl5_core/src/costume_sets.rs` 전사. `--check` 는 낡았으면 1 |
| `hats.py` | 주인공 모자 다섯(마법사·왕관·고양이귀·산타·밀짚)의 원본. **쓴** 모양으로 맞춰(챙 = 이마 위, 뷰마다 가운데, 챙 그림자) 마감 패스와 재질(펠트 주름·털·짚 결)을 거쳐 `sets/` 에 쓴다 — `python costumes/hats.py`, `hats.py preview out/x.png`, `--raw` 는 마감 없이 |
| `donor.py` | **게임 자신의 머리에서 머리칼·모자를 이식한다** (§63): `DONOR`(중심 맞춤, 주인공) / `fit_hat`(챙을 이마선에, 렌). 원본 휘도 위치를 디자인 램프에서 보간해 톤 간격을 지킨다. `palette(N)` 이 후보 머리의 색과 개수를 준다 |
| `finish.py` | 마감 패스: 우리 색의 검정을 원작 바닥(틴트된 어둠)으로, 면마다 왼쪽 위 빛의 형태 음영, 재질(`folds` 옷감 주름, `bristles` 짐승 털), 채색 외곽선(경계하는 가장 어두운 재질의 가장 어두운 톤), 글자 89개 배정 |
| `ren_studio.py`, `ren/rNN.py`, `ren/kit.py` | 정령 렌의 열 벌(아래) |
| `atelier.py` | 아카이브 시트 위에 합성한 미리보기(8배) |
| `astra/aNN.py` | **아스트라 11종의 설계 원본** (아래). `sets/astra_*.py` 는 여기서 생성된다 — 손대지 마라 |
| `astra/kit.py` | 공용 형: 스파이크 머리(`spiky`), 장교 코트(`officer`/`officer_open`), 여밈 장포(`robe`), 털(`fur`), 좌우 대칭(`sym`), 옆모습 앞섶(`recede`) |
| `astra/arms.py` | 무기: 검·단검·창·총을 스타일과 팔레트로 조각 축을 따라 그린다, 방패 격자 |
| `astra_studio.py` | `astra/` → `sets/astra_*.py` + `rig_table.txt` + `docs/astra_avatar/native/` (MGR·PNG·manifest·catalog 행) |
| `rig.py` | 리그: 포즈 슬롯 배치, CIF 에서 유도한 프레임별 포즈, 변형본 재매핑 표 |
| `astra_look.py` | 작업 화면: `N out` 머리·몸 12배, `fig N out` 조립 인물, `frames N out anim dir` 프레임별, `weapons N out` 무기·이펙트 |
| `astra_template.py` | 머리칼 격자를 그릴 얼굴 템플릿(`frame ax ay w h`)과 `check N frame`(덮이지 않은 자리 `!`) |
| `astra_motion.py` | 게임 틱(40 ms) GIF, `--static` 은 원본 CIF |
| `astra_preview.py` / `astra_replay.py` / `astra_verify.py` | 문서용 미리보기·참조 검사, 직업별 재생 생성, 런타임 합성 대조·갤러리 |
| `astra_native_v1.py` | v1 생성기, 되살릴 때만. 돌리면 `sets/`·`catalog.txt` 를 옛 그림으로 덮어쓴다 |

## 격자 규칙

* `.` 은 원래 픽셀을 둔다, `-` 는 지운다, 그 밖의 문자는 `COLOURS` 의 색.
* `at` = 격자 왼쪽 위가 놓이는 자리, **원래 이미지의 픽셀 좌표**(음수 가능).
* `clear_above` 보다 위의 원래 행은 지운다(모자가 덮는 머리칼).
* 캔버스는 **양쪽으로 같은 만큼** 넓어진다 — `.cif` 가 이미지를 **중심**으로
  놓기 때문이다. 그래서 위로 솟는 모자를 그려도 머리가 안 움직인다.
* 원래 시트의 이미지 수를 그대로 둔다(`DrawSprite` 는 인덱스 경계 검사가 없다).

## 머리(img0)에 대해 알아 둘 것

* 네 직업이 **같은 주인공**이라 머리 시트 `img0/000` 하나를 쓴다 — 머리 의상
  하나가 네 직업 모두에 맞는다. `img5` 는 몸이 아니라 직업별 이펙트다.
* 이미지 7장: 0 정면, 1 3/4 앞(가장 많이 쓰임, 80회), 2·5 1×1 자리표시,
  3 뒤, 4 옆, 6 3/4 아래. 3/4·옆은 오른쪽을 보는 쪽을 게임이 뒤집어 그린다.
* 의상 팔레트는 −1(자기 색). 출하 장비는 전부 `.pal` 색 치환을 단다.

## 아스트라 11종 — `astra/aNN.py` 의 설계 언어 (v3, v4)

한 모듈이 한 외형이고 대문자 이름이 전부다. 아바타마다 13장이 나간다:
머리·몸·팔·다리, 직업별 무기 넷(워리어 검 / 로그 쌍단검 / 건슬링어 총 / 나이트 창),
나이트 방패, 직업별 이펙트 넷 (`astra_studio.parts`; id 는 저장 파일이 기록하므로
절대 바꾸지 않는다).

* `PAL` — 문자 → `#rrggbb`. 격자에서 쓰는 글자는 전부 여기 있어야 한다.
* `HAIR` — 게임 머리칼 재배색 램프(`teal`/`white`/`outline`). `{}` 이면 머리칼을
  지운다. 03 처럼 원본 컷이 곧 초안이면 재배색만 쓰고, 그 밖은 지우고 그린다.
* `EYE` — `lash`·`pupil`·`iris`·`glint`. 원본 눈은 머리칼 램프로 칠해져 있어서
  따로 짚어(`astra_studio.EYES_BASE`) 이 색으로 칠한다. `None` = 눈 없음(투구).
* `HEAD` — 프레임(0 정면·1 3/4·3 뒤·4 옆·6 3/4아래) → 격자 하나 또는 목록.
  `' '` 는 손대지 않음, `'.'` 은 원본 유지, `'-'` 는 지움. **피부·눈은 덮지
  못한다** — 가면·투구·탈은 `'over_face': True`. 3/4·옆은 왼쪽을 본다(게임이
  뒤집는다). 그리기 전에 `astra_template.py 1 -3 -6 24 23`, 그린 뒤
  `astra_template.py check N 1` 로 `!`(덮이지 않은 원본 머리칼 자리)가 없는지 본다.
  `HEAD_EXTRA` 는 v2 호환.
* `DONOR` (v4) — 게임 머리 `img0/<sheet>` 의 머리칼(`hair` + `ramp`) 또는 모자(`groups`:
  [{`cols`, `ramp`|`map`}]), `skin`(그 머리의 얼굴색 — 피부에 닿은 어두운 톤을 뺀다),
  `keep`(그대로 두는 장식). 기본 머리에 중심 맞춤으로 깔리고, `HEAD` 격자는 그 위에
  덧그린다(뿔·가면·타래). 리그가 흔들 머리칼은 `SWAY['head']['loose'] = 'donor'`.
  후보는 `donor.palette(N)` 과 `img0` 접촉 시트로 고른다 — 주인공과 같은 7장 배치만.
* `HAIR_BACK` — 옆모습이 머리 뒤에 그리는 조각(이미지 2, 기본은 1×1 빈칸). 긴 머리.
* `BODY` — 프레임 0·1·2 격자 하나 또는 목록. 몸통 좌표에서 머리가 덮는 줄은
  정면 0~6 / 옆 0~4 / 뒤 0~8, 다리는 7~25 줄이다. 긴 옷은 아래로 그린다.
* `SWAY` — 리그가 움직이는 것: `head` 는 `loose`(움직이는 글자) · `band`(끝 몇 줄이
  눌리고 늘어나는가) · `hang`(이 줄 아래가 좌우로 흔들린다) · `rigid`; `body` 는
  `from`(뷰마다 자락이 시작하는 줄) · `loose`; `back_step`. 포즈는 `rig.py` 가 CIF
  에서 유도한다 — 디자인은 무엇이 느슨한지만 말한다.
* `ARM` — `sleeve`/`cuff`/`hand` 램프, `hand_len`(팔 끝 몇 칸이 손인가), `hand_grow`
  (권투 장갑처럼 손을 키운다).
* `LEG` — `trouser`/`foot`/`band` 램프와 `foot_rows`(장화 높이). 영역은 위치로
  나눈다(각 열의 맨 아래 = 발).
* `WEAPONS` — `sword`/`dagger`/`gun`/`spear`/`shield` 마다 `style` 과 `pal`
  (`arms.py` 문서). `THEME` — 이펙트 색조(`hue` 도, `sat` 배율).
* `PREVIEW_CLASS` — 미리보기 직업(10 은 로그).
* `FOLDS` / `PELT` / `NOFORM` (v4) — 마감 패스의 재질: 이 글자들의 넓은 면은 옷감
  주름 / 짐승 털 획을 받고, `NOFORM` 은 이미 칠한 것(원뿔·유리)이라 톤을 안 바꾼다.
  어두운 옷감도 면이다(4방향이 같은 색인 픽셀 넷 이상). `kit.cone(view)` 는 원뿔
  삿갓(7단 + 방사 살결 + 엮음 고리, 글자 `1..7` 과 테 `vxyz`), `kit.fur` 는 털뭉치.

```powershell
python costumes/astra_look.py fig 3 out/astra/f03.png 8     # 조립 인물
python costumes/astra_look.py frames 3 out/astra/w03.png 3 2 # 무장 걷기 옆모습 프레임별
python costumes/astra_look.py weapons 3 out/astra/wp03.png   # 무기·방패·이펙트
python costumes/astra_studio.py build && python costumes/build.py && python costumes/astra_preview.py
python costumes/astra_replay.py   # 그리고 직업별 재생을 돌린 뒤
python costumes/astra_verify.py out/astra-v3-warrior
```

## 정령 렌의 열 벌 — `ren/rNN.py` 의 설계 언어

렌은 주인공이 아니라 정령이고 시트가 둘뿐이다: 머리 `img0/042`(6장), 몸
`img1/025`(46장 — 팔·몸통·하체·빛꼬리 둘·소환 용). 한 벌 = 그 둘을 같은 번호
(`img0/32N`·`img1/32N`)로 다시 그린 쌍이고, `CLASSES = 16`(`costume::SPIRIT`)이라
주인공 옷장에는 절대 안 나온다. `ren_studio.py` 가 `ren/rNN.py` 를 읽어
`sets/ren_NN_{0,1}.py` 와 카탈로그 줄(`1300N0/1`)을 쓴다 — 그 둘은 생성물이다.

설계는 **렌 자신의 픽셀 라벨** 위에 쓴다(`ren_studio.py labels` 가 지도를 그린다):
머리 hair/face/ear/eye/orn, 팔 sleeve/trim/jewel/cuff/glove/hand, 하체
skirt/panel/trim/inner/shade/leg/foot, 빛꼬리. 대문자 이름이 전부다:

* `NAME`, `REF`(`docs/ren_avatar/references`), `PAL`(격자 글자).
* `HAIR` 램프(머리칼 8단계를 밝기 순서로), `ORNAMENT` `'hair'`|`'keep'`|램프.
* `HEAD` {0 정면, 1·2 3/4(화면 왼쪽을 본다), 3 뒤: [격자...]} — 모자·귀·리본·가닥.
* `TORSO` {33 앞, 34 옆, 35 뒤: 격자} — 통째로 다시 그린다. 턱 아래 6×7 만 보인다.
* `ARM`, `LEGS` {라벨: 램프|`'skin'`|`'clear'`}, `CUT`(레오타드), `ANKLET`.
* `DRESS` — 하체 프레임마다 그 프레임의 다리를 따라 긴 치마를 그린다(`dress()`의
  독스트링): 램프·외곽·프릴 단·밑단·트임·앞트임(망토·로브)·별·길이.
* `WINGS` 램프 | `'hide'` | {29, 30: 격자} — 빛꼬리는 cif 가 59% 로 그린다.
* `BODY` {프레임: [격자...]} — 마지막에 덧그림(바구니 같은 소품).
* `DONOR_HAT` (v4) — 게임 모자(`113` 장교모·`109` 마법사·`117` 챙모자)를 `views`
  {렌 뷰: 모자 뷰}로, 색 `groups` 로 재배색해 **쓴다**(§64): 챙 밑단은 렌 얼굴 첫 줄
  + 1, 크라운은 주인공 두개골 대비 렌 두개골 비율만큼 가운데 행을 뺀다
  (`ren_studio.hat_fit` → `donor.fit_hat`), 챙 아래 한 줄은 머리칼 그림자.
  `squash` {뷰: ±행}은 빼는 줄 수를 고친다(마법사 모자 −2 — 원뿔은 키가 모양이다).
  `face_cols` 는 모자와 얼굴이 같이 쓰는 색.
* `FOLDS` / `PELT` / `NOFORM` — 아스트라와 같다.
* `ren/kit.py`: `lock`(머리 가닥), `curtain`(뒷머리 v3: 흔들리는 가르마의 덩어리,
  광택 띠, 아래로 좁아짐), `wing`(깃털 날개) — 글자 O b y Y L / F f m l W 를 쓴다.

```powershell
python costumes/ren_studio.py zoom 8 out/ren08.png     # 원래 렌 | 그 옷, 걷기, 동작, 실제 크기, 레퍼런스
python costumes/ren_studio.py parts 8 out/ren08p.png   # 조각마다 12배 격자
python costumes/ren_studio.py sheet out/ren_all.png    # 열 벌 한 장
python costumes/ren_studio.py build && python costumes/build.py
cargo run -q -p hl5_probe -- --boot --avatar --script tools_re/scripts/spirit-avatar.txt
# astra_studio.py build 는 catalog.txt 의 아스트라 줄을 맨 뒤로 다시 쓴다 — 그 뒤엔
# ren_studio.py build 를 한 번 더 돌려야 표 순서가 돌아온다(build.py --check 가 잡는다)
```

## 순서

```
python costumes/atelier.py preview <slug> out/x.png    # 그리고 고치고
python costumes/build.py                              # Rust 표로 전사
cargo test -p hl5_core --lib costume                  # 합성·중심·번호 검사
cargo run -q -p hl5_probe -- --boot --avatar --script tools_re/scripts/avatar.txt
```
