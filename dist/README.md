# 배포용 APK

사이드로드용 서명본. Play 를 거치지 않고 그대로 설치한다.
`build_android.ps1 -Release` 가 여기에 이름을 붙여 떨군다.

| | |
|---|---|
| 파일 | `hero5-v1.08-vc31.apk` |
| versionName / versionCode | `1.08` / `31` |
| applicationId | `kr.heroeslore.five` |
| 최소 안드로이드 | API 26 (8.0 Oreo) |
| ABI | `arm64-v8a` 단독 |
| 크기 | 22.7 MB |

## 파일명 규칙

```
hero5-v<versionName>-vc<versionCode>.apk            ← main    (전부)
hero5-vanilla-v<versionName>-vc<versionCode>.apk    ← vanilla ((진) 무기 없음)
```

가운데 토큰은 컨텐츠 갈래다(2026-09-01). `android_app/flavor.properties` 가
정하고 `build_android.ps1` 이 바이너리와 이 이름에 **같은 변수로** 쓴다.
⛔ vanilla 빌드에서 토큰을 빼지 마라 — `dist/` 에서 어느 줄인지 안 보인다.
**두 갈래는 같은 패키지·같은 키라 서로 덮어쓰고 versionCode 는 한 수열이다.**
정본 = [`../BRANCHES.md`](../BRANCHES.md).

자매 저장소 hero3 / hero4 와 같다(그쪽 `dist/` 에 `hero3-v0.1.0-vc11.apk`).
gradle 이 내는 이름은 언제나 `app-release.apk` 라서 무엇이 들었는지 말하지
않는다 — 이 트리를 떠나는 빌드는 이름으로 자기가 무엇인지 말해야 한다.

**versionCode 를 이름에 같이 넣는 이유**는 둘이 다른 속도로 움직이기 때문이다:
`-Release` 는 빌드마다 versionCode 를 올리고, versionName 은 사람이 올릴 때만
바뀐다. 이름에 versionName 만 있으면 서로 다른 두 빌드가 같은 파일명을 갖고
뒤엣것이 앞엣것을 조용히 덮어쓴다.

`versionName` 은 `android_app/version.properties` 에서 손으로 올린다.
**1.08 은 원본 게임의 `01.00.08`** 을 따른 것이다.

## 설치 전에 지문을 확인할 것

이 APK 가 정말 이 저장소에서 나온 것인지 확인한다. 값이 다르면 설치하지 않는다.

```
9c834481eda1893c2d3b991950b87e1243b8047659f1fb0a29c52ebc790721eb
```

```powershell
apksigner verify --print-certs hero5-v1.08-vc31.apk
```

`-Release` 빌드는 이 값을 스스로 찍는다(`signer:` 줄).

★ **배포되는 모든 빌드의 지문은 같아야 한다.** 서명이 갈리면 안드로이드가
업데이트를 거부하고, 사용자는 지우고 다시 깔아야 한다 = **세이브 소실**.
서명 키는 `hero5` 별칭이고 gradle 이 `~/.gradle/gradle.properties` 의
`hl5.keystore` / `hl5.keystorePassword` / `hl5.keyAlias` / `hl5.keyPassword`
(또는 같은 이름의 `HL5_*` 환경변수)로만 찾는다. 없으면 `-Release` 가 **조용히
unsigned APK** 를 내고 설치되지 않는다.

## APK 자체는 커밋하지 않는다

`.gitignore` 가 `dist/*.apk` 를 뺀다. 이 문서와 위 지문만 저장소에 남는다.
릴리스 한 번이 22.7 MB 이고 `-Release` 는 자주 돈다 — 전부 넣으면 저장소가
금방 무거워진다. (hero3 / hero4 는 APK 를 같이 커밋한다. 여기서도 그렇게 할
거면 그 한 줄만 지우면 된다.)

배포할 때는 이 파일을 그대로 보내면 된다:

```
D:\tmp\Hero5NativeAPP\dist\hero5-v1.08-vc31.apk
```

## 무엇이 들어 있나

기기에 설치되면 서버도 인터넷도 없이 돈다 — 인프로세스 사설 서버가 로그인과
캐시샵을 답하고, 공용창고는 `<files>/hl5_warehouse.txt` 에 산다. 자세한 것은
저장소 루트의 [`ANALYSIS.md`](../ANALYSIS.md).

원본(`co.kr.eamobile.HeroesLore5`)과 **applicationId 가 달라 한 폰에 나란히
설치된다.**
