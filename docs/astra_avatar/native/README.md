# 실제 인게임 아바타 도트 v4

[실제 적용 갤러리](index.html) · [디자인별 이전 | 이후](before_after_v4.png) ·
[v3 와 v4 나란히](overview_v3_vs_v4.png) ·
직업별 실제 화면: [워리어](ingame_warrior_field.png) · [로그](ingame_rogue_field.png) ·
[건슬링어](ingame_gunner_field.png) · [나이트](ingame_knight_field.png)

**v4 (2026-10-15).** 원작 옆에서 우리 머리칼은 3~4단 면이라 아이콘처럼 보였다.
머리칼·투구·모자를 **게임 자신의 머리에서 재배색해 이식**한다(`costumes/donor.py`):
01 ← `img0/001` 뾰족머리, 04 ← `082` 은 투구, 05 ← `027` 긴 웨이브(아래로 커튼),
06 ← `016` 은발, 07·08 ← `113` 장교모. 큰 옷감 면은 주름(02·03·05·06·07·08·09·11),
멧돼지 탈은 짐승 털, 털 목도리는 털뭉치, 02·09 삿갓은 7단 원뿔에 살결. 마감 패스가
어두운 옷감을 외곽선으로 오인하던 것과 흰↔검 램프가 섞이던 것도 고쳤다. 리그·무기·
이펙트·id·시트 번호는 v3 그대로다. 근거는 [`ANALYSIS.md`](../../../ANALYSIS.md) §63.

**v3 (2026-10-14).** v2 는 머리·몸통이 한 장씩이라 움직이지 않았고(대기·걷기 내내
같은 그림을 1 px 옮길 뿐), 머리칼 재배색이 눈까지 바꿨으며, 06·07·08·09·11 은 무기가
"맨손"이라 건슬링어가 맨손에서 총을 쐈다. v3 는 셋을 다 고쳤다. 근거와 바이너리
주소는 [`ANALYSIS.md`](../../../ANALYSIS.md) §58.

| 무엇 | v3 |
|---|---|
| 움직임 | **리그.** 머리 시트 37장(원본 7 + 머리칼 포즈 27 + 뒷머리 3), 몸 시트 27장(원본 12 + 자락 포즈 15). 대기·걷기·무장대기·무장걷기에서 프레임마다 다른 포즈를 고르는 **CIF 변형본**(재매핑 198건) — 머리가 오르면 머리칼 끝이 눌리고, 걸으면 자락이 보폭에 맞춰 흔들리며 옆모습에선 뒤로 흐른다. 프레임 수·오프셋·판정은 원본 그대로다 |
| 눈 | 원본은 눈을 머리칼 램프로 칠한다. 눈 픽셀을 따로 짚어 아바타마다 고유 색(01 붉음, 03 하늘, 05 보라 …) |
| 그림 | 머리·몸통 재작화. 초안의 실루엣 — 01 불꽃 스파이크와 흰 테 장포, 02 구슬 삿갓·가면·긴 백발, 03 은발·흰 금장 코트·털, 04 불꽃 견갑·휘장, 05 굽은 뿔·무릎까지 백발·기모노·털목도리, 06 깃·털망토, 07 가면 제독·금 불꽃 밑단, 08 붉은 코트, 09 붉은 테 삿갓·장포, 10 갈기 달린 탈·털치마, 11 후드·권투 장갑 |
| 무기 | 직업마다 따로: 워리어 검 / 로그 쌍단검 / 건슬링어 총 / 나이트 창 + 방패, 아바타 컨셉대로(카타나·장검·대검·곡도·사브르·청룡도·식칼 / 탄토·크리스·스틸레토·발톱 / 플린트락·리볼버·핸드캐논·쌍열총 / 야리·랜스·글레이브·할버드·삼지창 …) |
| 이펙트 | 무기를 입으면 같은 아바타의 이펙트 시트가 함께 입혀진다. 베기·불꽃·구체·폭발만 아바타 색조로, 버프 글자·등급 화살표·먼지·탄피는 원본 그대로 |

## 시트

아바타마다 13장, 11종 143장. 폴더 `NN_slug/` 에 `imgL_SSS.mgr`(게임용 시트)와
`imgL_SSS/NNN.png`(다시 디코딩한 그림), `motion_<직업>.gif`(게임 틱 40 ms, 행: 대기 ·
걷기 · 무장 대기 · 무장 걷기 / 열: 아래 · 오른쪽 · 위 · 왼쪽), `motion_strip.png`(프레임별),
`weapons.png`(직업별 무기 조각·방패·이펙트).

| 부위 | 시트 | id |
|---|---|---|
| 머리·몸·팔·다리 | `img0..3/3NN` (NN = 01..11) | `1100N0..3` |
| 워리어 검 · 로그 쌍단검 · 건슬링어 총 · 나이트 창 | `img4/3NN` · `img4/32N` · `img4/34N` · `img4/36N` | `1100N4..7` |
| 나이트 방패 | `img6/3NN` | `1100N8` |
| 이펙트(워리어·로그·건슬링어·나이트) | `img5/3N0..3N3` | `1200N0..3` |

v2 의 id(`1100N0..4`)와 시트 번호는 그대로라 저장된 `hl5_avatar.txt` 는 새 그림을
입는다(무기는 v2 에서 워리어·나이트 공용이던 `1100N4` 가 이제 워리어 검이다).

## 검증 (전부 이 저장소에서 재실행 가능)

- `cargo test -p hl5_core --lib` 225 — 리그 CIF 가 직업 넷 모두에서 머리·몸통의 이미지
  번호 바이트만 바꾸고 전부 시트 안을 가리키는지, 변형본 경로가 아카이브와 안 겹치는지,
  이펙트 시트가 색만 바꾸고 유지 그림은 그대로인지, 무기 시트가 제 직업에만 가는지.
- 44 조합(11종 × 4직업)의 모든 애니메이션 부품 참조 532,301건이 변형본으로도 시트 안
  (`validation.json`).
- 직업 넷 재생(`replay_<직업>.txt`)이 각각 11종을 차례로 입힌다: 폴트 0, 리그 CIF 읽기 2회
  (기본 → 머리 → 머리+몸). 공격 재생으로 이펙트 색 확인.
- `astra_verify.py`: 재생의 저장소에 런타임이 합성한 **143 시트가 내보낸 MGR 과 픽셀 동일**,
  리그 CIF 12개 서빙.
- `screenshots/` 는 워리어 재생의 실제 게스트 프레임버퍼(캐릭터·옷장·필드), 나머지 직업은
  `ingame_<직업>_{field,character}.png` 모음. Android 단말 캡처는 아니다.

## 재생성

```powershell
python costumes/astra_studio.py build      # sets/, rig_table.txt, MGR, PNG, manifest, catalog
python costumes/build.py                   # hl5_core/src/costume_sets.rs
python costumes/astra_preview.py           # motion GIF·strip, weapons, overview, validation
python costumes/astra_replay.py            # replay_<직업>.txt
cargo run -q -p hl5_probe -- --boot --avatar --storage out/v4-warrior --script docs/astra_avatar/native/replay_warrior.txt
#   (rogue / gunner / knight 도 같은 식으로)
python costumes/astra_verify.py out/v4-warrior
cargo test -p hl5_core --lib costume
```

이전 판: [v3 개요](overview_2x_v3.png) · [v2 와 v3](overview_v2_vs_v3.png) · [v2 개요](overview_2x_v2.png) · [v2 캡처](ingame_contact_sheet_v2.png) ·
[v1 개요](overview_2x_v1.png) · [v1 과 v2](overview_v1_vs_v2.png).
