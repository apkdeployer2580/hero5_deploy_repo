# 히어로즈 로어 5 — 원본 APK 분석과 64비트 이식 근거

측정일 2026-08-18. 대상은 저장소 루트의 `heroesLore5.apk` (22,742,040 바이트).
모든 수치는 `tools_re/` 의 스크립트로 재현된다 — 8절 참조.

---

## 1. 결론부터

**원본 APK 를 64비트로 "변환"하는 것은 불가능하다.** 게임 본체는
`lib/armeabi/libHeroesLore5.so` 안의 **1,200,084 바이트 ARM32 기계어**이고
소스가 없다. ARM32 기계어를 ARM64 기계어로 바꾸는 도구는 존재하지 않는다.

**대신 가능한 것**: 프로세스 전체가 AArch64 인 앱 안에서 원본 `.so` 를
**ARMv5TE 해석 실행**하고, 그 코드가 바깥과 만나는 **좁은 경계면만** 네이티브로
구현한다. 게임 로직은 출시된 바이너리 그대로 한 바이트도 고치지 않고, 그 주변이
전부 64비트다. 형제 프로젝트 **크로노스 소드**(`C:\ChronosSwordNativeAPP`)가
같은 구조로 이미 출시됐다.

경계면이 좁다는 것이 근거다:

| 경계면 | 크기 |
|---|---|
| import 심볼 | **77** (libc 50 / GLES 19 / libstdc++ 4 / libm 3 / liblog 1) |
| JNI 진입점 (Java→native) | **18** |
| 게스트가 부르는 Java 정적 메서드 (native→Java) | **19** |
| 그 외 전부 (WIPI `MC_*` 121개 포함) | `.so` 안에 있음 — 재구현하지 않는다 |

기준선은 "import ≤ 120 이면 HLE 계층은 며칠짜리"다. 77 은 그 절반이다.

### 그런데 진짜 근거는 따로 있다 — 크로노스 소드와 거의 같은 바이너리다

두 게임은 EA Mobile Korea 가 같은 시기 같은 툴체인으로, 같은 "midas" 포팅
계층에 대고 컴파일한 물건이다. `tools_re/seamdiff.py` 로 대조한 결과:

| | 크로노스 소드 | 히어로즈 로어 5 | 판정 |
|---|---|---|---|
| 컴파일러 | GCC 4.4.0 | GCC 4.4.0 | 같음 |
| 아키텍처 | ARMv5TE, ARM 전용, soft-float | 동일 | 같음 |
| `DT_NEEDED` | libc, libstdc++, libm, libGLESv1_CM, libdl, liblog | 동일 | 같음 |
| import | 79 | **77** | **HERO5 ⊂ CHRONOS (진부분집합)** |
| JNI 진입점 | 14 | 18 | 크로노스의 14개 + **4개 추가** |
| Java 콜백 | 19 | 19 | **이름·시그니처까지 전부 동일** |
| WIPI 논리 화면 | 400×240 | **400×240** | 같음 |
| `MC_*` (WIPI 커널) | 121 | 121 | **120개 공유** |
| 공유 함수 이름 | — | **743** (그중 **711개는 크기까지 동일**) | midas 계층 통째로 |

**HLE import 계층에 새로 쓸 함수가 하나도 없다.** 히어로즈 로어 5 가 부르는
77개 import 는 크로노스가 이미 구현한 79개의 부분집합이다. 크로노스에만 있는
둘은 `_Znwj`(`operator new`)와 `_ZdlPv`(`operator delete`) — 히어로즈 로어 5 는
C++ 인데도 자체 할당자를 쓴다.

**Java 콜백 19개는 이름도 시그니처도 완전히 일치한다.** 대조에서 불일치가 0건이었다.

---

## 2. 원본 바이너리의 정체

`payload/libHeroesLore5.so` — 1,974,477 바이트

```
32-bit EM_ARM ET_DYN, soname libHeroesLore5.so
built by         GCC: (GNU) 4.4.0
architecture     ARMv5TE ("5TE")
Thumb            없음 — ARM 전용
VFP              없음 — soft float
NEON             없음
DT_NEEDED        libc.so, libstdc++.so, libm.so, libGLESv1_CM.so, libdl.so, liblog.so
dynamic flags    DT_SYMBOLIC, DT_TEXTREL

.text            1,200,084 바이트
PT_LOAD 합계     1,530,728 바이트
.symtab          살아 있음 — 함수 3,394개 / 오브젝트 297개 이름
exports          동적 심볼 3,671개
```

**Thumb 이 없고 VFP/NEON 도 없다** — 해석기가 구현해야 할 명령어 집합이 가능한
가장 단순한 ARM 서브셋이다. 크로노스용으로 이미 포크해 둔 `vendor/arm32_cpu`
(ARMv5TE 확장: `SMUL<x><y>`, `SMLA/SMLAW/SMLAL`, `LDRD/STRD`, `cond==0b1111`,
`BLX <reg>`) 가 같은 컴파일러·같은 아키텍처이므로 **네 가지 수정이 전부 그대로
필요하고 그대로 적용된다.**

**`.symtab` 이 살아 있는 것이 가장 큰 비용 배수다.** 프로파일 출력도, 게스트
폴트 보고도, 호스트 후킹도 전부 원본 이름으로 말할 수 있다. 심볼은
`tools_re/guest_functions.txt` (3,467줄) 와 `guest_objects.txt` 에 덤프해 뒀다.

### 재배치 — 로더는 거의 교과서다

```
R_ARM_RELATIVE    1540
R_ARM_JUMP_SLOT     75
R_ARM_ABS32          2
R_ARM_GLOB_DAT       1
PT_TLS            없음
```

### 크로노스에 없던 함정 세 가지

로더에서 **새로 처리해야 하는 것은 이것뿐이다.**

1. **`.init_array` / `.fini_array` 각 1개** — 크로노스에는 없었다. C++ 정적
   생성자를 엔트리포인트 전에 돌려야 한다. 바이너리 안에
   `_Z41__static_initialization_and_destruction_0ii.clone.0` (216바이트) 이 있다.
2. **`DT_TEXTREL` + `R_ARM_ABS32` 2개** — 재배치가 `.text` 에 쓴다. 로더는
   재배치 동안 텍스트 세그먼트를 쓰기 가능하게 열어야 한다.
3. **C++ ABI import** — 예외/RTTI 가 경계면을 넘을 수 있다. `_Unwind_*` 심볼이
   바이너리 안에 있다.

### APK 안의 IDA 데이터베이스 — 열어 봤다, 거의 비어 있다 (2026-08-18)

`lib/armeabi/libHeroesLore5.so.idb`, **21,340,268 바이트**. 매직 `"IDA1"` —
진짜 IDA Pro 데이터베이스다. 빌드할 때 실수로 `lib/` 에 딸려 들어갔다.

```
IDA 버전     7.3  (root.version 700)
작성          2020-10-26 00:46 UTC
열린 횟수     2
원본 경로     C:\Users\<user>\Desktop\영웅서기\임시\5\lib\armeabi\libHeroesLore5.so
로더          ELF for ARM (Shared object)
```

**이 바이너리가 맞다.** 섹션 주소 11개(`.plt` `.text` `.rodata` `.ARM.extab`
`.ARM.exidx` `.init_array` `.fini_array` `.data.rel.ro` `.got` `.data` `.bss`)가
ELF 와 **전부 정확히 일치**하고, 함수 3,430개 중 **3,339개가 같은 주소에서
`.symtab` 과 같은 이름**이다. (`root.md5` 는 `.so` 실제 해시와 다르게 나오는데,
주소 단위 일치가 이보다 강한 증거다. python-idb 의 md5 필드 해석 문제로 본다.)

**그런데 순수 자동 분석 결과물이다. 분석가가 넣은 지식이 없다.**

| 확인 항목 | 결과 |
|---|---|
| `.symtab` 으로 유도되지 않는 함수 이름 | **0개** |
| 진짜로 어긋나는 이름 | **0개** (15건은 전부 libgcc/EABI 별칭 — `__aeabi_dadd`↔`__adddf3` 류) |
| `.symtab` 이 이름을 안 주는 함수 76개 | 전부 `.plt` 썽크 (0x312b0–0x31648). import 77개로 이미 알던 것 |
| 함수 머리 주석 95개 | 전부 IDA 디컴파일러 자동 타입 힌트 — `'this'`, `'int'`, `'format'`, `'switch 19 cases '` |
| 이름 8,909개 중 `.symtab` 밖 5,020개 | 전부 IDA 자동 문자열 라벨 (`aMidasDrawpixel` 류) + ELF 해시 테이블 라벨 |
| 열린 횟수 2회 | "로드 → 자동분석 → 저장" 과 일치 |

21 MB 인 이유는 지식이 많아서가 아니라, **심볼이 이미 온전한 바이너리**에 대해
IDA 가 플래그·xref·명령어 디코딩을 전부 펼쳐 놓았기 때문이다.

### 건진 것 하나 — 경계면 구조체의 필드 이름

구조체 36개 중 대부분은 IDA 표준 타입(`Elf32_Sym`, `sockaddr_in`, `stat`,
`va_list`, `fd_set`, `pthread_mutex_t`, `phase1_vrs`/`core_regs` 등 ARM 언와인드)
이다. 게임/WIPI 쪽은 여덟 개뿐이지만 **이건 `.symtab` 에 없는 정보다**:

```
MC_GrpContext    m_mask, m_clip, m_fgpxl, m_bgpxl, m_transpxl, m_alpha,
                 m_offsetX, m_offsetY, m_pfnPixelOp, m_param1, m_reserved, m_style
midasAudioInfo   audioEncodingType, audioType, handlingType, dataOffset,
                 dataLength, bitsPerSample, samplingRate, channels,
                 bDirtyBuffer, audioBuffer
MC_FileInfo      attrib, creationTime, size
MCTimer          alarmTime
Heckbert_Edge    x, dx, i          (다각형 스캔 변환 — MC_grpFillPolygon)
Point2           x, y
Barycoord        u, v, w
CommonEle        next, prev
```

`MC_GrpContext` 는 `MC_grpGetContext`/`MC_grpSetContext`/`MC_grpInitContext` 가
주고받는 WIPI 그래픽 컨텍스트이고, `midasAudioInfo` 는
`MIDASKernelManager::decodeAudioFromBuffer(__midasAudioInfo*, uchar*, long)` 의
인자다 — 둘 다 호스트가 실제로 구현해야 하는 경계면이다.

**단, 복원한 것은 필드 이름과 순서뿐이고 오프셋·타입·크기는 아니다.** 게다가
복원이 휴리스틱(구조체 노드 뒤의 멤버 네트노드를 따라 걷는다)이라 **목록 자체가
짧게 나올 수 있다** — 같은 방법으로 `sockaddr_in` 은 4개 중 2개만 나온다. 각
목록은 "적어도 이 필드들이 이 순서로"로 읽고, 실제 레이아웃은 `disasm.py` 로
접근 코드를 읽어서 확정한다.

전체 덤프는 `tools_re/idb_structs.txt` 와 `tools_re/idb_extra_functions.txt`,
재생성은 `python tools_re/idb_extract.py` (`pip install python-idb` 필요).

**판정: `.idb` 는 계획을 바꾸지 않는다.** 브링업으로 간다.

---

## 3. 실행 구조 (원본 Java 쪽에서 알아낸 것)

`payload/AndroidManifest.xml` 과 `payload/classes.dex` (dex 문자열 4,046개) 에서:

```
package            co.kr.eamobile.HeroesLore5
versionName        01.00.08
런처 액티비티      co.kr.eamobile.MidasActivity
                     extends co.kr.eamobile.mtx.CarrierFuncActivity
표면               co.kr.eamobile.MidasGLSurfaceView
게스트 진입 클래스 co.kr.eamobile.CletEntry
```

`CletEntry` 의 콜백 순서 (dex 바이트코드에서 직접 확인):

| Java 메서드 | 부르는 네이티브 |
|---|---|
| `onSurfaceCreated` | `nativeInitFileStorage`, `nativeInitAudioConfiguration` |
| `onDrawFrame` | `nativeLoop`, `nativeDestroyTexture` |
| `onKeyHandleEvent` | `nativeKeyHandleEvent` |
| `onTouchHandleEvent` | `nativeTouchHandleEvent` (6개 경로) |
| `doFocusGained` / `doFocusLost` | `nativeFocusGained` / `nativeFocusLost` |
| `surfaceDestroyed` | `nativePauseApp` |
| `onDestroy` | `nativeDestroyApp` |

**WIPI 논리 화면은 400×240 이다.** `CletEntry.adjustAutoScreen` 의 상수가
`[400, 240]` 이다 — 크로노스와 같다. 다만 크로노스에서 확인됐듯 이것은
바이너리 안의 상수가 아니라 **`nativeInitDisplay` 의 인자**다. 게스트가
프레임버퍼를 여기서 할당하고 `MC_grpGetDisplayInfo` 로 되돌려 준다. 게임 자신의
레이아웃이 따라오는지는 경험적으로 확인할 문제다.

### 크로노스에 없던 JNI 진입점 4개 — 새로 쓸 코드의 전부

| 심볼 | 크기 |
|---|---|
| `Java_..._CletEntry_nativeFocusGained` | 52 B |
| `Java_..._CletEntry_nativeFocusLost` | 52 B |
| `Java_..._CarrierFuncActivity_nativeaddEAMKAndroidHeader` | 144 B |
| `Java_..._CarrierFuncActivity_nativeaddEAMKAndroidPacketSize` | 8 B |

### 역방향 — 게스트가 부르는 Java 정적 메서드 19개

전부 `co.kr.eamobile.CletEntry` 에 있고, **크로노스와 이름·시그니처가 완전히
동일하다.** 즉 크로노스의 `AndroidService` HLE 구현이 그대로 온다.

```
getAssetSize (Ljava/lang/String;)I          loadAsset (I[BI)I
getUniqueAssetNameFromID (I)Ljava/lang/String;
attachBufferIDtoResourceID (JI)V            detachBufferIDfromResourceID (J)V
getAvailableInternalMemorySize ()J
putSoundClip (JJJI)I    clipFreeSound (J)I  clipPlaySound (JZ)I
getSoundVolume ()I      setSoundVolume (I)V putMusicClip (JJJ)I
putClip (JII[BIIIII)I   vibration (II)I     getOpenGLESVersion ()Ljava/lang/String;
initMTX (Ljava/lang/String;I)I
requestMTX (Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I
exitApp ()V             phnCallPlace ([B)I
```

### 앱 셸이 재현하지 않아도 되는 것

APK 에 SKT 광고 SDK 가 들어 있다 — `com.sktelecom.tad.sdk.AdPopupView` /
`AdPopupActivity` / `IntroVideoActivity`, 그리고 `co.kr.eamobile.drm.SKTActivity`
(DRM 바인딩). 통신사 서비스가 없어진 지 오래이고 게스트 `.so` 는 이들을 직접
참조하지 않는다. **새 셸에는 넣지 않는다.**

원본이 요구하던 권한 15종 중 실제로 게스트 경계면에 대응하는 것은 넷뿐이다:
`INTERNET` (NETWORK), `VIBRATE` (`vibration`), `CALL_PHONE` (`phnCallPlace`),
`READ_PHONE_STATE` (단말 식별). 나머지(`WRITE_SETTINGS`, `DEVICE_POWER`,
`KILL_BACKGROUND_PROCESSES`, `ACCESS_FINE_LOCATION` 등)는 통신사 SDK 것이다.

### `assets/mtouchdevice.txt` — 터치 화이트리스트

```
SHW-M110S
SHW-M180S
SHW-M250S
NexusS
```

39바이트짜리 단말 목록이다. `CletEntry.<init>` 이 `checkTouchType` 을 부른다.
호스트가 보고하는 모델명이 이 목록에 없을 때 게임이 터치를 끄고 키패드로
떨어지는지 확인해야 한다 — 확인 전까지는 목록의 이름 하나를 보고하는 것이 안전하다.

---

## 4. 그래픽 — GL 을 거의 쓰지 않는다

**가능한 최선의 경우다.** `tools_re/xref.py` 로 GL 호출 지점을 전부 찾았다:

```
glDrawArrays      3곳 — 전부 Texture2D::update() 안
glTexImage2D      1곳 — Texture2D::createTexture()
glTexSubImage2D   1곳 — Texture2D::bindTextureFromDirect(int,int,int,int,unsigned char*)
```

게스트는 **자기가 프레임버퍼에 픽셀을 직접 쓰고**, GL 은 그것을 화면에 올리는
데만 쓴다. 스프라이트마다 GL 호출이 나가는 구조가 아니다.

**따라서 GL 구현이 필요 없다.** 그 호출들을 지켜보다가 사각형 하나를 CPU 로
확대 복사하는 상태 기계면 충분하고 — 크로노스의 `cs_core/src/gl.rs` (342줄) 가
바로 그것이다 — 덕분에 헤드리스 러너와 실제 앱이 완전히 같은 렌더 경로를 쓴다.
이것이 나중에 결정적 재생을 가능하게 한다.

### 다만 블리터 계층은 완전히 다르다 — 여기가 이 프로젝트의 본체다

크로노스는 WIPI-C 였고 `DM_*` 로 시작하는 평평한 C 함수 82개가 픽셀을 그렸다.
히어로즈 로어 5 는 **C++ 이고 `DM_*` 가 하나도 없다.** 대신 `Graphic` 클래스의
메서드 **118개**가 그 자리에 있다:

| 게스트 함수 | 크기 |
|---|---|
| `Graphic::DrawImageBlendingClip565R(int,int,int,int,int,GbmImage*,int,int,uchar,uchar,uchar,int)` | 7,984 B |
| `Graphic::DrawImageBlendingClip565(...)` | 7,896 B |
| `Graphic::DrawImageRotateEx5Clip565(...)` | 2,952 B |
| `Graphic::DrawImageMirrorClip565R(...)` | 2,728 B |
| `Graphic::DrawImageMirrorClip565(...)` | 2,500 B |
| `Graphic::DrawImageContortClip565(...)` | 1,640 B |
| `Graphic::DrawImageStretchClip565(...)` / `565R` | 1,500 / 1,464 B |
| `Graphic::BoxBlur565` / `BoxBlur666` | 648 / 652 B |
| `Graphic::SetBlendingTable565` | 1,360 B |

**크로노스의 `patch.rs` (8,879줄) 는 전이되지 않는다.** 그것은 `DM_*` 를 한 함수씩
호스트로 이식한 결과물이고, 여기서는 대응물이 없다. 다만 **방법론과 검증 절차는
그대로다** — 프로파일로 이름을 지목하고, 그 함수를 비트 단위로 호스트에 옮기고,
덤프 프레임이 픽셀 단위로 동일한지로 증명한다. `ab_compare.py` 가 그 판정을 한다.

크로노스에서 `BoxBlur` 가 메뉴 열 때 1.2초를 먹었다는 기록이 있는데, 히어로즈
로어 5 에도 `BoxBlur565`/`BoxBlur666` 이 그대로 있다. 같은 함정을 예상한다.

### 폰트는 공유된다

`_midas_funcFntDrawString` (5,500 B) 을 비롯한 `_midas_funcFnt*` 49개가 크로노스와
공유되고 크기까지 같다. 크로노스가 이미 호스트로 이식한 폰트 스택이 그대로 온다.
게스트 문자열은 EUC-KR 이다 (크로노스에서 확인된 사실이고 같은 계층이다).

---

## 5. 에셋 / 아카이브 포맷 — 크로노스와 다르다

`payload/data.vfs.mp3`, **16,399,297 바이트**. 확장자와 달리 mp3 가 아니라 게임
에셋 아카이브 전체다.

```
크로노스: 0b 19 ec 9e | 31 63 00 00 | 17 00 00 00 | 5f 04 00 00 | 0b 30 00 2d ...
히어로즈: 65 82 83 cc | 80 02 00 00 | 49 d3 9d bb | bc 25 83 f3 | 1b 1d 04 fe ...
```

크로노스는 헤더에 평문 카운트가 보인다. **히어로즈 로어 5 는 0바이트부터 고엔트로피다**
— 아카이브가 암호화돼 있거나 프레이밍이 다르다. 크로노스의 VFS 리더는 그대로
쓸 수 없다.

게스트 안에 도구는 다 있다:

```
DES     MX_desInit, MX_desEncrypt/Decrypt, MX_des{En,De}cryptPKCS7,
        desRound, startDes, DESencrypt/DESdecrypt, new_encrypt/new_decrypt
LZMA    LzmaDecode(1076 B), LzmaLen/Literal/LiteralMatchDecode,
        RangeDecoder{Init,BitDecode,BitTreeDecode,DecodeDirectBits,
        ReverseBitTreeDecode,ReadByte}, MX_lzmaDecode, MX_lzmaDecodeSize
PNG     _knlPngDecode, _knlPngDecodeByte, _knlPngDecodeLengths,
        _knlPngDecodeBlockHeader, _knlPngDecodeLiteralByte,
        _knlPngDecodeCompressedByte
VFS     MIDASKernelManager::{setVFSInfo, getAssetSizeFromVFS(640 B),
        loadAssetFromVFS, loadSoundFromVFS, loadMusicFromVFS}
파일     StaticUtil::{LoadResDecrypt, LoadDecryptFile, SaveEncryptFile}
```

**정공법은 게스트에게 물어보는 것이다.** 크로노스의 DES 이식에서 이미 확인된
원칙이다 — 교과서 DES 를 그대로 쓰면 세이브가 깨진다. 게스트의 `startDes` 를
후킹해서 실제 키와 모드를 관찰한 뒤 호스트 구현을 맞추고, **게스트 자신의 출력과
대조해서** 검증한다.

호스트가 VFS 를 해석할 필요가 있는지부터 따져야 한다. `loadAssetFromVFS` 는
게스트 안에 있으므로, 호스트는 `getAssetSize`/`loadAsset` JNI 콜백에 **원본
바이트를 그대로** 돌려주기만 하면 될 가능성이 있다. 그렇다면 5절의 작업은 대부분
사라진다. **브링업에서 가장 먼저 확인할 것.**

---

## 6. 지금 상태 — 브링업 1단계 통과 (2026-08-18)

로더·게스트 주소공간·트램폴린·ARMv5TE 해석기가 실제 바이너리에서 돈다.
**C++ 정적 생성자(`_GLOBAL__I_._code_Heroslore5.cpp`)가 773 게스트 명령어에
폴트 없이 완주한다.** 2절의 함정 세 가지 중 `.init_array` 와 `R_ARM_ABS32` 는
처리됐고, `DT_TEXTREL` 은 비용이 없는 것으로 판명됐다 — 게스트 메모리가 보호
없는 평평한 바이트 배열이라 텍스트로 가는 재배치가 그냥 평범한 store 다.

```
image        0x01000000..0x01176b68  (1498 KB)
symbols      3,764 defined
imports      76 트램폴린 (= 77 − 데이터 심볼 __stack_chk_guard)
exidx        0x0115b7c8 + 26,320 B (3,290 엔트리)
.init_array  0x01162e98 x1 -> 0x0109e168 _GLOBAL__I_._code_Heroslore5.cpp
.fini_array  0x01162e9c x1 -> 0x0109e170 _GLOBAL__D_._code_Heroslore5.cpp
```

HLE 는 77개 중 17개 구현 — allocator, `mem*`, `str*`, `__gnu_Unwind_Find_exidx`,
`__stack_chk_fail`, `__android_log_print`. 남은 59개는
`cargo run -p hl5_probe -- --todo` 가 그룹별로 낸다:
GLES 19 / files 18 / sockets 8 / libc 6 / libm 3 / C++ ABI 3 / threads 2.

성능은 **아직 측정하지 않았다.** 지금까지 실행한 것은 773 명령어뿐이고, 거기서
초당 처리량을 말하는 것은 의미가 없다. 규모만 말하면 `.text` 1,200,084 바이트는
크로노스(899,052)의 1.33배다. 크로노스는 인터프리터만으로 부팅해 플레이 가능한
속도에 도달했고, 핫 함수를 호스트로 옮긴 뒤 2600프레임 재생에서 게스트 명령어
151억 → 7,200만(약 210배), 33 ms 초과 프레임 205개 → 1개가 됐다. 같은 구조가
여기서도 성립할 것으로 보지만 **이것은 예상이지 측정이 아니다.**

성능에 대해 지금 말할 수 있는 것은 규모뿐이다: `.text` 1,200,084 바이트는
크로노스(899,052)의 1.33배다. 크로노스가 인터프리터만으로 부팅해서 플레이 가능한
속도에 도달했고, 핫 함수를 호스트로 옮긴 뒤 2600프레임 재생에서 **게스트 명령어
151억 → 7,200만 (약 210배)**, 33 ms 예산 초과 프레임 **205개 → 1개**가 됐다.
같은 구조가 여기서도 성립할 것으로 본다. **다만 이것은 예상이지 측정이 아니다.**

아래는 **크기순 상위 게스트 함수**로, 프로파일 전의 후보 목록일 뿐 측정된 핫스팟이
아니다. 크기와 비용은 다른 것이다.

```
18,184  TargetEffect::Proc()
17,704  Monster::Proc()
16,672  StateInGameMenu::DrawSkillMenu()
14,104  StatePopup::Paint()
 9,116  HERO::CalcStatusComputation(uchar)
 7,984  Graphic::DrawImageBlendingClip565R(...)
 7,896  Graphic::DrawImageBlendingClip565(...)
 5,500  _midas_funcFntDrawString          ← 크로노스와 공유, 이미 이식됨
```

---

## 7. 남은 것 — 착수 순서

1. ~~`libHeroesLore5.so.idb` 를 연다.~~ **완료 (2026-08-18).** 순수 자동 분석
   결과물이었다. 건진 것은 경계면 구조체 8개의 필드 이름뿐이고 계획은 그대로다.
   2절의 "APK 안의 IDA 데이터베이스" 참조.
2. **`rehost-bringup`** ← **지금 여기, 1단계 통과.** 주소공간·ELF32 로더
   (`.init_array`·`R_ARM_ABS32` 포함)·트램폴린·정적 생성자까지 끝났다(6절).
   남은 것은 HLE 59개 → JNI → `boot()` → 첫 프레임. 작업 목록은 `--todo` 가 낸다.
3. **VFS**: 호스트가 아카이브를 해석해야 하는지 먼저 판정 (5절 마지막 문단).
4. **`deterministic-probe`** — 가상 시계, 스크립트 입력, 프레임 덤프, 게스트 PC
   프로파일러. 크로노스의 `cs_probe` 가 그대로 온다.
5. **`cpu-differential-test`** — Unicorn 대조. 크로노스의 `armdiff` 가 그대로 온다.
6. **`frame-optimization`** — `Graphic::` 118개 중 프로파일이 지목하는 것부터
   호스트로. **이 프로젝트에서 가장 큰 덩어리이고, 크로노스에서 전이되지 않는다.**
7. **`network-rehost`** — `NETWORK` 클래스는 크로노스보다 훨씬 크다:
   `ProcPacket_{Login,Patch,Mail,Shop,Inventory,Community,Ranking,Event,Notice,
   System}` + `PacketWrite_*` + `UserLog`. 원본 서버는 없어졌다. 크로노스의
   `cs_server` 는 구조만 참고하고 opcode 는 새로 뜯어야 한다.
8. **`ship-64bit-android`** — 서명, 업데이트 호환, **세이브 보존**.

### 착수 전에 확인해 둘 것 — 확보됐다

원본이 도는 32비트 기기가 있으면 "이게 버그인가 원래 그런가"를 몇 초에 판정할 수
있다. **대상 기기(S25 Ultra)는 그 기기가 아니지만, 갤럭시 S9 이 그 기기다** —
9절 참조. 원본이 설치되고 부팅되고 클래스선택까지 플레이된다.

---

## 9. 대상 기기에서의 실측 — 원본은 설치조차 되지 않는다 (2026-08-18)

1절의 "불가능하다"를 실제 기기에서 확인했다. 추정이 아니라 측정이다.

### 기기

```
모델        SM-S938N (갤럭시 S25 Ultra, samsung/pa3qksx)
안드로이드  16 (SDK 36)
abilist     arm64-v8a          abilist32  (비어 있음)
zygote      zygote64
/system/bin/linker     없음    ← 32비트 동적 링커가 존재하지 않는다
/system/bin/linker64   있음
/system/lib            내용물이 `modules` 하나뿐
```

ABI 목록이 비어 있는 정도가 아니다. **32비트 링커와 시스템 라이브러리가 아예
없어서 32비트 ELF 를 적재할 방법 자체가 없다.**

### 설치 시도

```
$ adb install -r heroesLore5.apk
Performing Streamed Install
adb: failed to install heroesLore5.apk:
  Failure [INSTALL_FAILED_NO_MATCHING_ABIS:
           Failed to extract native libraries, res=-113]
```

### 대조군이 결론을 고정한다

형제 재호스트 `kr.chronos.sword` 는 **이 기기에 이미 설치돼 있다**:
`primaryCpuAbi=arm64-v8a, minSdk=26, targetSdk=35`. 기기·adb·서명·USB 디버깅에
문제가 없다는 뜻이고, 실패의 원인은 원본 APK 가 32비트라는 사실 하나로 좁혀진다.
동시에 이것이 **재호스팅 방식이 이 기기에서 실제로 통한다는 증거**이기도 하다.

`aapt2 dump badging` 대조:

| | heroesLore5.apk | chronosSword1_v0.80.apk |
|---|---|---|
| `native-code` | **`armeabi`** | `arm64-v8a` |
| `minSdkVersion` | 5 | 26 |
| `targetSdkVersion` | **선언 없음** | 35 |
| `compileSdkVersion` | 23 (`6.0-2438415`) | 35 |
| `versionName` | 01.00.08 | 0.1.0 |
| label | `영웅서기5 (@litfrozen)` | 크로노스소드 |

### 차단 요인은 둘이고, 하나만 고쳐서는 안 된다

1. **ABI — 관측됨.** `armeabi` ↔ `arm64-v8a`. 위의 `res=-113`.
2. **targetSdk — 예측, 미관측.** 원본은 `targetSdkVersion` 을 선언하지 않아
   `minSdkVersion` 5 로 취급된다. Android 14 부터 낮은 targetSdk 앱의 설치를
   막고 하한이 릴리스마다 올라간다. **ABI 검사가 먼저 걸려서 이 오류는 실제로
   보지 못했다** — 규칙에서 나온 예측이지 측정이 아니다.

그래서 "원본 APK 의 `.so` 만 64비트로 갈아끼워 재패키징" 은 성립하지 않는다.
어차피 새 셸을 쓸 것이라 실무적으로는 무의미한 제약이지만, 착각하기 쉽다.

**우리 APK 가 만족해야 할 것**: `native-code` 는 `arm64-v8a` 하나, `minSdk` 26+,
`targetSdk` 35+.

### 재현

```powershell
$adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"     # PATH 에 없다
& $adb devices -l
& $adb shell getprop ro.product.cpu.abilist32     # 빈 문자열
& $adb shell "test -e /system/bin/linker && echo YES || echo NO"  # NO
& $adb install -r heroesLore5.apk                 # res=-113
& "$env:LOCALAPPDATA\Android\Sdk\build-tools\36.1.0\aapt2.exe" dump badging heroesLore5.apk
```

---

## 10. 정답지 — 갤럭시 S9 에서 원본이 그대로 돈다 (2026-08-18)

7절이 "착수 전에 확인해 둘 것"으로 꼽은 항목이고, **확보됐다.** 원본이 아직
도는 32비트 기기를 옆에 놓을 수 있으면 "이게 우리 버그인가 원본도 그런가"가
몇 초짜리 질문이 된다. 크로노스에서 겹쳐 보이는 폰트, 이상한 배율, 못생긴 UI
정렬의 대부분이 **원본도 똑같았다**.

```
모델        SM-G960N (갤럭시 S9, samsung/starlteks, Exynos 9810)
안드로이드  10 (SDK 29)
abilist     arm64-v8a,armeabi-v7a,armeabi
abilist32   armeabi-v7a,armeabi          ← 원본이 요구하는 armeabi 가 있다
zygote      zygote64_32                  /system/bin/linker 있음
```

9절의 차단 요인 둘이 **모두 없다**: `armeabi` 를 지원하고, SDK 29 는 낮은
targetSdk 차단(Android 14+)이 도입되기 전이다.

```
$ adb install -r heroesLore5.apk
Performing Streamed Install
Success
```

설치 결과:

```
primaryCpuAbi=armeabi
versionCode=10008  minSdk=5  targetSdk=5
legacyNativeLibraryDir=/data/app/co.kr.eamobile.HeroesLore5-…/lib
런처 액티비티 = co.kr.eamobile.HeroesLore5/.HeroesLore5
```

**`targetSdk=5` 는 9절의 예측을 실측으로 바꾼다** — 원본이 `targetSdkVersion` 을
선언하지 않아 플랫폼이 `minSdkVersion` 5 로 취급한다는 것. (그 값으로 Android 14+
가 설치를 막는다는 부분은 여전히 규칙에서 나온 예측이고, S25 에서는 ABI 검사가
먼저 걸려 관측되지 않았다.)

### 실제로 플레이된다

```
adb shell am start -n co.kr.eamobile.HeroesLore5/.HeroesLore5
```

부팅 → `Connecting`(없어진 원본 서버를 찾다가 타임아웃) → 타이틀 →
**클래스선택**(워리어 / 로그 / 건슬링어 / 나이트). 한글도 정상 렌더링된다.
포커스를 잡는 액티비티는 `MidasActivity` 다.

캡처 두 장이 `docs/reference/` 에 있다:

| 파일 | 무엇 |
|---|---|
| `original-s9-01-connecting.png` | 부팅 직후. 흰 배경, 중앙에 `Connecting` 다이얼로그와 9칸 진행 표시 |
| `original-s9-02-class-select.png` | 클래스선택. 타이틀 일러스트 위에 4개 타일, 하단에 클래스 설명문 |

**이 캡처들은 픽셀 대조용이 아니다.** 원본 논리 화면은 400x240 이고 이것은 S9
서피스로 확대된 결과라 우리 프로브의 덤프와 직접 비교할 수 없다. 용도는 "이
화면이 원래 어떻게 생겼는가" 다 — 레이아웃, 폰트 모양, 색, 정렬.

### 정답지의 한계

* **네트워크 구간은 원본도 못 넘는다.** 원본 서버가 없어졌으므로 `Connecting`
  이후의 온라인 기능(편지함·상점·랭킹)은 이 기기로도 확인할 수 없다.
  → `network-rehost` 에서 사설 서버를 세운 다음에야 대조가 가능하다.
* 배율·여백은 S9 화면(2220x1080)에 맞춰진 것이라 우리 대상 기기와 다르다.

---

## 8. 재현용 도구

`tools_re/` — 전부 게임 무관하게 동작하고, 이 문서의 모든 수치를 다시 낸다.
의존성: `pip install pyelftools capstone pillow unicorn` (설치돼 있음).

```bash
python tools_re/seam_census.py payload/libHeroesLore5.so          # 1·2절 전부
python tools_re/xref.py payload/libHeroesLore5.so glDrawArrays    # 4절
python tools_re/disasm.py payload/libHeroesLore5.so <symbol> --frame
python tools_re/dexconst.py payload/classes.dex CletEntry         # 3절 400x240
python tools_re/dexstrings.py payload/AndroidManifest.xml payload/classes.dex
python tools_re/symtab.py                                          # 심볼 덤프 재생성
python tools_re/seamdiff.py \
    C:/ChronosSwordNativeAPP/payload/libchronosMian.so \
    payload/libHeroesLore5.so CHRONOS HERO5                        # 1절 대조표
```

방법론 출처: `apkdeployer2580/32bit-to-64bit-app-migration` (Claude Code 플러그인
마켓플레이스). 참조 구현: `C:\ChronosSwordNativeAPP`.

## 11. 브링업 2단계 — import 76개 완주와 JNI 골격 (2026-08-17)

`--todo` 의 59개를 전부 구현해 **import 76/76 서빙**이 됐다. 검증은
`--selftest`: HLE 를 러스트 함수로 직접 부르는 게 아니라 **트램폴린 주소를
`Vm::call` 로 불러** 게스트와 같은 경로(인자 디코딩 → 디스패치 → 복귀)를
두드린다. 20개 검사 전부 통과.

설계 결정과 근거:

* **printf (`fmt.rs`)** — 바이트 단위 엔진. `%s` 는 EUC-KR 을 그대로
  통과시킨다. 가변 인자는 `VaSource` 가 두 경로를 안다: 호출 경계에
  레지스터/스택으로 남아 있는 것(`__android_log_print`)과 `va_list` 로 게스트
  메모리에 스필된 것(`vsnprintf`). 64비트 인자는 AAPCS 대로 짝수 슬롯/8바이트
  정렬로 읽는다.
* **qsort 재진입** — 비교자는 게스트 코드다. 정렬 중 원본 배열을 건드리지 않고
  인덱스만 머지소트하므로, 비교자가 받는 포인터는 항상 실제 원소 주소다.
  셀프테스트는 비교자로 **strcmp 트램폴린**을 넘겨 재진입 경로까지 확인한다.
* **소켓 = 죽은 서버의 결정적 재현** — S9 정답지가 보여준 "Connecting → 자체
  타임아웃 → 타이틀"을 재현한다. connect 는 실패(논블로킹이면 EINPROGRESS),
  select 는 0을 돌려주며 **timeval 만큼 가상시계를 전진**시키고(게스트의 대기가
  벽시계 시간을 쓰지 않는다), SO_ERROR 는 ETIMEDOUT 을 읽어 준다.
* **파일 = storage 아래로 재루팅** — 게스트가 어떤 절대 경로를 대도 컴포넌트
  단위로 `Config::storage` 아래에 다시 놓는다. 이름이 `data.vfs.mp3` 인 경로는
  디스크가 아니라 `Config::vfs` 메모리에서 읽기 전용으로 서빙한다.
* **`FILE*` 는 불투명 핸들** — stdio 임포트가 fread/fseek/fclose 뿐이므로
  게스트는 FILE 내부를 보지 않는다. 핸들 값은 JNI 창의 매핑된 빈 영역이다.
* **C++ ABI 3종은 진단이 구현이다** — `__cxa_begin_cleanup` 등은 실제 throw 가
  진행 중일 때만 불린다. EHABI 언와인더 없이 다른 값을 돌려주는 것이야말로
  "조용한 0"이므로, 이름을 달고 죽는 것이 맞다.
* **GLES 는 캡처 상태 기계 (`gl.rs`)** — 4절의 측정(모든 draw 가
  `Texture2D::update` 안)에 따라 텍스처 픽셀을 호스트에 저장하고 DrawArrays 에서
  ortho→viewport 변환으로 대상 사각형을 구해 CPU 최근접 확대 복사한다. 매핑할
  수 없는 draw 는 추측하지 않고 원시 인자를 로그에 남긴다.

**JNI 골격 (`jni.rs`)** — JNIEnv 233슬롯/JavaVM 8슬롯 함수 테이블이 트램폴린을
가리키고, 룩업 계열(FindClass/Get*MethodID/Get*FieldID)은 이름을 인터닝해
핸들로 돌려준다. 핸들이 곧 설명이라, 미구현 `CallStatic*` 는
`CletEntry.putSoundClip(JJJI)I` 처럼 **대상 이름을 달고** 죽는다 — import 의
`--todo` 가 하던 역할을 JNI 에서는 이 에러가 한다.

그 결과 **`nativeInitKernel` 이 완주한다** (699 명령어, HLE 66회, 반환 0).
부수 측정: `AndroidService` 생성자가 룩업하는 CletEntry 콜백은 55개다
(`tools_re/jni_lookups_nativeInitKernel.txt`). seamdiff 의 "19개 동일"은
크로노스와 겹치는 부분집합 기준이었다 — 오디오 클립 3계열(사운드/뮤직/일반),
시스템 정보(getModel/getIMEI/…), MTX 결제(initMTX/requestMTX), 진동, 전화/웹
런처까지 전부 이 목록에 있다. 다음 증분은 이 중 실제로 불리는 것부터의 구현이다.

## 12. 브링업 3단계 — CletEntry 콜백·JNI Call·boot() 완주 직전 (2026-08-17)

`nativeInitKernel` 은 2단계에서 완주했다. 3단계는 **경계면의 Java 쪽 절반**을
세워 `onSurfaceCreated` 의 전체 boot 순서를 재현하는 것이다.

### boot 순서는 dex 에서 실측했다

`classes.dex`(androguard)로 `CletEntry.onSurfaceCreated` 를 디컴파일해 순서와
인자를 확정했다:

```
nativeInitKernel()
nativeInitDisplay(screenW, screenH, r=5,g=6,b=5,a=0, wipiW=400, wipiH=240,
                  marginX=0, marginY=0, colorType, magnification(f32),
                  magFilter, viewMode, useSublayer, sublayerPixel) → autoMarginY
nativeInitFileStorage(internalPath, externalPath)
nativeInitVFS(FileDescriptor, startOffset:long, length:long)
nativeInitAudioConfiguration(202400)          ; const-wide/32 리터럴
nativeStartApp()
```

RGB565(5,6,5,0)는 `MidasGLSurfaceView.<init>` 의 `setDisplayInfo` 상수이고,
400×240 은 `adjustAutoScreen`, 202400 은 `onSurfaceCreated` 의 리터럴이다.
`Vm::boot()` 이 이 순서를 그대로 호출한다.

### JNI Call 계층

게스트의 인라인 varargs 래퍼(`_JNIEnv::CallStaticIntMethod`)는 인자를 스택에
스필하고 **`CallStaticIntMethodV`(슬롯 130)** 를 부른다 — vsnprintf 와 똑같은
va_list 경로다. 그래서 `dispatch_jni` 는 `CallStatic*MethodV` 를 받아 인터닝된
메서드 이름으로 `clet_call` 에 분기하고, 인자는 `VaSource::List` 로 읽는다.
문자열/배열 지원(`NewStringUTF`, `GetStringUTFChars`, `NewByteArray`,
`GetByteArrayElements`, `Get/SetByteArrayRegion`, `GetArrayLength`)과
객체 int 필드(`GetIntField`)까지 붙였다 — 핸들은 게스트 힙 주소 그 자체이므로
`GetStringUTFChars`/`GetByteArrayElements` 는 항등함수다.

### CletEntry 콜백은 dex 로 동작을 읽어 구현했다

* `getAssetSize`/`loadAsset` — `MidasAssetManager` 가 `AssetManager.open(name)`
  으로 APK `assets/` 파일을 여는 것이므로, 호스트는 `Config::assets` 에서 읽는다.
  **이건 VFS 아카이브가 아니라 개별 에셋 파일**이다.
* `getUniqueNumberFromStrings`/`getUniqueAssetNameFromID` — Java 는 해시가 아니라
  `id = table.size()+1` 순차 부여 + 양방향 저장이다. 호스트는 `Vec` 양방향
  매핑으로 정확히 재현한다.
* 디바이스/메모리/네트워크 프로브 — 상수(`Config::model`, 넉넉한 메모리, 네트워크
  up). 오디오 클립 — 성공 응답(믹서는 다음 증분). `initMTX`/`requestMTX` — 성공
  (오프라인 통과, `docs/SERVER.md` §6.6).

### boot 은 InitGlobalValue 의 VFS 데이터 로드 직전까지 완주한다

`nativeInitVFS` 가 `FileDescriptor.descriptor`(int)를 `GetIntField` 로 꺼내
`dup`/`fdopen` 하는 것을 실측했다 — 그래서 boot() 은 메모리 VFS 를 여는 fd 를
`FileDescriptor` 객체에 담아 넘긴다. 이후 `getAssetSizeFromVFS` 가 그 fd 로
아카이브 헤더를 읽는 데까지 성공한다.

boot 은 InitKernel → InitDisplay → InitFileStorage → InitVFS →
InitAudioConfiguration → StartApp → `InitGlobalValue` 까지 **약 2만 게스트
명령어를 폴트 없이** 진행하고, `InitGlobalValue` 가 게임 데이터를 로드하려는
지점에서 멈춘다.

### 남은 게이트: VFS 인덱스 디코드 (다음 세션 1순위)

`InitGlobalValue` → `StaticUtil::LoadRes(name, &len)` (`0x144E80`) 은 `/c/...`
경로(예: `/c/font/table.dat`, `/c/csv/shop_%d.dat`,
`/c/csv/cash_single_android.dat` = **캐시샵 데이터**)를 로드한다. LoadRes 는
`MC_fsFileAttribute` 로 **호스트 FS(`stat`)를 먼저** 보고(파일이 storage 아래
없으면), VFS 아카이브로 폴백한다. VFS 폴백은 `getAssetSizeFromVFS` (`0x151968`)
→ `MIDASKernelManager::hash(name)` 로 인덱스를 룩업한다.

핵심 판정이 여기서 뒤집혔다: **CLAUDE.md 가 "loadAsset 콜백에 원본 바이트만
돌려주면 된다"고 본 것은 개별 에셋(assets/*)에만 맞다.** 게임 본체 데이터
(`/c/...`)는 **게스트의 VFS 파서(`getAssetSizeFromVFS`/`loadAssetFromVFS`)가
해시 인덱스로 아카이브를 파싱**해서 꺼낸다. 즉 다음 게이트는:

1. `MIDASKernelManager::hash` 알고리즘과 인덱스 레이아웃을 디코드하거나,
2. VFS 를 `/c/...` 파일 트리로 추출해 storage 에 풀어 `MC_fs` 의 호스트-FS 경로를
   만족시킨다 (아카이브가 암호화면 게스트의 DES/LZMA 로 추출해야 한다).

이 디코드가 첫 프레임(그리고 캐시샵이 실제로 뜨는 것)의 유일한 남은 관문이다.

### 프로브 도구

```
cargo run -q -p hl5_probe -- --boot                 # nativeInit* 순서 완주 시도
cargo run -q -p hl5_probe -- --boot --frames 1 --dump out/frame0.png
cargo run -q -p hl5_probe -- --boot --server 10.0.0.5:7300   # 실서버 연결
```

`--dump` 는 의존성 없는 PNG 라이터(stored-DEFLATE)로 `gl.fb` 를 떨군다.

---

## 13. 브링업 4단계 — 게임이 돈다 (2026-08-18)

3단계의 게이트는 "VFS 인덱스 디코드"로 보였다. 디코드는 30분이면 끝났고,
진짜 원인은 호스트 쪽 인자 정렬 버그 하나였다. 그 뒤로는 계속 굴러갔다.

### 13.1 VFS — 암호화가 아니었다

`getAssetSizeFromVFS` (`0x151968`) 를 읽으면 인덱스는 순차 리스트다:

```
[u32 hash][u32 size][size 바이트]   반복
```

`hash` 는 `MIDASKernelManager::hash` (`0x1516a4`) = djb2 (`h = h*33 + c`,
초기값 5381). 키는 **앞의 `/` 를 뗀 경로** — `tempLocalGetExactFilename`
(`0x387d0`) 이 선두 `/`·`\` 를 건너뛴다. 이 알고리즘으로 `data.vfs.mp3` 를
걸으면 **2,189 엔트리가 파일 끝(16,399,297)에 정확히 맞아떨어진다.** 헤더가
고엔트로피로 보였던 것은 첫 워드가 해시값이었기 때문이다.

즉 CLAUDE.md 의 원래 판정("호스트는 바이트만 돌려주면 된다")이 맞았고,
3단계에서 그것을 뒤집은 것이 성급했다.

### 13.2 진짜 원인 — `nativeInitVFS` 의 jlong 정렬

AAPCS 는 `jlong` 에 짝수 레지스터 쌍을 준다. `(JNIEnv*, jclass, jobject,
jlong, jlong)` 에서 r0/r1/r2 를 쓰고 나면 다음 가용 레지스터가 홀수 r3 이므로
**r3 은 패딩**이 되고 두 long 이 전부 스택으로 간다. 엔트리포인트가 실제로
읽는 곳도 그렇다 — `ldr sl, [sp, #0x20]` / `ldr sb, [sp, #0x28]`.

호스트가 패딩 워드를 빼먹어 `length` 가 0 으로 도착했다. 그러면
`getAssetSizeFromVFS` 의 루프 조건(`cursor < length`)이 **첫 엔트리 하나를 걸은
직후** 깨져서 모든 룩업이 -1 을 돌려준다. 증상은 `InitGlobalValue` 가 NULL 을
`ByteToInt16` 에 넘기는 폴트였다 — 원인에서 두 단계 떨어진 곳이다.

**교훈으로 남길 것**: JNI 진입점의 `jlong`/`jdouble` 인자는 레지스터 배치가
직관과 다르다. 새 진입점을 부를 때는 `disasm.py --frame` 으로 엔트리포인트가
어느 슬롯을 읽는지 먼저 본다.

### 13.3 정답지 대조는 측정이 됐다

`tools_re/ref_compare.py` 를 새로 썼다. `ab_compare.py` 는 **우리 재생 두 판의
바이트 동일**을 요구하는 다른 도구고 정답지에는 겨눌 수 없다 — 정답지는
4.5배로 확대돼 레터박스와 내비게이션 글리프가 붙은 폰 스크린샷이다. 이 도구는
그 역변환을 하고 나서 잰다: 게임 사각형을 찾아(또는 `--calibrate` 로 밝은
캡처에서 잡아) 400×240 으로 박스 다운스케일하고 평균·p95 오차를 낸다.

| 화면 | mean \|error\| | p95 | 허용오차 초과 |
|---|---|---|---|
| Connecting | **1.36** | 4 | 1.6% |
| 클래스선택 | 6.45 | 35 | 13.0% |

Connecting 의 차이는 진행 사각형의 애니메이션 위상뿐이다. 클래스선택의 차이는
캐릭터 스프라이트와 배경 불꽃의 애니메이션 위상, 그리고 폰 캡처의 스케일링이다.

**정답지가 값어치를 증명한 첫 사례**: 클래스선택 패널 뒤로 타이틀 로고와
"Tap to Continue" 가 비쳐 보인다. 우리 버그로 보였지만 원본도 S9 에서 똑같다.

### 13.4 소켓 계층 — 실서버를 물려야 드러나는 버그 셋

`--server` 는 3단계에 있었지만 실제 서버를 물려 본 적이 없었다. 물려 보니:

1. **`select` 가 무조건 "전부 준비됨"을 보고했다.**
   `__knl_net_connect_check_timer` (`0x14d124`) 는 타임아웃 0 으로 폴링하고
   **어느 집합이 돌아왔는지로 분기**한다 — 읽기 가능이면
   `recv(s, b, 1, MSG_PEEK)` 로 1바이트를 엿보고 결과가 0 이하면 죽은 연결로
   판정하며, 쓰기만 가능한 것이 논블로킹 connect 가 정상 완료된 경우다.
   갓 붙은 소켓을 읽기 가능으로 보고하면 멀쩡한 서버를 게스트가 먼저 끊는다.
   std 에 `select` 는 없지만 `TcpStream::peek` 이 정확히 그 질문에 답한다.
2. **`recv` 가 `MSG_PEEK` 를 무시했다.** 위의 엿보기가 바이트를 소비해
   서버 첫 프레임의 첫 바이트를 먹는다.
3. **fd 가 단조 증가였다.** `MC_netSocket` (`0x14e324`) 은 `fd >= 0x400` 을
   그 자리에서 거부한다 — `fd_set` 에 넣을 수 없는 번호이기 때문이다.
   게스트는 리소스 룩업 한 번에 VFS 를 두 번 열고 닫으므로 평범한 플레이
   몇 분이면 카운터가 1024 를 넘고 그 뒤 모든 `socket()` 이 실패한다. 증상은
   **한 시간 잘 되던 네트워크가 캐시샵에서만 실패**하는 것이었다. POSIX 처럼
   가장 낮은 빈 번호를 준다.

### 13.5 와이어는 문서와 달랐다

`docs/SERVER.md` 는 108바이트 SmartReq 헤더를 "MTX 전용"으로 적었다. 실제로는
`MX_smartSocketWrite` (`0x41214`) 가 **게임 소켓의 모든 송신**을 감싼다. 응답은
대칭이 아니고 `[u32 4+len][게임 프레임]` 이다. §2.6 을 실측으로 고쳤고,
실측된 로그인 시퀀스 전체를 §6.8 에 넣었다.

### 13.6 지금 어디까지 가는가

`tools_re/scripts/ingame.txt` 한 번에, 폴트 없이:

```
부팅 → 인트로(GRB 등급, EA 로고, 오프닝 텍스트) → 타이틀 → 메인메뉴 →
슬롯선택 → 클래스선택 → 오프닝 컷신(SKIP) → 로딩 → 던전 → 퀘스트 팝업 →
게임플레이 HUD → 저장 → 캐시상점 → 구매 요청(6/2) → 승인(6/4)
```

캡처는 `docs/shots/`. 블리터 계층(`Graphic` 메서드 118개)이 셀프테스트가 아니라
**실제 게스트 draw 로** 처음 검증됐다 — 한글 폰트, 알파 블렌딩, 팔레트,
아이소메트릭 타일, 스프라이트가 전부 동작한다.

### 13.7 아직 못 본 것

지급 패킷 `6/5`. 게스트 자신의 결제 확인(예/아니오) 다이얼로그 뒤에 있고, 그
`예` 를 스크립트로 누르는 데 실패했다. **측정된 사실만**: 좌표 (155,168) 은
맞다 — `--watch onHandleEvent` 로 `onHandleEvent(19, 0x9b, 0xa8, 0)` 이
게스트에 도착하는 것을 봤다. 그런데 `StateCashShop::TouchPressed` (`0xdd5f4`)
가 불리지 않는다. 다이얼로그가 입력을 어디서 가로채는가가 다음 질문이다.
호스트 쪽은 준비돼 있다 — `requestMTX` 가 플래그를 세우고 다음 프레임이
`nativeResultMTX(null, 0, 0)` 을 넣으면 `MTX_Result_CB` 가 `WriteData(6,5)` 를
낸다.

---

## 14. 브링업 5단계 — 기기에서 돈다, 그리고 첫 호스트 블리터 (2026-08-18)

4단계는 헤드리스에서 게임이 도는 것까지였다. 5단계는 그것을 **실제 64비트
기기 위에 올리고**, 프레임 예산을 처음으로 되찾는다.

### 14.1 캐시샵 — "입력이 안 먹는다"가 아니었다

4단계는 "결제 확인 다이얼로그의 `예` 가 안 눌린다"로 끝났다. `--watch` 로
상태 기계를 따라가 보니 다이얼로그는 **제대로 확인되고 있었다**:
`StateCashShop::KeyPressed` → `StatePopup::KeyPressed`, 팝업 타입 2, 그 타입의
SELECT 분기(`0x140434`)가 `gv+0xE1 = 3` 을 쓰고 정상 반환한다.

문제는 그다음이었다. `ProcPacket_Shop` (`0xCD004`) 은 `subcmd - 2` 로
분기하는데 **4번 자리가 그냥 `return`** 이다. 서버가 승인을 subcmd 4 로
보내고 있었고 — `docs/SERVER.md` §6.5 가 그렇게 적고 있었다 — 게스트는 그
번호를 무시한다. 화면에 계속 보이던 확인 다이얼로그는 멈춘 것이 아니라 매번
**새로 시작된 구매**였다.

실제 승인 opcode 는 **2**, 요청과 같은 번호다. 그리고 승인 패킷에 결제
콘텐츠 ID 는 들어가지 않는다 — 성공 경로(`0xCD3F0`)가 클라이언트 자기
카탈로그에서 `MainFrame_getItemContentsID` / `...RealID` 로 뽑아
`MX_mtxRequest` 를 부른다. 고치자 전 과정이 와이어에서 관측됐다:
`6/2 → 6/2 → 6/5 → 6/5 → 6/7 → 6/7`.

**남길 교훈**: 이 문서가 "승인은 subcmd 4"라고 적은 것은 디스어셈블을 읽고
쓴 것이었다. 읽기는 맞았고 *어느 방향인지*가 틀렸다. 계측기(`testserver.py`)를
붙여 실제로 오간 것을 보기 전에는 확정하지 않는다.

### 14.2 오디오 — 믹서를 쓸 일이 아니었다

"오디오 믹서"를 착수하기 전에 게스트가 무엇을 요구하는지부터 쟀다.
`--watch _ZN18MIDASKernelManager16loadSoundFromVFSExPKci` 가
`c/snd/eff_00.ogg` 를 찍는다 — **아카이브 안의 Ogg Vorbis** 다. 그리고
`classes.dex` 의 `MidasSoundManager.load` 는

```
SoundPool.load(vfsfd, vfsOffset + offset, length, 1)
```

로 **APK 에셋의 바이트 구간**을 그대로 넘긴다. 플랫폼 디코더가 의도된
디코더다. Rust 로 Vorbis 디코더를 쓰는 것은 대상 기기가 이미 가진 것을 다시
만드는 일이다.

그래서 `hl5_core/src/audio.rs` 는 **핸들 테이블과 바이트 구간**이다. 검증은
`--dump-audio`: 게임플레이 한 판에서 등록된 22개를 전부 파일로 떨구면
**22/22 가 온전한 Ogg Vorbis 스트림**이다(식별 헤더가 읽히고 마지막 페이지에
EOS 플래그가 있다). 효과음 모노 22050 Hz, BGM 스테레오 22050 Hz. 오프셋과
길이를 `long long` 인자에서 정확히 읽었다는 뜻이다.

### 14.3 fd 는 POSIX 처럼 — 캐시샵만 실패하던 이유

`Files::insert` 가 단조 증가 카운터였다. `MC_netSocket` (`0x14e324`) 은
`fd >= 0x400` 을 그 자리에서 거부한다 — `fd_set` 에 넣을 수 없는 번호이기
때문이다. 게스트는 리소스 룩업 한 번에 VFS 를 두 번 열고 닫으므로 평범한
플레이 몇 분이면 1024 를 넘고 그 뒤 모든 `socket()` 이 실패한다. 증상은
**한 시간 잘 되던 네트워크가 캐시샵에서만 "서버상태가 좋지 않습니다"** 였다.

### 14.4 안드로이드 셸

`hl5_android`(winit/GameActivity + softbuffer) + `android_app`(gradle) +
`build_android.ps1`. 형제 프로젝트에서 구조가 그대로 왔지만 코어 API 가 달라
복사가 아니라 이식이다.

**코어에 없던 것 하나가 여기서 드러났다**: `nativeInitDisplay` 에 배율 1.0 과
여백 0 을 넘기고 있었다. 프로브는 화면 == WIPI 라 티가 안 나지만 폰에서는
400×240 그림이 폰 크기 프레임버퍼의 구석에 그려진다. dex 의
`CletEntry.adjustAutoScreen` 은 디스플레이의 **긴 변/짧은 변**으로 균일
배율을 잡고 가운데 정렬한다:

```
mag = min(long/400, short/240);  marginX = (long - 400*mag)/2;  marginY = ...
```

게스트는 이 세 숫자를 보관하고 자기 사각형을 그 위치에 놓으며,
`onTouchHandleEvent` 는 들어오는 터치를 이 숫자로 나눈다. 그래서 역변환도
같은 숫자여야 한다(`Vm::map_touch`).

**갤럭시 S9 실측** (SM-G960N, 2220×1080): 부팅 0.87초, 배율 4.5, 여백 x=210 —
스크린샷의 그림 위치와 정확히 일치한다. GRB 등급 화면 → (탭) → 불꽃에서 V
로고가 피어오르는 인트로까지 돈다. 원본이 같은 기기에 그대로 설치돼 있고
서로 건드리지 않는다.

### 14.5 첫 호스트 블리터 — 그리고 그것을 지킨 검사

`--profile` 이 이름을 지목했다:

```
66.7%  MC_grpFillRect
21.1%  Graphic::DrawImageBlendingClip565
```

**두 함수가 88%다.** `patch.rs` 는 게스트 **심볼**에서 호스트 함수로 가는
맵이다. 인터프리터가 패치된 함수의 첫 명령어를 실행하려는 순간 대신 호스트
본문을 부르고 lr 로 돌아간다. 주소가 아니라 이름으로 잡는다 — 어긋난 패치는
로드 시점에 죽지 엉뚱한 코드 한가운데로 뛰지 않는다.

인수 기준은 **프레임 바이트 동일**이다. 같은 결정적 재생을 `--no-patch` 로 한
번, 패치로 한 번 돌려 `ab_compare.py` 로 비교한다.

**그 검사가 실제로 버그를 잡았다.** 첫 A/B 는 11개 프레임이 전부 달랐다.
도구가 안내하는 순서대로 세이브 디렉터리를 분리해도 여전히 달랐다 — 즉
이식이 틀린 것이다. 다시 읽으니:

`MC_grpFillRect` 는 `getInstance()` → `getKernel()` 로 시작한다. "커널에게
픽셀 포맷을 묻는다"로 읽히지만 **죽은 코드**다. `getKernel` 은 맨 `bx lr`
(`0x15160c`)이고 그 결과는 버려지며, 실제로 쓰이는 값은 `ldr fp, [r1, #0xc]`
— r1 을 **프레임버퍼 인자**로 다시 읽은 뒤의 `fb+0x0c` 다. 커널 쪽을 읽으면
0x10 도 0x20 도 아닌 값이 나와 아무것도 그리지 않는다.

고친 뒤 **11/11 바이트 동일**. 부수 확인으로 `MC_GrpContext` 의 오프셋이
확정됐다 — APK 에 딸려 온 IDA 데이터베이스가 이름과 순서만 복원했던 그
목록과 정확히 맞는다(mask, clip[4], fgpxl, bgpxl, transpxl, alpha, offsetX,
offsetY, pfnPixelOp, param1).

**측정**:

| | 게스트 명령어 | 기기 fps |
|---|---|---|
| 인터프리터만 | 2,777,048,985 | 5–8 |
| `MC_grpFillRect` 이식 | 924,585,440 | 14.5–18.5 |

한 함수로 3.0배다. 다시 프로파일하면 다음 대상이 드러난다 —
`Graphic::DrawImageBlendingClip565` 63.3%, `FillRectAlpha` 10.2%,
`DrawImageMirrorClip565` 4.7%, `FillRectBlending` 4.6%.

**이 계열을 이식할 때 기억할 것**: `MC_grpFillRect` 의 죽은 `getKernel()` 이
보여 준 대로, 읽히는 대로가 아니라 **레지스터를 따라간다.** 그리고 이식이
맞았는지는 눈이 아니라 `ab_compare.py` 가 말한다.

## 15. 브링업 6단계 — 소리, 공개망, 그리고 2.9배 (2026-08-18)

세 가지를 순서대로 했다: 프로파일이 지목한 블리터를 이식하고, 그 결과 드러난
**진짜** 병목을 고치고, 소리를 냈다. 셋 다 기기에서 확인됐다.

### 15.1 `DrawImageBlendingClip565` — 호출 횟수는 비용이 아니다

`MC_grpFillRect` 를 이식한 뒤 프로파일은 한 이름만 가리켰다:

```
70.5%  Graphic::DrawImageBlendingClip565   (0x5CAE8, 7,896 바이트)
 5.2%  Graphic::DrawImageMirrorClip565
 5.1%  Graphic::FillRectBlending
 4.4%  _midas_funcFntDrawString
```

7.9 KB 짜리 한 함수다. 도우미를 각각 한 번씩만 부르므로 나머지는 전부
인라인된 픽셀 루프고, 디스어셈블을 읽어 보면 **세 겹 선택자** 뒤에 열두 개의
특화 루프가 있다: `level == 0x10` 인가, 이미지가 4비트 팔레트인가, 그리고
변형(`0x10`/`8`/`4`/`0xc`)이 무엇인가.

**첫 번째 함정은 "어느 것을 옮길 것인가"였다.** `--watch` 로 각 분기 머리에서
호출을 세면 이렇게 나온다:

```
2687  +0x1b8   4비트 분기 머리
2462  +0x678   4비트 + 변형 0x10
 854  +0xdc0   8비트 + 변형 0x10
 398  +0xad8   8비트 + 변형 0x0c
  26  +0x1414  4비트 + 변형 8
```

4비트 경로가 압도적으로 많이 불린다. 그런데 **호출 횟수는 비용이 아니다** —
작은 것을 많이 그리는 것과 큰 것을 조금 그리는 것은 다르다. 그래서
`Vm::profile_inside` 와 `--profile-in <심볼>` 을 만들었다. 함수 *안의* 어느
오프셋에서 명령어가 소모되는지 세는 것이고, 답이 뒤집힌다:

```
55.9%  +0x0e00..+0x0f40   8비트 + 변형 0x10   (854회)
36.1%  +0x0b40..+0x0c80   8비트 + 변형 0x0c   (398회)
 2.6%  +0x0740..+0x0880   4비트 + 변형 0x10   (2462회)
```

제일 많이 불리는 경로가 시간의 2.6% 다. **8비트 루프 둘이 92%** 고, 그 둘은
픽셀 산술이 완전히 같으며 목적지 주사 방향만 반대다(0x0c 는 180도 회전이라
오른쪽 아래에서 거꾸로 걸어 나온다). 그래서 그 둘만 옮기고 나머지 열은
`patch.rs` 의 **사양(decline)** 으로 원본에 돌려줬다.

**블렌딩 산술**(0x5D958..0x5DA14). 테이블은 `Graphic::SetBlendingTable565` 이
`this + 0x6D` 에 만들어 두는 레벨당 32×32 바이트 행렬이고, 색 성분 **셋 다
5비트**로 인덱스된다 — 초록도 5비트다. 그래서 결과의 초록만 나갈 때 2배가
되고, 초록 가중치만 0..64 를 쓰며 빨강·파랑은 0..32 를 쓴다. 가중치 셋은
`alpha` 하나에서 나와 `RevisionAlpha565` 가 각각 +1, +2, +1 한다. 투명색은
`SKIP_COLOR` 전역이다.

**도우미 둘은 게스트로 되돌려 부른다.** `CalcAnchor`(104바이트)와
`CalcClipLCD`(1,236바이트)는 블릿당 **한 번씩만** 돌고, 둘을 합쳐도 위
프로파일의 7.5% 안이다. 1,236바이트짜리 클리핑을 전사해 그중 일부를 아끼는
것은 인수 기준의 여유를 통째로 거는 일이라 하지 않았다. 둘 다 이 함수가 넘겨
준 포인터로만 쓰므로 스택 밑에 스크래치를 파 주면 된다. 다만 스크래치는
호출이 볼 `sp` **위**에 있어야 한다 — `Vm::call` 이 자기 스택 인자를 `sp`
아래에 놓기 때문이고, `CalcClipLCD` 는 인자가 열둘이다.

**측정**:

| | 게스트 명령어 | S9 타이틀 fps |
|---|---|---|
| 인터프리터만 | 2,777,048,985 | 5–8 |
| `MC_grpFillRect` + `FillRectAlpha` | 830,153,352 | 18–22 |
| + `DrawImageBlendingClip565` | 290,288,483 | 20.8–25.1 |

fps 가 명령어만큼 줄지 않았다는 것이 다음 절의 출발점이다.

인수 기준은 언제나처럼 `--no-patch` 와의 프레임 바이트 동일이고, 이번에는 네
장면으로 넓혔다 — newgame 119 / ingame 543 / shop 134 / online 109,
**905 프레임 전부 동일, 0 차이**.

### 15.2 그런데 fps 는 2.86배가 안 됐다 — 병목이 호스트로 옮겨 갔다

게스트 명령어가 2.86배 줄었는데 기기 타이틀 fps 는 18–22 에서 20.8–25.1 로만
움직였다. 로그를 보면
이유가 보인다: 타이틀에서 초당 게스트 명령어가 **5.8 M** 인데 프레임의 70% 를
"guest" 로 쓴다. 인터프리터가 초당 천만 개 넘게 돌리는 기기에서 580만 개를
돌리는 데 7초가 걸릴 리 없다. 그 시간은 인터프리터가 아니라 **호스트**가 쓰고
있었다.

`gl.rs` 의 `glDrawArrays` 다. 게스트는 400×240 화면을 텍스처 하나에 담아
사각형 하나로 올리고, 우리 캡처 계층은 그것을 창 크기로 **CPU 에서** 확대
복사한다. S9 에서 목적지는 2220×1080 = **240만 픽셀**이고, 인터프리터가
아무리 빨라져도 매 프레임 그만큼이다. 게다가 픽셀마다 float 나눗셈이 둘
있었다.

열 인덱스는 `x` 에만, 행 인덱스는 `y` 에만 의존한다. 그래서 **같은 식을
글자 그대로** 한 번씩 미리 계산해 표에 담고, 안쪽 루프에는 표 조회와 저장만
남겼다. 산술을 바꾼 것이 아니라 배치만 바꾼 것이고 — 그래서 A/B 가 바이트
동일이다.

| | S9 타이틀 fps |
|---|---|
| 픽셀마다 나눗셈 둘 | 20.8 – 25.1 |
| 열·행 표 | **29.8 – 37.1** |

**교훈**: 프로파일러가 게스트 명령어를 세면 호스트 시간은 보이지 않는다.
게스트를 빠르게 할수록 그 눈먼 구간의 비중이 커진다. `hl5_android` 의 10초
통계 줄(fps, M ins/s, guest ms, present ms)이 이 불일치를 드러낸 유일한
계기였다 — 세 숫자가 서로 맞지 않는다는 것이 단서였다.

메뉴는 아직 8~10 fps 다. 거기서는 프레임당 게스트 명령어가 ~150만이고
프로파일이 평평하다: `DrawImageMirrorClip565` 14.9%, `FillRectBlending`
14.5%, 폰트 스택 합계 ~20%, 그리고 사양된 4비트 경로 15.8%.

### 15.3 소리 — 이벤트 큐와 자바 한 장

경계면은 이미 서 있었다(§14.2): 클립은 아카이브 안의 Ogg Vorbis 이고 원본도
디코드하지 않으며 `SoundPool`/`MediaPlayer` 에 바이트 구간을 넘긴다. 없던 것은
그 구간을 플랫폼까지 나르는 길이다.

**상태로는 부족하다.** 클립 표는 "무엇이 있고 무엇이 재생 중인가"를 말하지만,
그것만 읽는 셸은 "다시 시작했다"와 "계속 울리고 있다"를 구별하지 못한다 —
효과음이 두 번 나야 할 때 한 번 나는 차이다. 그래서 `hl5_core::audio` 가
게스트의 호출을 **`AudioEvent` 큐**로 쌓고, 셸이 프레임마다 비운다.

**자바 한 장**(`Hl5Audio.java`). `AudioAttributes.Builder`,
`SoundPool.Builder`, `MediaPlayer.setDataSource` 를 러스트에서 생 JNI 로
부르면 컴파일러가 검사해 주지 않는 메서드 시그니처 문자열 백 줄이 된다.
`javac` 가 공짜로 해 주는 일이므로 안드로이드 API 사용은 자바에 두고, 러스트
쪽(`hl5_android/src/sound.rs`)은 정적 메서드 일곱 개만 부른다.

두 가지가 실무적으로 중요했다:

* **클래스는 액티비티의 클래스로더로 찾는다.** `android_main` 스레드는 네이티브가
  만든 스레드라 `FindClass` 가 시스템 로더에서 시작하고, APK 안의 클래스를
  보지 못한다. `LoaderContext::FromObject(&activity)` 가 확실한 경로다.
* **`AssetManager.openFd` 는 압축되지 않은 에셋에만 된다.** 원본이 아카이브를
  `.mp3` 로 이름 붙인 이유가 그것이고(aapt 기본 무압축 목록), 우리
  `build.gradle.kts` 도 `noCompress` 로 명시한다. 돌려주는 fd 는 APK 전체를
  가리키므로 모든 오프셋에 `getStartOffset()` 을 더한다 — 원본의
  `vfsOffset + offset` 그대로다.

**S9 실측**: `audio: ready, archive starts at 4577020 in the APK` 뒤에 타이틀
BGM 과 메뉴 BGM 이 MediaPlayer 두 세션(piid 879 → paused, 887 → started)으로
재생됐고, 프레임워크 경고가 하나도 없다. 효과음 21개는 `SoundPool.load` 까지
예외 없이 갔지만 **재생되는 것은 아직 못 봤다** — 측정한 구간에서 게스트가
효과음을 한 번도 재생하지 않는다(헤드리스 재생도 "22개 등록, 1개 재생"이다).

### 15.4 공개망 — 되지만, 셀룰러 단독은 이 기기로 안 된다

8095 가 열린 뒤 `-Server <hero5-server>` 로 구운 릴리스 APK 를 S9 에
설치했다. `adb reverse` 없이, 부팅하자마자 서버 로그에 찍힌다:

```
<공인IP>:56082 login ok account=<16자리> model=SHW-M250S version=HERO5.ALL.06.000
<공인IP>:56082 balances cash=0 points=0
<공인IP>:56084 crossroad 'DAILY_ACTIVE_USER ...'
<공인IP>:56084 crossroad 'LATEST_GAME_VERSION ...'
```

출발지가 사설 대역이 아니라 공인 IP 였다 — 터널이 아니라 실제 인터넷 경로다.

(출발지 IP 와 계정 id 는 여기에 옮겨 적지 않는다. **이 프로토콜에는 인증
단계가 없어서 계정 id 를 아는 사람은 그 계정으로 붙을 수 있다** —
`hl5_server/README.md` 의 "보안 — 정직하게" 참조. 로그 발췌는 이슈·채팅·스크린샷으로 옮겨 다니고, 그 순간 그 값은 자격증명이
된다. 재현이 필요하면 `journalctl -u hl5-server` 를 직접 봐라.)

**"Wi-Fi 끄고 셀룰러만"은 이 기기로 불가능하다.** S9 에는 SIM 이 없다 —
`mServiceState` 가 `OUT_OF_SERVICE`, `mDataConnectionState=-1`. SIM 이 있는
기기(S25)를 꽂을 때 함께 확인할 것.

## 16. "Connecting 없이 바로 시작" — 조사 (2026-08-18)

요구는 한 문장이다. **설치하고 켜면 곧바로 메인 화면.** 접속 대기도 로그인도
없이. 구현은 다음 세션이지만, 이 요구는 지금까지의 작업과 성질이 달라서 조사를
먼저 했다 — 블리터 이식은 "원본과 같은 바이트"로 증명할 수 있었는데, 이것은
**정의상 원본과 달라지는 첫 작업**이다.

조사는 병렬 에이전트 5개 + 적대적 검증 6건 + 종합 1건으로 돌렸다. 아래는 그중
계획을 바꾼 것들이다.

### 16.1 원본도 똑같이 띄운다 — 그리고 그것은 구조적으로 강제된다

`docs/reference/original-s9-01-connecting.png` 가 갤럭시 S9 에서 **원본 APK** 가
띄우는 화면이고, 우리 것과 같은 "Connecting" 상자다. 우리 버그가 아니다.

왜 피할 수 없는지가 중요하다. `NETWORK::netConnect(char mode)`(`0xCA544`)는
소켓을 열자마자 **서버 도달 여부와 무관하게** `CreatePopup(339)` 로 꼬리분기한다:

```
0xca588  mov  r1, #0x150
         mov  r3, #0
         mov  r0, r4
         add  r1, r1, #3        ; 339
         strb r3, [r4, #0x965]
         strb r3, [r4, #0x964]
         pop  {r4, lr}
         b    #0xc83bc          ; _ZN7NETWORK11CreatePopupEi
```

그 팝업을 없애는 **정상** 경로는 하나뿐이다 — `NETWORK::RecvParsering`(`0xCDEEC`)
이 opcode 로 분기하기 *전에* `DeletePopup()` 을 부른다. 즉 **패킷이 한 개라도
도착해야** 사라진다. 서버가 없으면 남는 것은 `setError` 경유 타임아웃뿐이다.

문자열 자체는 `.so` 안에 없었다. `libHeroesLore5.so` 전체에서 `Connecting` 매치는
4건인데 전부 심볼 이름 `__knl_netmSocketConnectingTimer` 의 부분문자열이다. 실물은
VFS 아카이브의 `/c/csv/ingame_text.dat` 안에 있고, 그 파일의 0-based **339번**
엔트리다. 포맷은 `[u16 count=357]` 뒤에 `[u16 len+1][u8 len][len 바이트]` 레코드
357개이고 4477바이트에 정확히 맞아떨어진다. 340='Send', 341='Recieve',
342='서버상태가 좋지 않습니다…' — 진단 문자열이 나란히 있다.

그리는 것은 `StatePopup` **타입 11**이다(`StateNetMenu` 도 `CommonUi` 도 `GMenu`
도 아니다). `CommonUi::DrawDescriptionBox` 로 틀을 그리고, 폭/16 개의 사각형을
`Graphic::DrawRect` 테두리 + `Graphic::FillRectAlpha(알파=위상*100/N)` 로 그린다.
폭 144~159 면 9칸 — **정답지 캡처의 9칸과 일치한다.**

### 16.2 계획을 뒤집은 사실 — 네트워크를 끄면 부팅이 멈춘다

처음 세운 가설은 "`CletEntry.isNetworkAvailable` 이 무조건 1을 돌려주니 0으로
바꾸면 게스트가 접속을 건너뛴다"였다. **틀렸고, 정반대다.**

`MC_netConnect`(`0x14B8BC`)는 반환값을 이렇게 검사한다:

```
bl   isNetworkAvailable
sub  r0, r0, #1
cmp  r0, #1
mvnhi r0, #0          ; {1,2} 가 아니면 -1
bhi  return
```

그리고 유일한 호출자 `_midas_funcLibConnectTimerCB`(`0x41E94`)는 `cmn r0,#0xa` —
**-10 일 때만** 콜백한다. -1 은 그냥 버린다. 그 타이머는
`MIDASKernelManager::timerLoop` 가 콜백 전에 `MC_knlUnsetTimer` 를 부르므로
one-shot 이다. 결과: `netConnectCB` 가 영영 안 뜨고 → `NETWORK+0x358` 이 0 인 채
남고 → **Connecting 팝업이 사라지지 않는다.** `netOpen`(`0xCA4E0`)이 이미
`NETWORK+0x10` 에 시각을 찍어 뒀으므로 `StatePopup::Process`(`0x13FBD0`)의
60,000 ms 감시가 `setError(300)` 을 낼 때까지 버틴다.

**4프레임짜리 증상이 60초짜리가 된다.** 지금의 `1` 이 유일하게 옳은 값이었다.

### 16.3 더 깊은 사실 — 부팅을 진행시키는 것이 그 팝업이다

`StateMainMenu` 는 `SetState(0)` 으로 시작한다. 상태 번호는 `KeyPressed`
(`0x124558`)의 13항 점프테이블로 확정된다: 0=CheckEvent, 1=UserLogReg, 2=Logo,
3=Title, 4=MainMenu, 5~12=메뉴 항목.

`StateMainMenu::endProcess`(`0x11E878`)가 state 0 에서 17틱 뒤 `netConnect(9)`,
state 1 에서 `netConnect(8)`(CrossRoad)을 쏜다(`this+0x10` 이 상태당 1회 래치).

그런데 **state 0/1 은 접속 시도 말고는 하는 일이 없다.**
`DrawCheckEvent`(`0x121180`)는 화면을 흰색으로 채우는 `FillRect` 하나가 전부다.
그리고 다음 상태로 가는 문은 `StateMainMenu::Process` 의 팝업 결과 핸들러 하나뿐:

```
팝업 0x3a / 0x3b  → SetPopupState(0) + SetState(1)                     (0x1249B4)
팝업 0x38 / 0x39  → SetPopupState(0) + LoadLogoSprite(0) + SetState(2) (0x124E24)
```

`SetState` 호출처를 전수 조사해도 state 2 로 들어가는 다른 길이 없다. 그리고 그
팝업은 네트워크 시도의 결과 — 성공이든 실패든 — 로만 생긴다.

> **따라서 `netConnect` 를 무력화하면 부팅이 빨라지지 않고 흰 화면에서 멈춘다.**

이것이 이 조사의 결론이고, 모르고 시작했으면 하루를 태울 자리였다.

### 16.4 서버가 살아 있어도 Connecting 은 남는다

로컬 즉답 서버를 물려 실측했다. 네트워크 구간이 282 → 172프레임으로 줄지만
**Connecting 은 여전히 두 번**(35-44, 79-86) 뜨고 그 뒤에 120프레임짜리 모달이
하나 더 붙는다(87-206).

이유도 구조적이다. `RecvCrossRoadParsering`(`0xCC5D4`)은 `LATEST_GAME_VERSION`
응답 첫 바이트가 `0x1A`/`0x1B` 면 **무조건** `SetPopupState(0x38)` 을 세우고
`netDisconnect` 하며, 그 밖의 값이면 `setSystemError` 로 또 팝업을 낸다.
**팝업 없이 지나가는 응답이 없다.** 그래서 "서버를 빠르게 만든다"는 이 요구의
답이 아니다.

### 16.5 실측 — 긴 것은 Connecting 이 아니었다

프로브는 가상시계(33 ms/frame)로 돌고 오프라인 부팅에서는 `select` 가 한 번도
불리지 않으므로 `가상시간 = 33 ms × 프레임수` 가 정확히 성립한다. 입력 없는 부팅을
`--film-every 1` 로 전수 스캔한 결과:

| 프레임 | 화면 | 가상시간 |
|---|---|---|
| 1–34 | 흰 화면 (state 0) | 1.1 s |
| **35–38** | **Connecting** | 0.13 s |
| 39–158 | "서버상태가 좋지 않습니다 … (-1)" | **4.0 s** |
| 159–192 | 흰 화면 | 1.1 s |
| **193–196** | **Connecting** | 0.13 s |
| 197–316 | 같은 팝업 | **4.0 s** |
| 317~ | GRB 등급화면 → 로고 → 프롤로그 | 38 s |
| **1482** | **타이틀** | |

**Connecting 자체는 8프레임(0.26초)이다.** 네트워크 구간 전체가 282프레임(9.3초)
이고, 그중 8초가 오류 팝업 두 개다. 그리고 전체 1482프레임(48.9초) 중 나머지
~1,165프레임은 등급화면·로고·프롤로그다.

인트로 구간은 §16.13 에서 따로 판다 — 처음 쓴 문장이 틀렸고, 인트로를 남기기로
하면서 그 길이가 곧 사용자가 겪는 것이 됐기 때문이다.

### 16.6 기기에서 실제로 멈추는 것은 호스트다

`Config::server` 가 설정됐는데 닿지 않으면 `connect_real` 의
`TcpStream::connect_timeout(5초)` 가 **`vm.frame()` 안에서 블로킹**한다. 안드로이드
셸은 `virtual_clock: false` 라 그 지연이 그대로 노출된다 — 프레임과 입력이 통째로
멈춘다.

같은 60프레임 실행의 벽시계:

| | 벽시계 |
|---|---|
| 서버 미설정(오프라인) | 0.836 s |
| `--server 127.0.0.1:7398` (거부) | 2.885 s |
| `--server 10.255.255.1:7300` (블랙홀) | **5.871 s** |

"데이터 없는 폰에서 켠다"가 정확히 세 번째 시나리오다. 즉시 시작과 정면으로
충돌하므로 같이 고쳐야 한다.

### 16.7 잃는 것은 생각보다 좁다

**부팅 로그인은 온라인 기능의 전제조건이 아니다.** `netConnect` 는 모드마다 새
소켓을 열고 `netLibCB` → `setServerConnected` 가 **매번** 1/1 을 다시 보낸다.
`ProcPacket_Login`(`0xCDA1C`)은 `gv+0x30`/`gv+0x38` 이 0 이면 1/5(잔액)를 스스로
다시 요청한다. 캐시샵(모드 3)·창고(모드 2)·익스퍼트(모드 0)는 각자 로그인한다.

그래서 부팅 단축이 실제로 잃는 것은 **이벤트/공지 목록, DAU 집계, 버전 안내,
클라우드 세이브 핸드셰이크** 로 좁혀진다.

메뉴 9개 중 서버가 필요한 것도 셋뿐이다 — 익스퍼트(온라인 랭킹, 카테고리 5),
창고접속(카테고리 2/3/4/8), 업데이트(카테고리 100). 게임시작·환경설정·달성과제·
도움말·게임문의·게임종료는 코드 경로에 `netConnect` 도 `WriteData` 도 없다.

다만 **미접속 상태의 `WriteData` 는 조용히 아무 것도 하지 않는다** — "에러가 안
났다"가 "잃은 게 없다"를 뜻하지 않는다.

### 16.8 그래서 계획은 — 상태를 건너뛴다

`Vm::boot()` 직후 호스트가 **게스트 심볼만 불러서** 상태를 옮긴다. `Vm::symbol`
은 데이터 심볼도 주므로(`elf.rs` 의 `read_symtab` 이 `.symtab`/`.dynsym` 전체를
넣는다) `gv` 를 이름으로 읽을 수 있다.

* **c1** — `LoadLogoSprite(0)` → `SetState(2)`. 팝업 0x38/0x39 핸들러(`0x124E24`)
  가 하는 것과 같은 호출·같은 순서. 프레임 1~316(10.4초)이 소멸한다.
* **c2** — `FreeLogoSprite()`(`0x12195C`, 모든 포인터 null 검사 확인) →
  `LoadLogoSprite(2)` → `SetState(3)`. `KeyLogo`(`0x124448`)의 substate 4 스킵과
  같다 — 플레이어가 인트로 중 한 번 탭하는 것과 동일한 경로. 추가로 ~1,165프레임이
  소멸한다. **채택되지 않았다 — 아래 §16.9.**

`netConnect` 자체는 **건드리지 않는다.** 그러면 온라인 절반은 플레이어가 그 기능을
열 때 그대로 살아난다.

### 16.9 인트로는 남긴다 — 사용자 결정 (2026-08-18)

c2 는 채택되지 않았다. 따라서 **로고·GRB 12세이용가 등급 화면·© 2011 Electronic
Arts·프롤로그는 원본 그대로 재생된다.**

이 결정이 값어치 있는 이유가 하나 더 있다: c2 가 지웠을 **GRB 12세이용가 등급화면**
(substate 0)과 **EA 로고 마크**(substate 3)는 코드 문제가 아니라 **표시 의무** 문제였고,
그것을 사람이 판단할 몫으로 남겨 뒀었다. 인트로를 남기기로 하면서 그 질문이 함께 닫힌다.
(처음에는 "© 2011 Electronic Arts 화면"이라고 썼는데 그 텍스트 줄은 인트로가 아니라
**타이틀 화면**에 있다 — §16.13.)

건너뛰는 수단이 사라지는 것도 아니다 — 인트로 중 탭 한 번이면
`KeyLogo`(`0x124448`)의 substate 4 분기가 타이틀로 보낸다. **원본이 이미 주는
스킵이므로 그 경로는 손대지 않는다.**

### 16.10 인수 기준을 새로 정의해야 한다

지금까지의 기준은 "원본과 바이트 동일"이었고 그것으로 블리터 905프레임을 증명했다.
**부팅을 바꾸면 정의상 프레임이 달라진다.** 대체 기준:

1. **정렬 A/B** — 원래 부팅과 단축 부팅을 `--film-every 1` 로 찍고, 점프한 상태의
   첫 프레임을 기준으로 정렬해 이후 N프레임이 `ab_compare.py` 로 바이트 동일일 것.
   단축이 게스트가 도달했을 상태를 정확히 재현했다면 그 뒤는 같아야 한다.
2. 훅은 **게스트 함수 호출만** 할 것. 게스트 메모리에 상태를 직접 쓰는 순간 위
   증명 수단이 사라진다.
3. `patch.rs` 의 905프레임 4장면 A/B 가 **원래 부팅 모드로** 그대로 통과할 것.
4. 서버를 물린 채 캐시샵에 들어가면 부팅 로그인이 없었어도 서버 로그에
   1/1 → 6/2 → 6/5 가 남을 것.

그리고 **부팅을 바꾸는 코드는 `patch.rs` 에 넣지 않는다.** 그 파일의 계약은 "같은
바이트를 만드는 이식"이고, 동작 변경이 한 줄이라도 섞이면 그 파일의 A/B 가 무엇을
증명하는지 아무도 말할 수 없게 된다.

### 16.11 조사가 곁다리로 잡은 것

* **`docs/SERVER.md` §1.1 이 틀렸다.** 게임 소켓의 실제 목적지는
  `ngw.gxg.com:7300` 이 아니라 리터럴 IP **`210.222.18.28`** 이고 포트는 모드마다
  다르다(게임 18186 / 패치 9096 / CrossRoad 11536). `MX_smartConnect`(`0x38080`)
  가 r0/r1 을 즉시 덮어써서 그 문자열은 syscall 까지 내려오지 않는다. 실측:
  `MC_netSocketConnect(0, 0x1c12ded2, 0xa47, ...)`. 정정했다.
* **`.text` 안에 ARMv6T2 `MOVW` 가 정확히 1개 있고 우리 인터프리터가 무시한다.**
  `NETWORK::CheckServerConnectLimitTime+0x2c`(`movw r6,#0x1388`). `arm32_cpu` 가
  그것을 DataProc2 의 `TST`(S=0)로 디코드해 r6 에 아무것도 쓰지 않는다. 그 함수에서
  r6 은 달리 정의되지 않으므로 5,912 ms 요청 마감이 호출자가 남긴 값에서 나온다.
  실서버 응답 대기 구간에서만 문제가 되므로 지금까지 안 보였다.
* **세이브 파일이 디스크에 없다.** 오프라인으로 게임플레이까지 완주(게스트 명령어
  203억, 폴트 0)한 뒤 게스트는 "저장되었습니다"를 띄웠는데 `--storage` 트리에
  디렉터리 28개 / 파일 0개다. 원인 미상.
* **`read_server` 폴백 함정** — `.or(option_env!("HL5_SERVER"))?` 가 split 보다
  앞에 있어서, 비어 있지 않은 잘못된 `hl5_server.txt` 하나가 빌드타임 기본값으로
  되돌아가지 않고 조용히 앱을 오프라인으로 만든다.
* **`fd_write` 의 WouldBlock → `EIO(5)`.** `MC_netSocketWrite` 는 `11`(EAGAIN)일
  때만 재시도(-19)로 보고 그 외는 -1 하드에러다. 관측된 write 는 128~189바이트라
  아직 안 터졌다.
* **`.so` 에 DWARF 가 남아 있다** (`.debug_info` 0x29041 바이트, `.debug_str`,
  `.debug_line`). 구조체 필드 오프셋·타입을 IDA 휴리스틱이 아니라 정본으로
  복원할 수 있다 — `tools_re/idb_structs.txt` 의 "이름과 순서뿐" 한계를 넘어설 길이다.

### 16.12 이번에도 점프테이블에서 한 번 틀렸다

`setError` 의 net-state 점프테이블을 읽을 때 mode 8/9 ↔ 팝업 0x39/0x3b 를 정확히
뒤집어 읽었다. 원인은 `addls pc, pc, rX, lsl #2` 의 **첫 항목이 `addls` 다음
*다음* 워드**이고 바로 다음 워드는 default 라는 것. `--watch` 실측이 바로잡았다.

교훈은 §14.5 의 죽은 `getKernel()` 과 같다 — 점프테이블을 읽었으면 반드시 실행으로
한 번 확인한다.

### 16.13 인트로 길이 — 처음 쓴 한 문장이 틀렸고, 고치니 다른 것이 나왔다

§16 을 커밋한 직후 사용자가 **"인트로는 건너뛰지마"** 라고 결정했다. 그 순간
§16.5 에 적어 둔 이 문장이 장식이 아니라 판단 근거가 됐다:

> 인트로는 프레임 카운터와 `MC_knlCurrentTime` 임계값 중 **먼저 오는 쪽**으로
> 진행한다 — **fps 를 올려도 짧아지지 않는다.**

두 문장이 서로 모순이다. "먼저 오는 쪽"(OR)이면 틱이 빨라질수록 프레임 카운터가
벽시계 기준으로 먼저 차므로 **짧아져야** 한다. 그래서 다시 팠다.

#### 결과: 결론은 맞았고 이유가 틀렸다

`EndProcLogo`(`0x123634`)는 substate 0..4 의 5항 점프테이블
(`0x123658` `cmp r0,#4; addls pc,pc,r0,lsl#2`)이고, **`MC_knlCurrentTime` 은
substate 4 블록에만 있다.** GRB 등급화면(substate 0, 80틱)과 EA 로고(substate 3,
39틱)에는 시간 검사가 **하나도 없다** — 순수 프레임 카운트다. §16.5 는 이 셋을
"인트로"로 뭉뚱그렸다.

substate 4 의 조건은 확실히 OR 이고, 시간 경로는 진행을 **막지 않고 카운터를 구간
경계로 앞으로 민다**:

```
counter <= 0x4f : delta > 5400  → counter = 0x50    (0x123898)
0x50..0xa9      : delta > 10800 → counter = 0xaa    (0x123904)
0xaa..0x103     : delta > 16200 → counter = 0x104   (0x123928)
0x104..0x15d    : delta > 21600 → counter = 0x15e   (0x12394c)
0x15e..0x1b7    : delta > 27000 → counter = 0x1b8   (0x12388c)
종료: counter > 450 (0x1236cc)
```

즉 논리만 보면 틱이 빨라질수록 짧아져야 하고, **451틱이 하한**이다.

#### 진짜 이유는 게스트가 자기 목을 조른다는 것

`TimerCb+0x288`(`0x9E038`):

```
StateMainMenu::GetState()      ; cmp #2   (Logo)
StateMainMenu::GetSubState()   ; cmp #4   (프롤로그)
mov r2, #0x37                  ; 55 ms
bl  MC_knlSetTimer
```

**프롤로그 substate 만 틱 주기가 55 ms 로 하드코딩돼 있다.** 그 밖의 틱은
`1000/fps표[gv[0x86]] − 경과`(최소 5 ms)이고 실측 요청값은 45 ms 다. 표 두 벌
(`0x158688` = 주기 {90,80,…,5}, `0x1586b0` = 목표 fps {10,11,…,22})이 말하듯
**게스트는 어떤 설정에서도 22 fps 를 넘겨 그리지 않는다.**

5,400 ms ÷ 90틱 = 60 ms 이고 게스트 자신의 틱이 55 ms 다. **두 메커니즘이 설계상
거의 정확히 겹치도록 맞춰져 있다** — 그래서 시간 스냅은 느린 호스트에서만 개입하고,
빠른 호스트에서는 55 ms 틱이 바닥을 만든다.

#### 그리고 첫 조사도 숫자 하나를 틀렸다

디스어셈블 조사는 하한을 `451 × 55 = 24.8초`로 계산했다. **틀렸다.**
`timerLoop`(`0x152318`)에 **10 ms 조기 발화 슬랙**이 있다 — `mov r6,#0xa`,
`adds r0,r0,r6`, 그리고 `now + 10 > due` 비교. 실효 최소 경과가 D−9 이므로
프롤로그 틱의 하한은 55 가 아니라 **46 ms**, 일반 틱은 45 가 아니라 36 ms 다.

따라서 프롤로그 하한은 `451 × 46 = 20.75초`다. (그 조사의 표는 자기 주장과도
어긋났다 — 하한을 24.8초라 해 놓고 60 fps 칸에 27.7초를 적었다.)

**교훈은 §14.5·§16.11 과 같다.** 디스어셈블만으로 시간 상수를 계산하면 틀린다.
타이머 계층이 그 상수를 어떻게 쓰는지까지 읽어야 하고, 그 다음엔 실행으로 확인해야
한다. 이번에는 `--watch MC_knlSetTimer` 로 요청 인자를 세는 것이 확인이었다
(프롤로그 구간에서 `0x37` 이 정확히 421회).

#### 실측 — fps 와 무관한 불변량

`--boot --frames 1600 --watch _ZN13StateMainMenu11EndProcLogoEv --watch ...SetStateEa
--watch ...SetSubStateEa --watch MC_knlSetTimer` 한 번으로 전 구간이 나온다:

| 구간 | 틱 | 요청 틱 주기 | 호스트 CPU/틱 |
|---|---|---|---|
| 네트워크 (state 0+1) | 79 + 79 | 45 ms | 5.9 ms |
| GRB 등급화면 (substate 0) | **80** | 45 ms | 4.8 ms |
| EA 로고 (substate 3) | **39** | 45 ms | 4.8 ms |
| 프롤로그 (substate 4) | **421** | **55 ms (0x37, 정확히 421회)** | **3.3 ms** |
| 타이틀 연출 | ~42 | 45 ms | **25.2 ms** |

**프롤로그 틱의 호스트 비용은 타이틀 틱의 1/7.6 이다.** 인트로는 게스트 바운드가
아니다 — 프레임 주기를 정하는 것은 `present()`(매 프레임 2220×1080 fill + 스케일
복사)다. 남은 블리터 이식 넷은 메뉴/타이틀을 겨눈 것이고 **인트로에는 거의 닿지
않는다.**

#### 기기 환산 — 여기부터는 측정이 아니다

**인트로 구간의 기기 fps 는 한 번도 측정된 적이 없다.** 아래는 S9 타이틀
30~37 fps(CLAUDE.md, 이전 세션 실측)에서 나온 환산이다.

| 구간 | 틱 | 환산 |
|---|---|---|
| 네트워크 | 158 | 8.5~10.4 s |
| GRB + EA 로고 | 119 | 6.4~7.9 s |
| 프롤로그 | 421~451 | 24.4~28.5 s |
| 타이틀 연출 | ~42 | 2.3~2.8 s |
| 로더 (측정) | — | 0.87 s |
| **합계** | | **≈ 45~50 초** |

프로브 가상시계 기준값 48.9초(§16.5)와 같은 자리다.

**무입력이면 45~50초. 프롤로그 진입 직후(15~18초 시점)에 한 번 탭하면 18~21초.**
계획한 네트워크 두 단계 생략이 여기서 8.5~10.4초를 뺀다.

fps 로 살 수 있는 것은 프롤로그에서 **5~7초**, GRB+EA 에서 **약 2초**뿐이다.
**인트로를 남기는 한 부팅~타이틀은 약 32초 밑으로 내려가지 않는다** — 우리 성능이
아니라 게스트 자신의 페이싱이다.

#### 그리고 이것이 나왔다 — 처음 8초는 탭이 안 먹는다

`KeyLogo`(`0x124448`)는 **substate 1 과 4 만** 처리한다. GRB 등급화면(80틱)과
EA 로고(39틱) 동안 탭·키가 **완전히 무시된다.** 실측: 프레임 400(GRB)·480(EA로고)
에서 탭 → `KeyLogo` 는 불리지만 `SetState` 없음. 프레임 560(프롤로그) → 즉시
`SetState(3)`.

기기 환산으로 **처음 15~18초 중 뒤쪽 6~8초가 무반응**이다. 사용자는 이것을
"터치가 죽었다"로 읽는다. 원본도 같지만, "설치하고 켜면 바로 메인 화면"이라는
요구와 정면으로 부딪히므로 **인트로를 남기더라도 이 구간을 탭으로 넘길 수 있게
할지는 따로 결정할 몫**이다.

`state 3`(타이틀)으로 가는 문은 정확히 둘뿐이고 네 줄이 똑같다 —
`EndProcLogo+0x28c`(`0x1238C0`, counter > 450)와 `KeyLogo+0x5c`(`0x1244A4`, 탭):
`FreeLogoSprite → LoadLogoSprite(2) → SetState(3) → PlaySound(0x14,1,1,0)`.

#### 곁다리 정정 둘

* **"© 2011 Electronic Arts" 텍스트 줄은 인트로에 없다.** 그것은 타이틀 화면
  (`state 3`, `DrawTitle`)에 있다. 인트로의 EA 요소는 substate 3 의 로고
  스프라이트(`/c/sp/imgcom/ealogo.mgr`)뿐이다. §16.9 를 그에 맞게 고쳤다.
* **substate 1 과 2 는 도달 불가능한 죽은 코드다.** substate 1(네트워크 사용 동의
  예/아니오)로 분기하는 명령이 없다 — `0x123834` 의 워드가 무조건 분기이고 바로
  앞의 `ldrsb r3,[gv+0x2a]; cmp r3,#0` 은 죽은 코드다. `SetSubState(2)` 를 부르는
  곳도 없다. 실행 실측에서도 두 substate 진입 0회.

## 17. 즉시 시작 — 구현 (2026-08-18)

§16 의 계획을 코드로 옮긴 세션이다. 한 가지를 먼저 말해야 한다: **이 세션은
payload/ 없는 원격 클론에서 진행돼 게스트 실행 검증을 하나도 하지 못했다.**
컴파일과 인터프리터 단위 테스트만 통과했고, §16.10 의 인수 기준 네 가지는
전부 payload 가 있는 기계의 몫으로 남아 있다 (§17.5).

### 17.1 BootMode — 상태 점프의 실물

`hl5_core::BootMode { Original, SkipNetwork }` 가 `Config` 에 들어갔다.
`Config::default()` 는 **Original** — 재생 스크립트와 정답지 A/B 가 전부 그
경로를 전제하기 때문이고, 안드로이드 셸의 기본값만 **SkipNetwork** 다
("설치하고 켜면 바로 메인 화면"이 이 앱이 존재하는 요구라서).

점프는 `Vm::try_skip_network()` (lib.rs, `boot()` 옆 — §16.10 의 규칙대로
`patch.rs` 에는 한 줄도 없다):

* `gv+0x27c` 에서 `StateMainMenu` 인스턴스 포인터를 **읽고** (호스트가 게스트
  메모리에 하는 유일한 일 — 쓰기가 아니라 읽기), `GetState()` 가 0/1 일 때만
* `LoadLogoSprite(0)` → `SetState(2)` 를 **게스트 호출로** 실행한다. 팝업
  0x38/0x39 결과 핸들러(`0x124E24`)와 같은 호출·같은 순서 — 단 핸들러의 첫
  호출 `SetPopupState(0)` 은 일부러 뺐다: 그것은 네트워크 시도가 만든 팝업을
  지우는 호출인데, 이 점프는 프레임이 하나도 돌기 전이라 지울 팝업이 존재한
  적이 없다. state 0/1 은 한 틱도 돌지 않으므로 `endProcess` 가 `netConnect`
  에 도달하지 않는다.
* `SetState` 는 단일 인자 썽크 `_ZN13StateMainMenu8SetStateEa` (8바이트,
  `SetStateEaa` 로 전달)를 부른다 — §16.13 의 실측 watch 가 잡은 것과 같은
  진입점이다. 핸들러가 혹시 `Eaa` 를 직접 부른다면 정렬 A/B 가 그 차이를
  잡아낼 것이다 (§17.5-②가 이 가정의 검증이기도 하다).
* 심볼 넷(`gv`, `GetState`, `LoadLogoSprite`, `SetState`) 중 하나라도 없으면
  **에러로 죽는다** — patch.rs 와 같은 계약: 다른 바이너리는 조용히 다르게
  돌지 않고 시끄럽게 멈춘다.
* `boot()` 직후 `StateMainMenu` 가 아직 없으면(포인터 0) 프레임마다 재시도하고
  300프레임이 지나면 로그를 남기고 포기한다 — 원본 부팅이 그대로 도는
  안전한 실패. 단 §16.3 대로 생성자가 `SetState(0)` 으로 시작하므로
  `nativeStartApp` 안에서 만들어져 있을 것이 예상값이다. 어느 쪽인지는
  로그 한 줄("skip-network: StateMainMenu 0 -> 2")로 판정된다.

노출은 세 겹: 프로브 `--boot-mode skip-network`, 빌드 `-Boot <mode>`
(`HL5_BOOT` 로 전달), 기기 런타임 `<files>/hl5_boot.txt` (디버그 빌드에서
adb 로 쓸 수 있어 기기 위 A/B 에 필요하다).

### 17.2 논블로킹 다이얼 — §16.6 의 프리즈 제거

`connect` 의 논블로킹 경로가 워커 스레드에서 다이얼한다. 게스트에게는 원래
말하던 EINPROGRESS-then-select 프로토콜이 그대로 돌아가고, `vm.frame()` 안에서
`TcpStream::connect_timeout(5초)` 가 블로킹하던 것(§16.6 의 5.9초 프리즈)이
사라진다. 완료는 `poll_dials()` 가 승격한다 — select 대기 루프 안에서도 돌므로
게스트가 select 에 앉아 있는 동안 도착한 연결도 그 select 가 본다. 실패한
다이얼은 죽은 서버 세계(준비 안 됨 + `SO_ERROR`=ETIMEDOUT)로 무너진다 —
게스트가 S9 정답지에서 처리한다고 증명된 바로 그 모양이라서, 성공/실패 어느
쪽도 새 경로를 만들지 않는다. 블로킹 connect 는 동기 그대로다(그 의미가
"기다려 달라"이고, 게임 소켓은 그 경로를 쓰지 않는다).

곁다리 둘도 같이 고쳤다 (§16.11 에서 잡아 둔 것):

* **`read_server` 폴백 함정** — 파일을 *파싱해 본 뒤에* 채택한다. 형식이
  틀린 비어 있지 않은 `hl5_server.txt` 는 이제 경고를 찍고 빌드타임 기본값으로
  넘어간다.
* **`fd_write` 의 WouldBlock** — `errno_of` 에 `WouldBlock→EAGAIN(11)` 을
  넣었다 (`MC_netSocketWrite` 가 재시도(-19)로 보는 유일한 값). TimedOut·
  ConnectionRefused 도 정확한 번호를 받는다.

### 17.3 MOVW/MOVT — 인터프리터의 마지막 알려진 구멍

`vendor/arm32_cpu` 의 DataProc 디코드에서 즉치형 S=0 공간의 opcode 0x8/0xA 를
MOVW/MOVT 로 실행한다 (TST/TEQ/CMP/CMN 은 S=1 만 유효하고, MSR 0x9/0xB 는
매치 순서상 PsrImm 이 먼저 가져가므로 여기 도달하는 것은 정확히 MOVW/MOVT 뿐).
`NETWORK::CheckServerConnectLimitTime+0x2c` 의 `movw r6,#0x1388` 이 이제 r6 에
5,000 을 쓴다 — 실서버 응답 대기의 요청 마감이 호출자가 남긴 쓰레기가 아니라
설계값에서 나온다. 단위 테스트(`movw_movt`)가 두 명령의 실행과 플래그 불변을
고정하고, 기존 32개 테스트(디코더 LUT 전수 일치 포함)는 그대로 통과한다.

### 17.4 안드로이드 기본값이 skip-network 인 이유

이 결정만 코드가 아니라 제품이다. 프로브·Config 기본값을 Original 로 둔 것과
방향이 반대인데, 두 세계의 소비자가 다르기 때문이다: 프로브의 소비자는 재현
스크립트와 A/B 비교(원본 경로 전제)이고, APK 의 소비자는 "켜면 바로"를 요구한
사용자다. 검증 때 기기에서 원본 부팅이 필요하면 `hl5_boot.txt` 에 `original`
한 줄이면 된다.

### 17.5 이 세션이 증명한 것과 못 한 것

증명한 것: 컴파일(hl5_core/probe/android 3종), `arm32_cpu` 테스트 33개(신규
movw_movt 포함), 그리고 코드가 §16 의 결정 사항(게스트 호출만, patch.rs 격리,
Original 보존, 인트로 불변)을 지킨다는 것.

증명하지 못한 것 — **payload 가 있는 기계에서 해야 할 순서**:

1. `--boot-mode skip-network --boot --frames 40 --film out/skip --film-every 1`
   → 첫 프레임부터 GRB 등급화면(state 2)인지, 로그에
   "skip-network: StateMainMenu 0 -> 2" 가 있는지.
2. §16.10 의 정렬 A/B — Original 부팅 프레임 317~ 과 skip 부팅 프레임 1~ 을
   정렬해 `ab_compare.py` 바이트 동일.
3. `patch.rs` 905프레임 4장면 A/B 가 Original 모드로 그대로 통과.
4. 서버 물린 skip 부팅에서 캐시샵 진입 → 서버 로그 1/1 → 6/2 → 6/5.
5. MOVW: `--watch _ZN7NETWORK27CheckServerConnectLimitTimeEv` 로 실서버 응답
   대기 마감이 5,912 ms 근처로 잡히는지.

## 18. S25 Ultra 실기기 브링업 — 그리고 버그 둘 (2026-08-18)

§17 이 남긴 검증 넷을 전부 실행하고, 프로젝트가 존재하는 이유인 기기에
처음으로 올린 세션이다. 그 과정에서 **조용히 망가져 있던 것 둘**이 나왔다.

### 18.1 §17.5 인수 기준 — 넷 다 통과

| | 기준 | 결과 |
|---|---|---|
| ① | skip 부팅 첫 프레임이 GRB 등급화면 | **통과.** `skip-network: StateMainMenu 0 -> 2 (Logo); states 0/1 never ran`. 프레임 1은 양쪽 공통의 그리기 전 빈 화면이고, 프레임 2에서 Original 은 흰 화면(state 0), skip 은 GRB 로 갈린다. `boot()` 직후 인스턴스가 이미 있어 재시도는 0회 — §17.1 의 예상값 그대로. |
| ② | 정렬 A/B 바이트 동일 | **통과, 예상보다 강하게.** 실측 오프셋은 **316** (Original 318 ↔ skip 2). 정렬 후 **1,283 프레임 전부 바이트 동일** (`ab_compare.py` exit 0). 몇 프레임이 아니라 GRB→EA 로고→프롤로그→타이틀 **인트로 전체**다. |
| ③ | 905프레임 4장면 A/B | **통과.** newgame 119 + ingame 543 + shop 134 + online 109 = **905**, 전부 동일. 문서의 905 와 정확히 같은 수 (`--film-every` 기본값이 10이라서). 단 재실행에서 함정 하나 — §18.4. |
| ④ | skip 부팅 + 서버 → 캐시샵 | **통과.** 부팅 로그인이 **없는데도** 캐시샵이 스스로 `1/1 → 1/5 → 6/2(승인) → 6/5(결제) → 6/7` 을 왕복한다. §16.9 의 "각자 로그인한다"가 와이어로 확인됐다. |

`21-menu.png` 이 사실 타이틀이라 잠깐 "재생이 안 도는가" 싶었지만, 그 다음
덤프(`22.png`)가 메뉴다 — **덤프 이름이 한 칸 밀린 것뿐**이고 재생은 클래스선택·
던전·HUD·캐시샵까지 전부 정상으로 간다 (`out/shotsheet.png` 50장 대조).

### 18.2 세이브가 없던 이유 — 2 GB 가 음수였다

가장 값어치 있는 발견이다. 증상은 "게스트가 저장되었습니다 를 띄우는데
디렉터리 34개 / 파일 0개". 8개 스토리지 트리에서 **100% 재현**됐다.

체인은 이렇다:

```
StaticUtil::SaveEncryptFile("SL_0.sav", ...)   ← 게스트는 정상 호출
  → MX_sdSaveData(...)                          ← 이름·크기 다 정상
      → MC_fsOpen(<컨테이너>, 4, 1)             ← 여기서 -13 으로 죽는다
```

`MC_fsOpen` (`0x14d718`) 은 **open() 에 닿기도 전에** 되돌아온다:

```
bl   MIDASKernelManager::getAvailableStorageSpace   ; -> AndroidService::getAvailableInternalMemorySize
cmp  r0, #0xa
mvnle r0, #0xc          ; -13
ble  <return>           ; open() 은 실행되지 않는다
```

`getAvailableInternalMemorySize` 는 `CletEntry.getAvailableInternalMemorySize()J`
를 `CallStaticLongMethod` 로 부른다. 호스트가 돌려주던 값이
**`Long(2 * 1024 * 1024 * 1024)`** 였다.

> 게스트는 그 64비트 값의 **하위 워드만 읽고 부호 있는 비교를 한다.**
> 2 GB 의 하위 워드는 `0x80000000` = **-2,147,483,648**. `cmp r0,#0xa` 뒤의
> `ble` 가 항상 잡혀 **모든 쓰기 open 이 -13** 이 됐다.

2 GB 는 부호 비트를 넘기는 **가장 작은** 값이다. 1 GB(`0x40000000`)로 내리면
그대로 낫는다. `MC_fsOpen` 위의 어느 층도 결과를 보지 않기 때문에 게스트는
멀쩡히 "저장되었습니다" 를 띄웠다 — 그래서 원인 미상으로 남아 있었다.

고친 뒤 (`cletentry.rs`):

* 호스트 — `kickass` 15,396 B + `LOCAL.sav` 63 B 생성. 다시 켜면 슬롯 1이
  **워리어 / PlayTime 00:01:26** 으로 살아난다. 왕복 확인.
* S25 — `pm clear` 로 비운 뒤 플레이. 옵션 저장 단계에서 2,340 B,
  인게임 `save` 를 누르자 **15,396 B** 로 자란다(호스트와 같은 크기).
* **렌더링은 한 픽셀도 안 바뀐다** — newgame/shop/online 전 장면이 고치기 전과
  바이트 동일. 파일이 열리게 됐을 뿐이다.

세이브는 파일 하나가 아니라 **MIDAS 컨테이너 `kickass` 한 개**다. 앞에
`[이름][크기]` 엔트리 표가 있고 `M.sav / Heroslore5 / ET.sav / EX.sav / OP.sav /
SL_0.sav / H_0.sav / B_0.sav` 가 그 안에 들어간다.

### 18.3 §17.2 의 논블로킹 다이얼이 네트워크를 전부 죽이고 있었다

§17.2 는 게스트 실행 검증이 0회였고, **그 세션이 통신을 깨뜨렸다.**
`online.txt` 를 물리면 서버 로그가 이렇게만 남았다:

```
-- connected ('127.0.0.1', 53211)
-- closed    ('127.0.0.1', 53211)      ← 한 바이트도 오가지 않는다
```

`connect_real` 은 멀쩡했다(`connect_timeout` 으로 블로킹 다이얼 후
`set_nonblocking`). 범인은 `getsockopt` 이었다:

```rust
let connected = matches!(..., Some(FdObj::Socket { stream: Some(_), .. }));
let answer = if level == 1 && opt == 4 && !connected { ETIMEDOUT } else { 0 };
```

**다이얼이 아직 진행 중인 소켓**(`stream: None`, `dial: Some`)은 이 식에서
"연결 안 됨"이 되어 `SO_ERROR = ETIMEDOUT` 을 돌려준다 — 죽은 서버 세상과
**구분이 안 된다.** 게스트는 `connect` 다음 프레임에 그걸 읽고 연결이 끝났다고
판정해 소켓을 닫는다. 동기 다이얼이던 시절에는 `connect` 가 돌아온 시점에
`stream` 이 언제나 `Some` 이라 생길 수 없던 창이다.

진행 중인 다이얼은 **오류가 아니다**. `!connected && !dialing` 일 때만
ETIMEDOUT 을 돌려주도록 고쳤고, 문서에 적혀 있던 흐름이 그대로 돌아왔다:

```
>> category=1 (Login) subcmd=1     << len=32
>> category=1 (Login) subcmd=5     << len=44
>> category=9 (Event) subcmd=1     << len=37
>> text handshake 'DAILY_ACTIVE_USER  G1000157 SHW-M250S AN FU'   << 19
>> text handshake 'LATEST_GAME_VERSION  G1000157 AN FU 108'       << 1a00
```

**교훈**: 서버 로그가 비어 있는 것을 "트래픽 없음"으로 읽지 마라. 이번에는
`testserver.py` 를 `python` 으로 띄워 stdout 이 버퍼링되는 바람에 로그 파일이
0바이트였고, 그것 때문에 한동안 깨진 줄도 몰랐다. **`python -u` 로 띄운다.**

### 18.4 A/B 의 함정 — 흔들리는 쪽은 `--no-patch` 다

세이브를 고친 뒤 ingame A/B 가 543 중 **14 프레임 차이**로 떨어졌다. 차이는
블리터가 아니라 캐시샵 대화상자였다:

* `--no-patch` → "엘릭서(20)을(를) 구매하였습니다"
* patched → "서버상태가 좋지 않습니다 … (-1)"

프로브는 **가상 시계**를 쓰는데 서버는 **실시간**이다. patched 는 벽시계로
2.9배 빠르므로 가상 1프레임당 실제 서버에 주어지는 시간이 그만큼 짧아진다.
교차 대조로 범인이 갈렸다:

| 비교 | 결과 |
|---|---|
| patched vs patched (다른 실행·다른 서버) | 543 동일 |
| patched 고치기 전 vs 후 | 543 동일 |
| **no-patch 고치기 전 vs 후** | **529 동일 / 14 차이** |
| **서버 없이(오프라인) no-patch vs patched** | **543 동일** |

즉 **patched 쪽은 언제나 같은 543 프레임을 낸다. 흔들리는 것은 기준선인
`--no-patch` 쪽이다.** 실서버를 물린 A/B 는 이식 정확도가 아니라 경주를
측정한다. **블리터 A/B 는 오프라인으로 돌려라** — 그때 543 프레임이 동일하다.

### 18.5 S25 Ultra 실측 — 처음 올렸다

`SM-S938N` / 안드로이드 16 (SDK 36) / `abilist32` 비어 있음.
`-Fast -Install` (릴리스 Rust + 디버그 서명, 그래서 `hl5_boot.txt` 를 adb 로 쓸 수 있다).

```
primaryCpuAbi=arm64-v8a   minSdk=26  targetSdk=35   DEBUGGABLE
boot ok in 36,711,678 guest instructions;  surface 2340x1080
배율 4.5 / margin_x 270 / margin_y 0        (CletEntry.adjustAutoScreen 그대로)
```

**부팅→타이틀 28.3초** (무입력, 실측):

| 시각 | 화면 |
|---|---|
| 0.13 s | 스플래시 |
| 1.4 – 4.4 s | GRB 12세이용가 (80틱) |
| 4.7 – 6.2 s | EA 로고 (39틱) |
| 6.2 – 27.1 s | 프롤로그 (421틱) |
| **28.3 s** | **타이틀** |

§16 의 모델이 그대로 맞았다. **흰 화면도 Connecting 도 한 번도 나오지 않는다**
— skip-network 가 기기에서도 듣는다는 뜻이다. 그리고 §16 의 "fps 는 지렛대가
아니다"가 실증됐다: S9 보다 5배 빠른 기기인데도 28.3초다. 게스트 타이머가
바닥이기 때문이고, S25 는 이제 그 바닥에 붙어 있다.

**탭 처리도 §16 대로다** (실측):

| 탭 시점 | 구간 | 결과 |
|---|---|---|
| 2.35 s | GRB | **무시** |
| 5.81 s | EA 로고 | **무시** |
| 9.78 s | 프롤로그 | **먹는다** — 11.6초에 타이틀 (무입력이면 28.3초) |

즉 프롤로그에서 한 번 탭하면 타이틀까지 **11.6초**다.

**성능** — 메인 메뉴에서 **44~45 fps / 19.0 M guest ins/s** (10초 창 평균,
guest 6.4 s + present 1.6 s per 10 s). S9 의 메뉴 8~10 fps 와 견주면 4.7배다.
호스트가 게스트 자신의 상한(22 fps)의 두 배로 프레젠트하고 프레임마다
8 ms 여유가 남으므로, **이 기기에서 메뉴는 더 이상 병목이 아니다.**
참고로 프로브 프레임당 게스트 명령어는 메뉴 418 K < 타이틀 656 K — **메뉴가
타이틀보다 가볍다**(CLAUDE.md 의 "메뉴 ~150만"은 이 빌드에 더 이상 맞지 않는다).

**기기에서 확인된 것**: 메인 메뉴 · 슬롯선택 · 클래스선택(워리어/로그/건슬링어/
나이트) · 컷신 · 던전 · 대사 · 퀘스트 팝업 · 게임플레이 HUD · 전투(피격/데미지
숫자/사망) · 부활 결제 프롬프트 · **세이브 왕복** · 한글 전부 정상.

**소리**: `dumpsys audio` 에서 우리 pid 의 `MediaPlayer` 가
`state:started / USAGE_GAME / CONTENT_TYPE_MUSIC / 22050 Hz` — **BGM 은 확실히
난다.** 효과음은 `SoundPool` 이 만들어져 있으나 전투 중 14회 폴링에서 계속
`state:idle` 이었고 `Hl5Audio` 예외도 없다. 다만 **호스트에서는 같은 재생에서
클립이 10개 재생된다**(인트로/메뉴만 돌린 구간은 1개 = BGM). 그러니 "게스트가
효과음을 안 부른다"는 더 이상 맞지 않는다 — 부르는데 기기에서 들리는지가
아직 미확인이다. 여기가 다음 세션의 출발점이다.

### 18.6 곁다리로 확인된 것

* **공개망 서버가 열려 있다.** `<hero5-server>` TCP 연결 성공 —
  CLAUDE.md 가 "남은 한 가지"로 적어 둔 클라우드 방화벽 작업은 이미 끝났다.
* `payload/icon-{ldpi,mdpi,hdpi}.png` 는 저장소에 없다(라이선스 데이터).
  원본 APK 의 `res/drawable-*-v4/icon.png` 에서 뽑는다 — 없으면 빌드가
  거기서 멈춘다.
* `--selftest` 29개, `arm32_cpu` 33개, import 76/76 — 두 수정 뒤에도 전부 통과.

## 19. 기기 검증 마무리 — 공개망 로그인과 "안 들리던" 효과음 (2026-08-19)

§18 이 남긴 셋(APK 설치·공개망·효과음)을 S25 에서 전부 끝냈다.

### 19.1 공개망 서버 로그인 — 휴대폰 자신의 IP 로 붙는다

`<files>/hl5_server.txt` 에 `<hero5-server>` 를 넣고(`-Fast` 는 디버그
서명이라 `run-as` 로 쓸 수 있다), 판정을 위해 `hl5_boot.txt` 를 잠시 `original`
로 돌려 부팅 로그인을 살렸다. 서버(`journalctl -u hl5-server`):

```
<폰 공인IP>:54314 connected
<폰 공인IP>:54314 login ok account=<16자리> model=SHW-M250S version=HERO5.ALL.06.000
<폰 공인IP>:54314 balances cash=0 points=0
<폰 공인IP>:54322 crossroad 'DAILY_ACTIVE_USER <16자리> ... G1000157 SHW-M250S AN FU'
<폰 공인IP>:54322 crossroad 'LATEST_GAME_VERSION ... G1000157 AN FU 108'
```

출발지 `<폰 공인IP>` 은 **개발 호스트의 공인 IP(<개발호스트 공인IP>)와 다르다**
— 개발 기계를 우회하지 않은 진짜 공개망 경로다. `account` 는 기기의
`hl5_account.txt` 와 정확히 일치한다. 이것으로 §18.3 의 네트워크 수정이
프로브뿐 아니라 **arm64 기기 빌드에서도** 동작함이 확인됐다.

`run-as` 안에서 리다이렉트를 쓰려면 셸 한 겹을 더 씌워야 한다. 그냥
`run-as pkg sh -c 'echo x > f'` 는 바깥 셸이 따옴표를 먹어 실패한다:

```powershell
adb shell "run-as kr.heroeslore.five sh -c 'cat /data/local/tmp/f > files/files/f'"
```

### 19.2 효과음은 계속 울리고 있었다 — 20 dB 작게

§18.5 는 "`SoundPool` 이 전투 내내 `state:idle`" 을 근거로 효과음이 안 난다고
적었다. **그 근거가 틀렸다.** `dumpsys audio` 의 `state:idle` 은 짧은 클립을
폴링이 놓친 것일 뿐이다. `Hl5Audio` 에 로그를 넣자 곧바로 보였다:

```
audio: play sound key=26411768 id=2 vol=0.1 -> stream=1
audio: play sound key=26411768 id=2 vol=0.1 -> stream=2
```

`stream != 0` — **SoundPool 은 계속 정상으로 재생하고 있었다.** 문제는 `vol=0.1`.

게스트가 보내는 값은 `master=100 sound=10 music=100`, 나중에 `music=10`.
호스트는 그것을 `10/100 = 0.1` 로 읽었다. **원본은 그렇게 읽지 않는다.**
`classes.dex` 의 `MidasSoundManager` / `MidasMusicManager` 는 둘 다:

```java
volumeRateWIPI = getStreamMaxVolume(STREAM_MUSIC) / 100f;   // 15/100 = 0.15
soundVolume    = min(wipi * volumeRateWIPI, streamMax);     // 10*0.15 = 1.5
soundPool.play(id, soundVolume, soundVolume, 1, loop, 1f);  // 범위는 0..1 !
mediaPlayer.setVolume(musicVolume, musicVolume);            // 역시 0..1 !
```

즉 원본은 **스트림 단위(0..streamMax) 값을 0..1 짜리 API에 그대로 넘긴다.**
15단계 기기에서는 WIPI 값이 약 7 이상이면 전부 1.0 으로 포화한다 — 게임이
쓰는 10 은 1.5 → **full volume**. 우리가 100으로 나눈 것은 필드 이름이
유도하는 "올바른" 해석이지만 **원본 동작이 아니고**, 그 차이가 약 20 dB 다.

`Hl5Audio.wipiGain()` 이 이제 원본 산술을 그대로 재현한다(효과음·음악 공통).
`init` 이 `Context` 를 받아 실제 `getStreamMaxVolume` 을 읽는다(S25 = **15**).
확인:

| | 고치기 전 | 고친 뒤 |
|---|---|---|
| 효과음 | `vol=0.1 -> stream=1` | `vol=1.0 -> stream=1` |
| 음악 | `AudioTrack: setVolume(0.100000)` | `AudioTrack: setVolume(1.000000)` |

**마스터 볼륨만은 일부러 원본과 다르게 둔다.** 원본의 `CletEntry.setVolume` 은
`AudioManager.setStreamVolume` 로 **기기의 시스템 미디어 볼륨 자체를 바꾼다**.
재호스트가 사용자의 폰 볼륨을 건드릴 이유가 없어 로컬 배수로 남겼다.

**교훈 둘.** ① `dumpsys audio` 의 `state:idle` 은 "재생 안 됨"의 증거가 아니다
— 짧은 효과음은 폴링 사이에 끝난다. 경계면에 로그를 넣는 쪽이 빠르다.
② 볼륨처럼 "당연한" 단위는 **원본 dex 를 읽어서** 정한다. 이 버그는 코드만
보면 옳아 보이고, 정답지는 `classes.dex` 안에만 있었다.

### 19.3 남은 잡동사니

* `SoundPool.play` 가 0을 돌려주는 것은 실패다. 이제 그때만 `Log.w` 한다 —
  조용한 효과음을 두 번 찾고 싶지 않다.
* 살아 있는 프로세스에 `am start` 를 여러 번 때리면 풀이 이상해져 한동안
  `stream=0` 이 나온다. 오디오를 잴 때는 `force-stop` 후 새 프로세스로 시작할 것.
* 이 세션의 변경은 `hl5_android`(Rust `sound.rs` 시그니처)와 `Hl5Audio.java`
  뿐이다. `hl5_core` 는 손대지 않았고 `--selftest` 29개 · `arm32_cpu` 33개 통과.

## 20. 81%를 먹던 함수 — `internalGrpCopyFrameBuffer` (2026-08-19)

블리터 이식 후보 목록이 **틀려 있었다.** CLAUDE.md 는
`DrawImageMirrorClip565` / `FillRectBlending` / 폰트 스택 / 4비트 경로를
남은 후보로 적어 뒀는데, 그것은 **newgame 재생으로 잰 프로파일**이었다.
newgame 은 클래스선택까지만 가서 그리는 양이 적다.

**ingame 재생으로 다시 재자 답이 하나였다:**

```
81.1%  16498091  internalGrpCopyFrameBuffer      <- 목록에 없던 함수
 4.8%    985051  DrawImageBlendingClip565
 2.4%    484753  _midas_funcFntDrawString
 2.2%    451457  DrawImageStretchClip565
 2.0%    408118  DrawImageMirrorClip565
```

기존 후보 넷은 전부 2.4% 이하다. **`--profile` 은 ingame 으로 돌려야 한다.**

### 20.1 비용은 복사가 아니라 픽셀당 서론이다

`_Z26internalGrpCopyFrameBufferiiiiiiiiP14_MC_GrpContexth` (`0x148430`,
912바이트, 인자 10개)는 `MC_grpCopyFrameBuffer` 아래의 표면간 블릿이다.
`--profile-in` 이 함수 *안의* 어디가 뜨거운지 말해 준다:

| 오프셋 | 비중 | 무엇 |
|---|---|---|
| +0x0c0 ~ +0x1c0 | **58%** | 픽셀당 경계·클립 검사 열두 개 |
| +0x280 ~ +0x390 | **40%** | 16비트 복사 본체 (`0x1486ec`) |

내부 루프(`0x148528..0x1485f4`)는 **halfword 하나를 옮기기 전에 열두 번
load+compare** 를 한다 — 픽셀당 약 25개 인터프리트 명령어, 전체 화면에,
매 프레임. 그래서 81% 다.

검사 열두 개(전부 전사):

```
src_h <= src_y            건너뜀      (0x148528)
src_w <= src_x + x        건너뜀      (0x148534)
dst_h <= dst_y            건너뜀      (0x148548)
dst_w <= dst_x + x        건너뜀      (0x148558)
src_y < 0 / src_x+x < 0 / dst_y < 0 / dst_x+x < 0   건너뜀  (0x14856c..0x148588)
m_clip.x0 >  dst_x + x    건너뜀      (0x148594)
m_clip.y0 >  dst_y        건너뜀      (0x1485a0)
m_clip.x1 <= dst_x + x    건너뜀      (0x1485ac)
m_clip.y1 <= dst_y        건너뜀      (0x1485b8)
그 다음 dst 의 bpp 로 분기 (0x10 -> 0x1486ec, 0x20 -> 0x148620)
```

**전부 루프 불변이거나 "x 가 어떤 반열린 구간에 있는가"로 환원된다.** 그래서
이식본은 행마다 한 번 계산하고 살아남은 구간을 그대로 복사한다. 산술은
바꾸지 않았다 — 아래 버그까지 포함해서.

`MC_GrpContext` 오프셋은 CLAUDE.md 의 IDA 필드 순서와 정확히 맞아떨어졌다:
`+0x04..0x10` = `m_clip` 넷, `+0x1c` = `m_transpxl`, `+0x20` = `m_alpha`,
`+0x2c` = `m_pfnPixelOp`, `+0x30` = `m_param1`. **idb 에서 건진 필드 이름이
처음으로 값어치를 했다.**

### 20.2 전사한 원본 버그 하나

진입 클리핑(`0x148438..0x1484cc`)에서:

```
0x148490   addlt fp, fp, sb     ; if (src_y < 0) fp += src_y
```

`fp` 는 **너비**다. 소스 Y가 음수면 열 수가 아니라 **행 길이**가 줄어든다.
원본 버그로 보이지만 **고치지 않았다.** 이 파일의 계약은 "같은 바이트"이고,
고치는 순간 이식이 아니게 된다. 재생에서 `src_y < 0` 으로 들어오는 경로는
없어서 관측된 영향도 없다.

### 20.3 이식 범위와 결과

이식한 것: **16비트 목적지 · 불투명 · 픽셀 op 없음** — 게스트가 화면을
올릴 때 쓰는 경로. 사양(decline)한 것:

* 목적지가 `0x10` bpp 가 아님 (`0x148620` 의 32비트 갈래)
* `m_alpha <= 0xfe` — 픽셀마다 `_midas_funcGrpPixelOpProc`(`0x148024`) 호출
* `m_pfnPixelOp != 0` — 픽셀마다 게스트 간접 호출 (`0x1487a4`)

**인수 기준 — 오프라인 4장면 905 프레임 전부 바이트 동일:**

| 장면 | 프레임 | no-patch | patched | 배수 |
|---|---|---|---|---|
| newgame | 119 동일 | 830.2 M | 290.3 M | (변화 없음 — 이 함수를 안 부른다) |
| **ingame** | **543 동일** | **25.30 G** | **3.84 G** | **6.58배** |
| shop | 134 동일 | 2.78 G | 291.1 M | **9.56배** |
| online | 109 동일 | 2.48 G | 251.0 M | **9.90배** |

`--selftest` 29개 통과.

### 20.4 이제 프로파일이 평평하다

```
25.6%  DrawImageBlendingClip565   <- 사양된 4비트 경로
12.6%  _midas_funcFntDrawString
11.7%  DrawImageStretchClip565
10.6%  DrawImageMirrorClip565
 8.0%  ChangeFrameBufferBlending565
 6.5%  FillRectBlending
```

`internalGrpCopyFrameBuffer` 는 목록에서 사라졌다. 지배적인 하나가 없으므로
남은 것은 추측 없이 위 순서대로 하면 된다. **기기 실측(S9 30~37 fps,
S25 44~45 fps)은 전부 이 이식 전 수치다** — 다시 재야 한다.

## 21. 캐시샵 카탈로그 디코드 (2026-08-19)

`tools_re/vfs.py` (아카이브 리더)와 `tools_re/cashcat.py` (카탈로그 디코더)를
새로 넣었다. 아카이브는 **리틀엔디언** `[u32 hash][u32 size][size]` 반복이고
(문서에 엔디언이 없었다), 2,189 엔트리가 16,399,297 바이트에 정확히 맞는다.

`c/csv/cash_single_android.dat` (2,530 B, 30행)의 레코드 구조는 추측이 아니라
`CashShop::LoadCashShopData` (`0xde07c`)가 24바이트 `CashItemInfo` 로 읽는
순서 그대로다:

```
u16 count
행마다: u16 len, u8 id->+0x00, u16 param->+0x02, u8 namelen; name->+0x04,
        u8 a->+0x08, u8 count->+0x09, u8 desclen; desc->+0x0c,
        u16 price->+0x10, u16 price2->+0x12, u16 price3->+0x14
```

**실측과 교차 검증된 것 둘:**

* `id` — 재생이 엘릭서(20)(id 4)를 사고 서버가
  `approving purchase of item 4` 를 찍는다.
* `price` — 부활의 부적이 100이고 S25 화면이 "실제 현금 100원"이다.

`count` 는 수량이다 — 괄호 안 숫자와 정확히 일치한다(엘릭서(20)→20,
부활의 부적(5)→5, 나머지 1).

**아직 확정 못 한 것** (다음 세션이 다시 유도하지 않도록):

* `param` 은 u16 이고 273까지 간다 → `_NET_ITEM_.kind`(u8)가 **아니다.**
  무관한 행끼리 값을 공유한다(창고 확장과 공허 무기 5종이 모두 222) →
  아이템 정체가 아니라 그룹/아이콘이다.
* `a` 는 행마다 유일한데 **부활의 부적 두 행만 같다** — 같은 아이템의 수량
  차이다. 지급 아이템의 가장 유력한 후보지만 관측으로 확인되진 않았다.
* `price2`/`price3` 는 0이 아닐 때 `price` 의 ~70% / ~50% 로 일정해 세일가로
  보인다. 단 **`CashShop::GetDiscount`(`0xdbeb8`)는 이 필드를 읽지 않는다** —
  `gv[0x740 + n*4 + 4]`, 즉 서버가 채우는 전역을 본다. "할인"은 의도에 대한
  추측이지 추적된 사실이 아니다.

### 20.5 S25 재측정 — 게임플레이가 7 fps 였다 (2026-08-19)

이식 전/후를 **같은 기기·같은 장면**에서 쟀다. 비교를 위해
`copy_frame_buffer` 등록만 잠시 주석 처리한 APK 를 따로 올렸다
(같은 던전, 같은 위치, 같은 HUD — 스크린샷으로 확인).

| 장면 | 이식 전 | 이식 후 | |
|---|---|---|---|
| 인트로 | 51~57 fps | 51~57 fps | 게스트 타이머가 정한다 (§16) |
| 타이틀 | 43.9 fps | 43.9 fps | 변화 없음 |
| 메인 메뉴 | 44~45 fps | 44 fps | 변화 없음 |
| **게임플레이** | **7.0 fps** | **35~41 fps** | **5.9배** |

이식 전 게임플레이는 `guest 9,870 ms / 10,000 ms` — 프레임의 **98.7%를
인터프리터가 먹고** present 는 175 ms 였다. 완전히 게스트 바운드다.
이식 후에는 6,790 ms 로 내려온다.

**여기서 배울 것은 숫자가 아니라 어디를 재느냐다.** 문서에 적혀 있던 기기
수치(S9 타이틀 30~37, S25 메뉴 44~45 fps)는 전부 **쉬운 장면**이었다.
실제 최악은 게임플레이였고 **한 번도 측정된 적이 없었다** — 7 fps 는 사실상
플레이 불가인데, 메뉴만 보고 "S25 에서 메뉴는 더 이상 병목이 아니다"라고
적어 둔 탓에 급하지 않은 작업으로 분류돼 있었다. §20 이 후보 목록을
newgame 프로파일로 정해서 틀렸던 것과 **같은 실수의 기기 쪽 판본**이다.

메뉴·타이틀이 44 fps 에서 꼼짝 않는 것은 이 이식이 그 장면에서 안 불리기
때문이다(호스트 newgame 도 명령어 수가 그대로였다). 그 장면들의 병목은
§20.4 목록의 다른 함수들이다.

## 22. "네트워크 없는 캐시샵" — 조사 결과 (2026-08-19)

사용자 요구는 **네트워크 없이, 인게임으로, 기존 가격에 취소선 + 0원**이다.
크로노스 소드에 같은 기능(`CS_FREE_CASH`)이 있으므로 먼저 그것을 읽고 HL5 와
대조했다. 결론: **네 조각 중 하나는 이미 있고, 하나는 구조가 달라 그대로 못 온다.**

### 22.1 이미 있는 조각 — MTX

크로노스의 1번 조각(무한 로딩의 정체)은 HL5 에서 이미 해결돼 있다.
`cletentry.rs` 의 `requestMTX` 가 `pending_mtx_result` 를 세우고
`lib.rs::deliver_mtx_result` 가 다음 프레임에 게스트가 등록한 콜백을 부른다.
통신사 없이 결제가 성공한다.

### 22.2 그대로 못 오는 조각 — 지급

여기가 두 게임의 갈림길이다.

| | 크로노스 소드 | 히어로즈 로어 5 |
|---|---|---|
| 지급 주체 | **게스트 로컬 코드** (`NpcUI_setBoughtItemOnCASH`) | **서버** |
| 경로 | MTX 콜백 → 게스트가 가방에 넣는다 | `ProcPacket_Shop` → 와이어에서 읽어 넣는다 |

HL5 의 경로를 디스어셈블로 확인했다 (`ProcPacket_Shop`, `0xcd004`):

```
0xcd228  HERO::GetBagItemPtr()
0xcd238  BagItem::NewBagNetItem(_NET_ITEM_*, int, char)   가방에 넣는다
0xcd258  NETWORK::readNetItem(int, _NET_ITEM_*)            와이어에서 읽는다
```

즉 **아이템은 서버가 준다.** 그래서 이 작업은 크로노스 코드의 이식이 아니라
**지급 경로를 새로 만드는 일**이고, 그 사실이 견적을 바꾼다. 후보 둘은
CLAUDE.md 의 "캐시샵을 로컬로" 절에 (A)/(B) 로 적었다. (A)(호스트가 카테고리 6 을
로컬로 합성)를 권하는 이유는 게스트의 지급 코드가 손대지 않은 채로 돌기 때문이고,
`_NET_ITEM_` 의 정확한 바이트 배치는 `hl5_server/protocol.py::net_item` 에 이미
있다 — `readNetItem` 을 그대로 따라 쓴 것이다.

**두 후보의 공통 선행조건은 "어느 필드가 지급 아이템인가"** 하나다. §21 의
미확정 항목이 그대로 여기 걸린다. 추론으로 좁히면 `a` 가 유력하지만
(행마다 유일 + 부활의 부적 두 행만 같음 = 같은 아이템의 수량 차이),
**실서버로 한 번 사서 서버가 보낸 `kind`/`index` 를 카탈로그 행과 대조하는
쪽이 훨씬 빠르고 확실하다.** 서버는 이미 떠 있고 기기에서 붙는다(§19.1).

### 22.3 가격 표시 — 후크 지점은 하나

> **정정: 셋이다, 그리고 아래가 고른 자리는 이 프로젝트의 재생이 지나가지
> 않는 쪽이다 (§25.4).** 가격 필드 오프셋(`+0x10`/`+0x12`/`+0x14`)과 "할인
> 세 갈래가 한 `DrawText` 로 수렴한다"는 맞다 — 다만 그것이 **레이아웃마다**
> 맞고, 레이아웃이 둘이라 자리가 셋이다. 아래는 조사 시점 그대로 남긴다.

`StateCashShop::DrawAutoCashShopInfo` (`0xdc1cc`, 1,096바이트)가 가격을 그린다.
세 변종이 있는데 **전부 한 호출로 수렴한다:**

```
0xdc3b0  GetDiscount(...)  == 1 -> 0xdc5b4 -> ldrsh [r7,#0x12]   price2
0xdc3c8  GetDiscount(...)  == 2 -> 0xdc5cc -> ldrsh [r7,#0x14]   price3
                         그 밖              -> ldrsh [r7,#0x10]   price
                                    ↓ (셋 다 0xdc3e8 로 합류)
0xdc3f4  TextMgr::GetCommonText(0x109)
0xdc40c  MC_knlSprintk
0xdc464  BFont::DrawText(iiiiPci)      복귀주소 0xdc468  ← 후크는 여기 하나
```

`[r7,#0x10]` 이 가격이라는 것은 §21 의 카탈로그 파서와 **독립적인 두 번째
확인**이고, 동시에 `price2`/`price3` 가 할인 단가라는 것을 추측에서 사실로
바꿨다. 인자는 `(this, fb, r5+0xe, r6+0x18, 0x82, str, 0)`, 문자열이 6번째다.
화면에 보이는 것은 `● 500원` 한 줄이다.

**난관은 취소선의 x 좌표다.** 글리프가 실제로 어디 앉는지는 1,252바이트짜리
`BFont::DrawText(iiiiPciiii)` (`0x43ba4`) 의 규약에 달려 있다 — 크로노스가
"측정기는 ASCII 글리프마다 1px 을 더 얹는데 그리기 루프는 안 얹는다, 두 측정을
섞으면 선이 3~5px 어긋난다"고 적어 둔 그 함정이다.

> **HL5 는 여기서 크로노스보다 유리하다.** 측정기를 전사할 필요가 없다 —
> `BFont::GetWidth` (`0x43774` / `0x436a0`) 를 `Vm::call` 로 **게스트 자신의
> 것을 부르면** 두 측정을 섞을 방법이 없다. 재귀는 크로노스와 같은 수단으로
> 막는다: 후크가 복귀주소로 자기 호출지점을 식별하고, `Vm::call` 은
> `LR = RETURN_MAGIC` 이므로 스스로 부른 것은 후크에 걸리지 않는다.

### 22.4 인수 기준

§16.10(부팅 변경)과 같은 문제다 — **의도한 동작 변경이므로 바이트 동일이
정의상 불가능하다.** 대체 기준 셋:

1. **가격 줄만 다를 것.** 캐시샵 장면을 찍고 가격 사각형을 마스킹한 뒤
   `ab_compare.py` 에 넣어 나머지가 프레임 단위로 동일함을 보인다.
2. **`patch.rs` 의 오프라인 905 프레임 A/B 가 그대로 통과할 것.** 그러므로
   가격 후크를 `patch.rs` 에 넣지 마라 — 그 파일의 계약은 "같은 바이트를 만드는
   이식"이고, 동작 변경이 섞이면 그 A/B 가 무엇을 증명하는지 말할 수 없게 된다.
3. **서버 없이 실제로 사서 가방에 들어올 것.**

## 23. 전체화면과 호스트 쪽 프레임 비용 셋 (2026-08-18, 원격 세션)

사용자 요구 둘: **프레임이 끊긴다**, 그리고 **양옆이 비는 화면을 폰 전체로
꽉 채워라.** 이 세션은 `payload/` 가 없는 원격 컨테이너에서 돌았다 — 게스트
바이너리는 라이선스 데이터라 저장소에 없으므로, **프로브도 A/B 재생도 역어셈블도
불가능하다.** 그래서 §20.4 의 블리터 이식(게스트 코드 전사가 필요한 것)은 손대지
않았고, 게스트 바이트에 의존하지 않는 호스트 쪽 비용 셋을 치웠다. 전부
`cargo build` 로 검증된 구성 변경/재배치이고, 기기 실측은 다음 로컬 세션 몫이다.

### 23.1 전체화면 — 게스트에 400×240 을 그대로 말한다

지금까지 기기에서는 게스트에게 실제 창 크기를 말했고, `fit_screen` 이
`adjustAutoScreen` 대로 균등 배율 + 여백(S25: 4.5배, margin_x 270)을 계산해
게스트가 자기 쿼드를 그 안에 앉혔다. 그 여백이 사용자가 말한 "양옆이 잘린
화면"이다.

바꾼 것: `hl5_android::App::boot` 이 `Config` 에 창 크기 대신 **WIPI 화면
그대로(400×240)** 를 넣는다. `fit_screen` 은 배율 1.0 / 여백 0 이 되고 — **이것이
프로브가 모든 재생과 모든 A/B 에서 항상 써 온 바로 그 구성이다** — GL 캡처
프레임버퍼가 400×240 이 된다. `present()` 가 그것을 창 전체로 **축별 독립**
확대한다(fill, fit 아님). 5:3 → 기기 비율로 가로가 늘어나는 것은 사용자 결정이다.

터치는 역방향으로 같은 소유자를 갖는다: 셸이 창좌표를 프레임버퍼 비율로 줄여
`map_touch` 에 넘기고, `map_touch` 는 (배율 1/여백 0 이므로) 클램프만 남는다.

부수 효과가 이 절의 프레임 절감 절반이다: 게스트 쿼드 확대가 창 크기
(~2.4 M 샘플/프레임, 텍셀 디코드 포함)에서 400×240(96 K)으로 줄고, `present`
의 전체창 `fill(0)` 도 사라진다(모든 픽셀을 어차피 쓴다).

자바 쪽도 한 줄 있다: `MainActivity` 가 `layoutInDisplayCutoutMode` 를 안 정해서
가로 모드에서 펀치홀 옆이 시스템 레터박스로 남았다. P+ 에서 SHORT_EDGES,
R+ 에서 ALWAYS 로 컷아웃 영역까지 창을 확장한다.

### 23.2 present 스킵 — 게스트는 22 fps, 셸은 60 Hz 였다

게스트는 자기 fps 표(§16, 최대 22)대로만 그리는데 셸은 16.7 ms 마다 전체창
패스를 돌렸다 — 프레임의 2/3 는 **이미 화면에 있는 것과 비트 동일한 버퍼**를
다시 칠한 것이다. `vm.gl.draws` (glDrawArrays 카운터)가 안 움직였으면 present
를 건너뛴다. 컴포지터가 마지막 버퍼를 유지하므로 화면은 그대로다. resize /
RedrawRequested / resume 은 무조건 present 한다(리사이즈 직후 버퍼 내용은
미정의라서).

### 23.3 인터프리터 핫루프 — 명령어마다 SipHash 를 하고 있었다

`Vm::run` 은 **해석하는 모든 명령어마다** `patches: HashMap<u32,_>` 를 조회한다
— std 기본 해셔(SipHash)로 PC 를 해시해서, 4개짜리 맵에 "없다"는 답을 듣는 게
명령어당 고정비용이었다. 이미지 워드당 1비트짜리 비트맵(`patch_filter`,
설치된 최고 주소까지 ~수 KB)을 `install_patches` 가 만들고, 핫루프는 shift +
load + mask 로 프리필터한다. 비트가 선 주소만 HashMap 을 본다. `--no-patch` 면
비트맵이 비어 bounds 검사에서 즉시 떨어진다. **게스트 가시 동작 불변** — 같은
주소에서 같은 패치가 뜨고, 사양(decline) 경로도 그대로다.

### 23.4 이 세션이 증명한 것과 못 한 것

* 증명한 것: 세 변경 전부 리눅스 `cargo build` 통과. 23.3 은 게스트 동작에
  중립(디스패치 결과가 주소 단위로 동일). 23.1 의 게스트 쪽 구성은 새 실험이
  아니라 프로브가 늘 쓰던 구성.
* 못 한 것(payload 없음): 905 프레임 A/B 재확인, `--selftest`, 기기 fps 실측,
  그리고 **§20.4 블리터 이식 자체** — 25.6% 짜리 4비트 블렌딩 경로부터의 그
  목록이 게임플레이 프레임의 본체이고, 게스트 역어셈블이 필요해서 여기서는
  불가능했다.
* 안드로이드 전용 코드 두 곳(struct 리터럴의 `presented_draws` 초기화,
  MainActivity 의 컷아웃)은 NDK 가 없어 컴파일 확인을 못 했다 — 단순 추가라
  위험은 낮지만, 다음 `build_android.ps1` 이 첫 검증이다.

다음 로컬 세션 검증 순서: `--selftest` → 오프라인 4장면 905 프레임 A/B
(23.3 이 섞였으니 반드시) → 기기 설치 후 (1) 화면이 컷아웃까지 꽉 차는가,
(2) 터치가 늘어난 비율대로 맞는가, (3) 10초 로그의 present ms 와 fps.

## 24. 인프로세스 사설 서버 — 캐시샵의 "네트워크 없이" 절반 (2026-08-18, 원격 세션)

§22 의 후보 (A)("호스트가 카테고리 6 을 로컬로 답한다")를 구현했다. 다만 조사가
가정했던 것보다 (A) 의 범위가 넓었다: 게스트의 캐시샵은 소켓을 열고 **로그인부터
다시 하므로**(§16.7), 구매 응답 하나가 아니라 접속·로그인·잔액·구매 왕복 전체를
답해야 한다. 그래서 조각 하나가 아니라 **사설 서버 전체를 프로세스 안으로**
옮겼다.

### 24.1 만든 것

* `hl5_core/src/localserver.rs` — `hl5_server/server.py` 의 전사.
  측정으로 검증된 핸들러 전부: 1/1(로그인, 헤더 폰번호/IMEI 폴백과 무신원 거절
  포함), 1/5(잔액 0/0), 9/1(빈 이벤트 목록 — Connecting 을 지우는 그 패킷),
  6/2(구매 승인 — **응답 opcode 는 4가 아니라 2**, §18 의 교훈 그대로),
  6/5(지급, count 0), 6/7, CrossRoad 3종(DAILY_ACTIVE_USER 0x19 /
  LATEST_GAME_VERSION 0x1A+빈문자열 / 그 밖 0x19). 미지원 카테고리는 배포
  서버처럼 **무응답**. EUC-KR 거절 문구 둘은 미리 인코드한 상수다.
* `files.rs` 에 `FdObj::Local` — `Config::server` 미설정이고
  `Config::local_server` 가 켜져 있으면 `connect` 가 소켓을 인프로세스 세션으로
  바꾼다. 논블로킹 connect 는 실서버와 같은 모양(EINPROGRESS → select 쓰기가능 →
  `SO_ERROR`=0)으로 완료되고, `send` 가 세션에 바이트를 먹이면 응답이 그 자리에서
  recv 큐에 쌓인다. `select` 는 "실 스트림도 다이얼도 없는" 경우를 즉답으로
  처리한다 — 로컬 세션의 준비 상태는 이 스레드 자신의 send 로만 바뀌므로 기다릴
  것이 없고, 유휴 대기는 죽은 서버 세계와 똑같이 가상시계에 청구된다.
* 배선: 프로브 `--local-server`, 안드로이드 셸은 **서버 미설정이면 자동으로 켠다**
  (`hl5_server.txt` 나 `HL5_SERVER` 가 있으면 실서버가 이긴다).

### 24.2 검증 — 게스트 없이 이 환경에서 가능한 것의 전부를 했다

이 세션에도 `payload/` 가 없어 게스트 재생은 불가능하다. 대신 이 작업에는 게스트
말고도 **정답지가 하나 더 있다**: 배포된 Python 서버 자신이다. 그 서버는 실제
클라이언트를 상대로 디버그됐고(§18: opcode 2, CrossRoad 질의별 ack), S25 가 공개망
으로 로그인한 실물이다(§19.1).

`tools_re/gen_localserver_fixtures.py` 가 **hl5_server 실물 코드**(`server.Session`
+ 메모리 스토어)를 스크립트 대화로 몰아 요청/응답 쌍 16개를 덤프하고,
`hl5_core/tests/localserver_ab.rs` 가 같은 요청을 Rust `Session` 에 넣어 **바이트
동일**을 단언한다. 로그인 3변종(페이로드 폰 / 헤더 폰 / 헤더 IMEI), 무신원 거절,
미로그인 구매 거절, 구매 왕복, 미요청 지급, 무응답 카테고리, CrossRoad 3종이 다
들어 있다. 추가로: 1바이트씩 조각내 보낸 요청의 재조립이 통짜 전송과 동일할 것,
SmartReq 아닌 스트림은 끊고 EOF 를 보고할 것, `net_item` 레코드가
`protocol.net_item` 과 동일할 것. `cargo test -p hl5_core` 4개 전부 통과.

### 24.3 하지 않은 것 — 지급은 여전히 비어 있다

> **이 절은 §25.2 가 대체했다.** 매핑은 확정됐고 지급은 돈다. 아래는 이
> 세션 시점의 상태로 남긴다.

6/5 의 지급 목록은 배포 서버와 똑같이 **count 0** 이다. 어느 카탈로그 필드가
`_NET_ITEM_.kind/index` 가 되는지는 여전히 관측되지 않았고(§22.2, `a` 유력),
추측으로 채우면 틀린 아이템이 실제 세이브의 가방에 들어간다. `net_item` 빌더는
픽스처 A/B 아래 준비돼 있다 — 매핑 확정이 지급 전의 유일한 남은 단계다.

기존 동작은 전부 불변이다: `Config::local_server` 기본값이 `false` 라 프로브
재생·905프레임 A/B·S9 정답지 대조는 종전의 죽은 서버 세계 그대로다.

### 24.4 다음 로컬 세션의 검증 순서

1. `cargo run -p hl5_probe -- --boot --local-server --script tools_re/scripts/ingame.txt`
   — 캐시샵에서 6/2 → 6/5 가 로그(localserver: purchase …)에 남고 게스트가
   "Connecting" 을 스스로 지울 것. 실서버 재생(§18)과 같은 화면 흐름이어야 한다.
2. 오프라인 905프레임 4장면 A/B — `local_server` 기본 off 라 **변화 없음**이
   정답이다 (select 즉답 분기가 죽은 서버 세계의 바이트를 바꾸지 않았다는 확인).
3. 기기: `hl5_server.txt` 없이 설치 → 캐시샵 진입·구매 왕복. 비행기 모드에서도
   같아야 한다.

## 25. 캐시샵 로컬화 완결 — 그리고 병렬 세션 둘을 합치면서 드러난 것 (2026-08-19)

두 갈래가 같은 기능을 각자 구현했다. `main`(§24)은 `hl5_server` 를 전사한
`localserver::Session` 을 냈고, 원격 갈래는 `shop::LocalPeer` + 지급 표 +
가격 후크를 냈다. **서버 절반이 중복이었고, 나머지 절반은 서로에게 없었다.**

이 세션은 그 둘을 합치고, 원격 갈래가 payload 없이 남겨 둔 미검증 셋을
**전부 확정하고**, 그 과정에서 **와이어 형식 두 군데와 후킹 지점 하나가
틀려 있었다는 것**을 찾아냈다. 마지막 것은 원래 계획대로 갔으면 기기에서
"후크가 고장났다"로 보였을 것이다.

### 25.1 병합 — 증명이 있는 쪽을 남긴다

| | 남긴 것 | 버린 것 |
|---|---|---|
| 인프로세스 서버 | `localserver::Session` | `shop::LocalPeer` |
| 지급 표·카탈로그 | `shop::GrantTable` | — |
| 가격 후크 | `shop::install_hooks` | — |
| 소켓 표현 | `FdObj::Local(Session)` | `FdObj::Socket{ local: … }` |

기준은 하나였다: **`Session` 에는 픽스처 A/B 가 있다**
(`gen_localserver_fixtures.py` 가 진짜 `hl5_server` 코드를 돌려 요청/응답 쌍을
덤프하고, 테스트가 그것을 Rust 로 재생해 바이트 대조). `LocalPeer` 에는 없었고,
실제로 세 군데가 달랐다 — 1/1 이 신원을 안 읽고 무조건 성공, 6/2·6/5 가
로그인 게이트 없음, 죽은 세션이 EOF 를 보고하지 않음(게스트가
`StatePopup` 의 60초 감시에 걸린다). 반대로 `Session` 은 6/5 를 count 0 으로
못 박아 두어 지급을 못 했다 — 그 자리에 `GrantTable` 을 끼웠다.

`Config::free_shop` 은 `Config::local_server` 를 **함의**하도록 했다. 네 번째
세계를 만들 이유가 없다: 네트워크 없이 산다는 것이 곧 인프로세스 서버가
카테고리 6 을 답한다는 뜻이다.

`select` 는 HEAD 쪽이 맞았다 — 로컬 세션만 살아 있으면 `any_live_socket()` 이
false 라 **가상시계로 즉답**한다. 원격 쪽은 로컬 세션을 "살아 있는 소켓"에
넣어 폴링 루프에 들여보낸 뒤 루프 안에서 다시 빠져나오게 했다. 결과는 같지만
코드 경로가 둘이 된다.

**후크 디스패치는 고쳐야 했다.** `hooks` 를 `!is_empty()` 로만 감싸 두었는데,
그러면 `--free-shop` 을 켜는 순간 §23.3 이 없앤 **명령어마다의 SipHash** 가
그대로 돌아온다(후크가 앉는 자리가 하필 폰트 함수다). `patches` 와 같은
비트맵 프리필터를 쓰도록 합치고 이름을 `dispatch_filter` 로 바꿨다. 후크는
`boot()` 에서, 패치는 그 전에 설치되므로 **양쪽이 바뀔 때마다 재구축**한다.

### 25.2 지급 매핑 — 관측이 아니라 코드로 확정했다

§22 는 "실서버로 사서 관측한다"를 적어 뒀다가 "배포 서버도 count 0 이라
성립하지 않는다"로 정정했었다. 남은 길은 게스트를 읽는 것이었고, 그것으로
끝났다.

```
kind  = 18                       항상. item_18.dat = CashItemInfo, 49행
index = 카탈로그의 `a` 필드
count = 카탈로그의 수량 필드
```

**결정적 증거는 하드코딩된 상수 둘이다.** `ItemTable::GetItemTableInfo`
(`0xa2728`)는 `kind` 를 19개 테이블(`/c/csv/item_%02d.dat`)로 가르고 `index` 에
행 크기를 곱한다. 그리고 `ProcPacket_Shop` (`0xcd38c`) 이 이렇게 한다:

```
cmp   r1, #0x12000000        ; kind == 18 ?
ldrb  r3, [sp, #0x1a5]       ; index
sub   r2, r3, #0x2c          ; index - 44
cmp   r2, #4
bhi   <일반 경로>            ; 44..48 이 아니면 그냥 지나간다
add   r3, r3, #0x25          ; index + 37  -> 81..85
ldrb  lr, [r1, #0x22c]       ; HERO 클래스 0..3
bl    ItemTable::NewItemToBag(kind=클래스, index+37)
```

`item_18.dat` 44~48행은 공허 1~5급무기이고, `item_00..03.dat`(롱소드/대거/
콜드/스피어 = 네 클래스) 81~85행은 1~5급 공허대검/단검/권총/장창이다.
`-44` / `+37` 이라는 두 리터럴은 **`index` 가 `item_18` 의 행 번호일 때만**
맞물린다. 그 규칙으로 카탈로그 30행을 조인하면 **30/30 이 이름까지 일치**한다.

`param` 은 지급 아이템이 아니라 **아이콘 id** 였다 — 29/30 행에서
`item_18[a]` 의 아이콘 필드와 같다(id 6 오브원석만 카탈로그 쪽이 낡았고 이름은
맞는다). "무관한 행끼리 값을 공유한다"고 이상해했던 것(창고확장과 공허무기가
모두 222)은 그냥 **같은 아이콘을 쓴다**는 뜻이었다.

> **게스트는 `index` 를 검사하지 않는다.** `GetItemTableInfo` 는 `mla` 로
> 곱할 뿐이다. 범위를 넘긴 index 는 실패하지 않고 테이블 뒤 메모리로
> 아이템을 만들어낸다. 그래서 호스트가 막는다(`CASH_ITEM_ROWS`, 범위 밖 행은
> `GrantTable::rejected`).

### 25.3 와이어 레코드가 두 군데 틀려 있었다

`docs/SERVER.md` §6.5 의 `_NET_ITEM_` 표는 `readNetItem` 을 읽어 만든 것이라
필드 **순서**는 맞았지만 두 가지가 틀렸다. 둘 다 지급을 실제로 켜기 전까지는
증상이 없었고, 양쪽 구현이 **똑같이** 틀려 있어서 픽스처 A/B 도 잡지 못했다
(그 테스트가 증명하는 것은 "Python 과 같다"이지 "게스트가 원하는 것과 같다"가
아니다 — 이 구분이 이번에 값을 했다).

**1. `+0x00` 은 uid 가 아니라 수량이다.**

```
ProcPacket_Shop  0xcd230:  ldr r2, [sp, #0x1a0]   ; _NET_ITEM_+0x00
                 0xcd234:  mov r3, #1
                 0xcd238:  bl  NewBagNetItem(_NET_ITEM_*, int, uchar)
NewBagNetItem    0xa5b08:  subs r6, r2, #0 / ble return
                 0xa5b24:  add r7, r7, #1 / bl NewBagNetItem(item, flag)
                           cmp r6, r7 / bgt 0xa5b24        <- n 번 도는 루프
```

행 번호를 넣었으면 **그 숫자만큼** 지급됐을 것이다. 고친 뒤 실측:
엘릭서(20)을 사면 `r2 = 0x14 = 20`.

**2. `kind > 10` 이면 레코드는 이름에서 끝난다.**

```
readNetItem 0xc85e0:  ldrsb r3, [r4, #4]
            0xc85e4:  cmp   r3, #0xa
            0xc85e8:  ble   0xc85f0          ; <= 10 일 때만 뒤를 읽는다
            0xc85ec:  pop   {r4, r5, r6, pc} ; 그 외는 여기서 끝
```

캐시 아이템은 전부 kind 18 이므로 뒤의 58바이트는 읽히지 않는다. 붙여 보내면
**다음 레코드**의 필드로 읽혀 목록이 어긋난다(한 개짜리 목록에서는 무해해서
더 늦게 발견됐을 것이다). 짧은 형식은 10바이트 + 이름.

`protocol.py`·`localserver.rs`·`docs/SERVER.md` 를 함께 고치고 픽스처를
재생성했다. 픽스처의 `N` 줄은 이제 **두 줄**이다 — 짧은 형식과 긴 형식.
두 모양이 다르다는 사실 자체를 테스트가 붙잡고 있어야 한다.

### 25.4 후킹 지점이 하나가 아니라 셋이었다 — 그리고 쓰던 것은 안 쓰이는 쪽

§22.3 은 "세 변종이 `DrawAutoCashShopInfo` 안의 `DrawText` 하나로 수렴한다"고
적었다. 수렴하는 것은 맞는데 **레이아웃마다** 수렴하고, 레이아웃이 둘이다.
그리고 §22 가 고른 자리는 이 프로젝트의 재생 스크립트가 한 번도 안 지나간다.

증상은 정확히 이랬다: 캐시샵에 들어가 구매가 완주하는데
`free-shop hook 0 price line(s) redrawn`. `--watch` 로 확인하니
`DrawAutoCashShopInfo` 호출 **0회**.

| 레이아웃 | 그리는 함수 | 오버로드 | 복귀주소 |
|---|---|---|---|
| 상세 박스 | `DrawAutoCashShopInfo` (`0xdc1cc`) | `DrawText(iiiiPci)` (`0x4414c`) | `0xdc468` |
| 간이 오버레이 ① | `StateCashShop::Paint` (`0xdc614`) | `DrawText(iiiPcii)` (`0x445e8`) | `0xdcdc4` |
| 간이 오버레이 ② | 같은 함수 | 같은 오버로드 | `0xdd244` |

`Paint` 안의 두 자리도 각각 할인 세 갈래의 합류점이다 — `+0x12` / `+0x14` /
`+0x10` 을 읽는 세 갈래가 전부 같은 `GetCommonText(0x109)` +
`Sprintk("%d%s")` 로 `b` 한다.

**복귀주소 검사는 장식이 아니다.** `Paint` 는 같은 `0x445e8` 오버로드로
`0xdcc2c`·`0xdd078` 에서 **아이템 이름**도 그린다. 함수만 후킹하면
"엘릭서(20)"에 취소선이 그어진다.

인자 배치도 오버로드마다 다르다: `(this, fb, x, y, wrapWidth, str, align)` 대
`(this, fb, x, y, str, align, colour)`. 그래서 후크 둘이 각자 인자만 풀고
공통 몸통에 넘긴다. 몸통은 **받은 인자를 그대로 되돌려 호출**하므로 자기가
안 쓰는 인자의 의미를 알 필요가 없다.

덤으로 §22 의 이름 하나가 틀렸다: `DrawText(iiiiPci)` 의 4번째 인자
`0x82` 는 **색이 아니라 줄바꿈 폭**(130px)이다. 색은
`BFont::SetColor(0x00ffffff)` 가 따로 정한다. 통과시키기만 하므로 동작에는
영향이 없었지만, 다음 사람이 `SetColor` 로 "고치려" 들 이름이었다.

### 25.5 미검증 셋 — 둘은 확정, 하나는 실측

원격 세션이 payload 없이 남겨 둔 것들이다.

1. **`GetWidth(Pc,i)` 의 둘째 인자 = strlen** — **맞다.** 측정 커서가
   ASCII 에서 1, EUC-KR 선두바이트에서 2씩 나아가며(`0x436f4` / `0x4375c`)
   그 값과 비교되므로 **바이트 길이**다. 게스트 자신이 바로 이 가격 문자열에
   `strlen` 을 넘긴다(`0xdd128` → `0xdd13c`). 그리고 **크로노스의 "측정기
   두 개가 다르게 센다" 함정은 여기 없다** — 이 바이너리에는 측정기가
   `0x436a0` 하나뿐이고 `DrawText` 가 자기 정렬에 그것을 쓴다.
2. **`fb` 디스크립터 레이아웃 = `patch.rs mod fb`** — **맞다.** 게스트가
   필드 이름을 직접 준다(`FrameBufferInfo::FrameBufferInfo`, `0x51fe0`:
   `GetBpl`/`GetBpp`/`GetWidth`/`GetHeight`/`GetFrameBufPtr`), 그리고
   `Graphic::GetFB`(`0x52098`, `ldr r0,[r0]`)가 `MC_grpFillRect` 와 **같은
   포인터**를 넘긴다.
3. **취소선 y** — 디스어셈블로는 못 정한다(`HanFontHeight` 는
   `ldrsb [this+0x45]` 한 줄이라 잉크가 셀 어디에 앉는지 말하지 않는다).
   렌더된 가격에서 쟀다:

   ```
   free-shop: price "500원" at x=150 y=108, width=28, HanFontHeight=11
   잉크 = 108..118행  (정확히 y 에서 시작하는 11행)
   ```

   따라서 중앙은 `y + h/2` = **113**. 중간에 `0x43e34: add ip, fp, #1` 을
   보고 `+1` 을 넣었다가 1px 낮게 그어졌다 — **그 명령은 이 오프셋이 아니다.**
   후크가 이 입력들을 실행마다 한 줄 로그로 남기므로 스크린샷과 대조된다.

한 가지를 더 고쳤다: 취소선이 **클립을 안 봤다.** `MX_fntDrawString` 은
`(*_lcdG)+0xc` 의 `MC_GrpContext` 에서 클립을 가져오는데, `m_mask` 는
`MC_grpSetContext` 가 OR 만 하고 지우지 않아 **한 번 켜지면 끈적하게 남는다.**
지금 캐시샵 경로에는 `Graphic::SetClip` 호출자가 없어 잠복 상태지만, 우편함이나
퀘스트 화면을 먼저 들렀다면 선이 안 그려진 글리프 위를 지나갔을 것이다.
읽기 네 번이면 되므로 그냥 지키게 했다.

### 25.6 인수 기준 — 넷 중 셋 통과

1. **`patch.rs` 오프라인 4장면 905 프레임 A/B** — **통과.** 병합 전(2afbbaf)
   대비 119 + 543 + 134 + 109 전부 바이트 동일.
2. **§23(전체화면·present 스킵·핫루프)도 렌더 무변화** — **통과.**
   80616f7 대비 같은 905 프레임 전부 동일. 이 세션이 §23 을 처음 검증했다.
3. **서버 없이 사서 가방에 들어올 것** — **통과.** `--grant` 없이 카탈로그만
   으로 `NewBagNetItem(..., r2=0x14, ...)`.
4. **가격 줄만 다를 것** — **통과.** 같은 캐시샵 화면을 `--local-server`
   (후크 없음)와 `--free-shop`(후크 있음)으로 찍으면 픽셀 차분의 경계상자가
   **(150,108)–(195,119)** 하나다 — 가격 줄 그 자체이고, 마스킹하면 두
   프레임이 완전히 동일하다(`c3-shop`, `c4-shop`). 취소선이 이웃 위젯이나
   아이콘을 건드리지 않는다는 것이 이것으로 증명된다.

그 밖에 `--selftest` 29개, 유닛·픽스처 11개 통과.

> **함정 하나 더: 오프라인 A/B 는 `out/storage` 를 비우고 돌려야 한다.**
> `--free-shop` 재생이 세이브에 아이템을 넣어 두면 그 다음 오프라인 재생이
> 다른 화면을 그린다 — 905 중 534 프레임이 달랐고 원인은 코드가 아니라
> **세이브**였다. §18.4("흔들리는 쪽은 `--no-patch` 다")와 같은 계열이다.
> 두 번 속지 않으려면: A/B 전에 `rm -r out/storage`, 그리고 4장면을 항상
> **같은 순서로**.

## 26. S25 실기기 시험 — 캐시샵이 폰에서 무료로 돈다 (2026-08-19)

§25 까지는 전부 프로브(호스트) 위의 증명이었다. 이 절은 **갤럭시 S25 Ultra
실물**(SM-S938N, Android 16 / SDK 36, arm64-v8a 단독)에서 잰 것이다.
`build_android.ps1 -Release`, versionCode 10, `<keystore>.jks` 의 `hero5`
별칭으로 서명, **서버 주소 없이** 빌드(그래야 `free_shop` 이 켜진다).

### 26.1 먼저 걸린 것 — 설치가 안 됐다

기존 설치본이 **머신 공용 디버그 키**로 서명돼 있었다
(`CN=Android Debug`, SHA-256 `ef2572fc…`). 릴리스 키로 서명한 APK 는 그 위에
설치되지 않는다 — `INSTALL_FAILED_UPDATE_INCOMPATIBLE`. 이것은
`~/.gradle/gradle.properties` 의 chronosSword 주석이 미리 경고해 둔 바로 그
함정이다("그것이 S25 의 빌드가 업데이트 불가가 된 경위다").

유일한 길은 `adb uninstall` 이고, 그러면 `/data/data/…/files` 즉 **세이브가
지워진다.** 이번에는 잃을 것이 없었다(시험 전에 이미 `pm clear` 를 했다).
지금부터는 `hero5` 별칭으로 서명되므로 다음 빌드부터 제자리 업데이트가 된다.

> **릴리스 서명 빌드에 `-Server` 를 주면 무료 캐시샵이 꺼진다.**
> `free_shop`/`local_server` 는 `server.is_none()` 일 때만 켜진다. 그리고
> `option_env!("HL5_SERVER")` 는 컴파일 시각에 굳으므로, 이전 빌드가 서버를
> 구워 넣었다면 `cargo clean -p hl5_android` 를 먼저 해야 한다.

### 26.2 안드로이드 셸이 `vm.log` 를 한 번도 비우지 않고 있었다

첫 부팅 로그에 `free-shop: hooks on …` 이 없었다. 원인은 후크가 아니라 로그였다:
`hl5_android` 는 `vm.log` 를 **읽는 곳이 한 군데도 없었다.** 사설 서버의
로그인·구매·지급 줄, 가격 후크의 기하 정보, `--watch` 기록이 전부 아무도 보지
않는 `Vec<String>` 에 무한히 쌓이고 있었다.

§22 계획이 적어 둔 검증 방법("`adb logcat -s hl5` 에 `localserver: granting`
이 나와야 한다")은 **그 상태로는 성립할 수 없었다.** 프레임 루프와 `boot()`
직후에서 `vm.log.drain(..)` 을 `log::info!` 로 흘려보내게 고쳤다. 아래의 모든
관측은 그 수정 덕분에 가능했다.

### 26.3 §23 — 세 가지 다 실측했다 (처음)

| 항목 | 결과 |
|---|---|
| 전체화면 채움 | **통과.** 2340×1080 을 좌우 여백 없이 채운다. 컷아웃 영역까지 그린다. |
| 터치 대응 | **정확.** 3×3 메뉴 한가운데를 탭하니 하이라이트가 정확히 그 칸으로 갔다(이웃 아님). 게스트↔화면은 축별 배율 x5.85 / y4.5 로 왕복한다. |
| present 비용 | 아래 표 |

**fps 와 프레임 비용 (10초 창, 릴리스 빌드):**

| 장면 | fps | 게스트 ms/10s | present ms/10s | 게스트 ins/s |
|---|---|---|---|---|
| 인트로 / 타이틀 | 56.1~58.5 | 1,193~2,088 | 2,836~4,819 | 1.7~7.1 M |
| **메인 메뉴** | **42.0~42.2** | 5,645~5,680 | 2,051~2,068 | 18.8~19.0 M |
| **게임플레이** | **38.3~39.0** | 6,533~6,615 | 1,177~1,199 | 37.9~38.7 M |

§20.5 의 이식 후 수치(메뉴 44 / 게임플레이 35~41)와 같은 자리다 — 이번 병합이
성능을 되돌리지 않았다는 뜻이고, 그것이 이 표의 요점이다.

**병목이 장면마다 뒤집힌다.** 인트로는 게스트가 거의 놀고(1.2 s) present 가
가장 비싸다(4.8 s) — §16 이 "인트로는 게스트 바운드가 아니다"라고 한 그대로다.
게임플레이는 정반대로 게스트 6.6 s / present 1.2 s 다. present 스킵(§23.2)이
버는 것은 **인트로·타이틀 쪽**이고, 남은 블리터 이식이 버는 것은 게임플레이
쪽이다. 하나로 둘을 고칠 수 없다.

부팅→타이틀은 `am start` 로부터 **약 34초**(0.9초 간격 폴링, 프로세스 시작
포함). §18.5 의 28.3초와는 재는 방법이 다르니 같은 수로 읽지 마라.

### 26.4 캐시샵 — 폰에서 무료로, 네트워크 없이

부팅 로그가 먼저 말한다: `main: no server configured; answering in-process`,
그리고 `free-shop: hooks on _ZN5BFont8DrawTextEiiiiPci,
_ZN5BFont8DrawTextEiiiPcii` — **후크 둘 다** 걸렸다.

**가격 표시 — 통과.** 캐시샵을 열자 후크가 찍은 줄이 프로브와 **똑같다**:

```
free-shop: price "500원" at x=150 y=108, width=28, HanFontHeight=11 -> strike y=113
```

화면에는 `~~500원~~ 0원`(빨간 취소선이 500원 정중앙을 가로지르고 그 뒤에 0원)이
그려진다. §25.5 에서 렌더로 맞춘 `y + h/2` 가 기기에서도 같은 자리다.

**지급 — 두 경로 모두 통과.** 캐시샵은 인게임 HUD 와 **부활 흐름** 둘에서 열리는데
(CLAUDE.md), 이번에 우연히 둘 다 밟았다.

1. **부활의 부적 (id 26)** — 레벨 1 워리어가 고블린 셋에게 죽자 게임이 100원
   결제를 제안했다. 승낙하니:
   ```
   localserver: login ok account=<16자리>
   localserver: purchase requested item=26
   localserver: purchase paid item=26
   localserver: granting 1 record(s) for item=26
   ```
   그리고 **캐릭터가 살아났다 — HP 0 → 92.** 이것이 "가방에 들어온다"의 가장
   강한 증거다: 게스트가 받은 아이템을 실제로 **소비**했다. 같은 일이 세 번
   반복됐다(죽을 때마다).
2. **엘릭서(20) (id 4)** — 캐시샵에서 직접 구매. 같은 4줄이 `item=4` 로 남는다.

셋 다 **통신사 결제 없이, 서버 없이, 인터넷 없이**다. `--grant` 도 안 썼다 —
지급 목록은 게임 자신의 카탈로그에서 유도된다(§25.2).

### 26.5 곁다리로 알게 된 것

* **캐시샵은 세이브를 1회 이상 한 뒤에만 열린다** (로딩 화면 팁이 그렇게
  말하고, 실제로 그렇다). 세이브 없이 shop 을 눌러도 아무 일도 안 일어난다 —
  "버튼이 안 먹는다"로 보이지만 버튼 문제가 아니다.
* **이 UI 는 기기에서도 "탭 한 번은 하이라이트, 두 번째가 활성화"다.**
  프로브 스크립트의 `key select` 자리를 기기에서는 같은 곳 두 번째 탭으로
  바꿔야 한다.
* **`adb shell input keyevent` 는 게스트에 도달하지 않는다.** `WindowEvent::
  KeyboardInput` 의 Enter→SELECT 매핑은 물리 키보드용이고, 주입된 키는 그
  경로로 오지 않는다. 기기 자동화는 **터치만** 쓸 수 있다.
* **BACK 은 액티비티를 닫지 않는다.** `input keyevent 4` 뒤에도
  `mCurrentFocus` 가 그대로 `kr.heroeslore.five/.MainActivity` 다 — 시스템이
  가로채 세이브 도중에 앱을 끄는 일은 없다는 뜻이고, 그것이 코드가 BACK 을
  일부러 처리하는 이유였다.

## 27. 화면을 늘리지 않고 채운다 — 논리 캔버스를 넓힌다 (2026-08-19)

§23.1 이 "전체화면 채움"을 넣은 방식이 틀렸다. 사용자 보고: **"화면을 가로로
그냥 늘려놓은 느낌, 그래서 깨진 것 같다."** 맞는 진단이었다.

### 27.1 무엇이 잘못됐나 — 축별로 다른 배율

§23.1 은 게스트에게 화면이 400×240 이라고 말하고 `present()` 가 그것을 창
전체로 **축별 독립 배율**로 늘렸다:

```
가로 2340/400 = 5.85배
세로 1080/240 = 4.50배
```

가로로 **30% 잡아당긴 것**이고, 게다가 비정수 배율이라 최근접 샘플링이 어떤
열은 6픽셀, 어떤 열은 5픽셀로 복제한다 — 그래서 "늘어났다"가 아니라
"깨졌다"로 보인다. 실측으로도 확인했다: 타이틀의 `Tap to Continue` 글자폭이
400 캔버스 대비 **4.537배**(≈4.5, 세로와 같음)여야 정상인데, 늘린 빌드에서는
5.85배였다.

### 27.2 옳은 방법 — 캔버스를 화면 비율로 넓힌다

형제 프로젝트 크로노스 소드가 이미 푼 문제다
(`cs_android/src/lib.rs::boot`). **절대 늘이지 않는다. 게스트가 자기 화면이라고
믿는 것을 바꾼다:**

```
wipi_w = floor(240 × 화면가로 / 화면세로)      S25 에서 520
wipi_h = 240
배율   = min(2340/520, 1080/240) = 4.5         ← 축이 같다, 정의상
```

400×240 은 **`nativeInitDisplay` 의 인자이지 바이너리 안의 상수가 아니다.**
Java 쪽 `adjustAutoScreen` 이 400/240 리터럴을 넣을 뿐이고
(§27.5), 게스트 자신은 화면 크기를 런타임에 읽는다.

**HL5 게스트가 실제로 그렇게 하는지 440·480·520 세 폭으로 실측했다:**

| 요소 | 동작 |
|---|---|
| 메인 메뉴 3×3, 슬롯선택, 클래스선택, 각종 모달, **캐시샵 패널** | **중앙 정렬로 따라온다** |
| 미니맵, 스킬 버튼, DASH, 가상 d-pad, 보스 HP 바 | **오른쪽 가장자리에 붙어 따라온다** |
| 플레이어 HP/SP 바 | 왼쪽 가장자리 (원래대로) |
| GRB 등급 마크 | 오른쪽 가장자리에서 항상 10px — 폭 440/480/520 에서 각각 429.8/469.8/509.8 로 끝난다 |
| **인게임 map/shop/save 바** | **고정 논리 좌표 (161/211/261) — 따라오지 않는다** |

즉 게스트는 폭을 존중한다. 캐시샵 가격 후크도 그대로 동작한다 —
`x=150`(400 캔버스) → `x=210`(520 캔버스)으로 후크가 읽는 좌표 자체가 옮겨간다.

### 27.3 남는 흠 하나 — 인게임 상단 바가 왼쪽으로 치우친다

`map / shop / save` 세 글자는 논리 x 161/211/261 에 **고정**돼 있다. 400 캔버스
에서는 중앙(200) 근처라 가운데로 보이지만, 520 캔버스에서는 중앙이 260 이므로
바의 중심(213)이 **47 논리픽셀(화면 211픽셀) 왼쪽**에 앉는다.

가려지지도, 눌리지 않지도 않는다 — 그냥 가운데가 아니다. 전체가 30% 늘어나는
것과 바 하나가 치우치는 것 중에서는 후자가 낫다는 판단으로 그대로 둔다.
고치려면 그리기 지점을 후킹해 `화면폭/2` 기준으로 옮기고 탭 판정도 같이
옮겨야 하는데, 그것은 렌더 교정이 아니라 게스트 동작 변경이다.

### 27.4 어느 층이 확대하는가 — 크로노스와 다르게 간다

크로노스는 `Gl::new(screen_w, screen_h)` 로 **호스트 프레임버퍼를 화면 크기**
로 잡고 게스트의 쿼드가 확대를 하게 한다. HL5 는 그렇게 하면 안 된다 —
§23.2 가 없앤 전화면 샘플링 패스(2.4 M 픽셀)가 그대로 돌아온다.

HL5 가 택한 것(**Scheme B**): 게스트에게 `screen = wipi = 520×240` 을 말한다.
배율 1.0, 여백 0, `gl.fb` 는 **125 K 픽셀**(400×240 대비 +30%, 2.4 M 아님).
`present()` 는 손대지 않았다 — 캔버스가 이미 화면 비율이므로 그 안의 축별
계산이 **저절로 균일해진다**(S25 에서 정확히 4.5/4.5). `to_guest` 도 같은
이유로 그대로다.

### 27.5 원본은 무엇을 했나

`CletEntry.adjustAutoScreen` (dex, 순수 Java):

```
wipiLCDWidth = 400; wipiLCDHeight = 240;          ← const/16 리터럴
mag = min(long/400, short/240)
marginX = (long - (int)(400*mag)) / 2
marginY = (short - (int)(240*mag)) / 2
```

2340×1080 이면 **배율 4.5, marginX 270, marginY 0** — 즉 1800×1080 을 가운데
두고 좌우 270px 씩 검은 띠. 원본은 20:9 폰에서 화면의 23%를 버렸다. 클램프도
대체 경로도 없고, 축이 따로 스케일되는 일은 원본에선 **한 번도 없다.**

### 27.6 비용과 인수 기준

* **게스트 명령어: +0.2%** (부팅~타이틀 1500프레임에서 215.9 M → 216.4 M).
  넓어진 픽셀은 대부분 **이미 호스트로 이식된 블리터**가 처리하기 때문이다.
  아직 이식 안 된 전화면 루틴(`BoxBlur565/666`)은 30% 더 들 것이다 — 메뉴 열
  때 나타난다면 §20.4 순서를 앞당길 근거가 된다.
* **S25 게임플레이 fps: 36.6~39.6** (400 캔버스에서 38.3~39.0). 차이 없음.
  present 비용도 그대로(10초당 1.1~1.2초).
* **오프라인 4장면 905 프레임 A/B: 전부 바이트 동일.** 프로브 기본값은 여전히
  400×240 이고 이 변경은 셸이 게스트에게 말하는 숫자만 바꾼다.

> **넓힌 캔버스에서는 "바이트 동일"이 인수 기준이 될 수 없다** — 정의상 다른
> 그림이다. 대신 (1) 400×240 A/B 가 그대로 통과할 것, (2) 넓힌 폭에서
> `--no-patch` 와 자기일관적일 것, 두 가지를 쓴다. 프로브에
> `--screen WxH` / `--wipi-width n` 을 넣어 폰의 기하를 헤드리스로 재현할 수
> 있게 했다.

### 27.7 함정 하나 — 첫 서피스 크기가 영구적이다

`nativeInitDisplay` 는 **한 번만** 불리고 게스트는 거기서 들은 크기로
프레임버퍼를 잡는다. 그래서 캔버스 폭도 그때 정해지면 프로세스 수명 내내
바뀌지 않는다. `resumed` 가 아직 확정되지 않은 창 크기를 줄 수 있으므로
`App::boot` 이 `w == 0 || h == 0` 이면 물러나고 다음 `Resized` 를 기다린다
(`boot` 은 원래 멱등이다).

같은 이유로 `widened_canvas` 의 하한은 `WIPI_W`(400)이지 240 이 아니다.
세로 방향 크기가 먼저 도착하면 `floor(240×1080/2340) = 110` 이 되고, 240 으로
클램프하면 **원작보다 좁은 캔버스**가 되어 UI 가 잘린다.

### 27.8 바이너리 전수 조사 — 왜 이것이 안전한가

§27.2 의 실측(440/480/520 세 폭)은 "돌아간다"를 보였을 뿐이고, 내가 안 가 본
화면에 하드코딩된 400 이 있는지는 답하지 못한다. 그래서 `.text` 를 전수로
훑었다. 결과는 실측보다 강하다.

**1. `screen_w/h`·여백·배율은 그리기 코드가 읽지 않는다.**
`nativeInitDisplay`(`0x14ea58`)는 인자를 세 곳으로 흩는다:

```
setTargetDisplayInfo(screenW, screenH, r,g,b,a, mag, magFilter, viewMode, mx, my)  0x151610
setWIPIDisplayInfo (wipiW, wipiH, colorType)                                       0x152220
setSubLayerInfo    (useSublayer, sublayerPixel)                                    0x151660
```

`setTargetDisplayInfo` 가 쓴 필드(`kernel+0x624`, `+0x628`, `+0x63c`, `+0x644`,
`+0x648`, `+0x64c`)를 읽는 곳은 `createTextures`(`0x151fd4`)와 `init2DScreen`
**둘뿐**이다 — 즉 GL 표시용 쿼드. **화면 크기와 배율은 게임의 그리기에
관여하지 않는다.**

**2. 프레임버퍼는 `wipiW × wipiH` 에서 잡힌다** (`setWIPIDisplayInfo`):

```
pitch = (bpp/8) * wipiW ;  bytes = wipiH * pitch ;  MC_knlAlloc(bytes)
kernel+0x4e0 = {w=wipiW, h=wipiH, bpl, bpp, ptr}
```

넓은 캔버스 = **정확히 그만큼 큰 버퍼**. 넘칠 여지가 없다. 두 번째 오프스크린
버퍼도 `onStartApp+0x3f0`(`0x9db94`)이 같은 두 값으로 만든다.

**3. `Graphic::GetWidth()` 가 곧 `wipiW`다.** `ChangeFrameBuff`(`0x5fbc0`)가
`FrameBufferInfo` 를 거쳐 `this+0x54`(width) / `+0x58`(height) /
`+0x4c`(halfWidth) / `+0x50`(halfHeight) 를 채운다.

**4. 하드코딩된 400 은 좌표가 아니다.** `.text` 전체에서 `0x190` 은 **57곳**,
전부 구조체 필드 오프셋(`ldrh r1,[r3,r2]` 꼴), `operator new(400)`,
드롭아이템 400칸 테이블(`cmp r4,#0x190`), 윤년 계산이다 —
**x 좌표나 폭으로 쓰인 곳은 0곳.** `0xf0`(240)은 74곳인데 화면과 관계있는 것은
`bl GetWidth` **직후의** `cmp r0,#0xf0` 열 곳뿐, 즉 240 은 *보고된 폭을 시험하는
값*이지 폭 자체가 아니다. 스캔라인 크기 상수(96000/192000/800)는 0곳.

**5. 가운데 정렬은 `GetHalfWidth` 로 한다.** 호출 수: `GetWidth` **314곳
(서로 다른 함수 95개)**, `GetHeight` 310, `GetHalfWidth` 209,
`GetHalfHeight` 140. 대표적인 모양:

```
CommonUi::DrawTouchCancelUi   x = GetWidth()
StateMainMenu::DrawLogo       x = GetWidth() - 2 - GbmImage::GetWidth()
StateGame::SetTouchKeypad     모든 키패드 사각형이 GetWidth() - table[i]
Map::MapSetData               camX = clamp(cx - GetHalfWidth(), 0, mapW-1-GetWidth())
```

**6. 게스트에는 이미 해상도 사다리가 있다.** 폭을 상수와 비교하는 곳 전부:
`cmp GetWidth(), #240` **10곳**, `#320` **2곳**, `#480` **2곳**. 즉 이 코드는
처음부터 여러 폭을 상정하고 쓰였다. 400 과 520 은 240·320 분기의 같은 쪽에
떨어지므로 레이아웃 경로도 바뀌지 않는다.

### 27.9 조사가 지목한 회귀 하나 — 재현되지 않았다

전수 조사는 회귀를 **하나** 지목했다: 월드맵 아트가 480×320 이고
`StateMap::SetMapMode`(`0x1278f8`) / `StateInGameMenu::SetMapMode`(`0xe8bf8`)가
`GetWidth() >= 480` 에서 스크롤을 그만두고 가운데 정렬로 바꾸므로, 520 에서는
좌우 20 논리픽셀(기기 90px)이 안 칠해진다는 것.

**직접 확인했더니 재현되지 않았다.** 520 캔버스에서 `map` 버튼으로 연 화면과
모드 토글 세 상태(월드맵/지역맵/미니맵 off), 총 네 장을 픽셀로 재 보니
**안 칠해진 열이 한 곳도 없다**(임계값 3열 이상 연속). 네 장은 서로 확실히 다른
화면이다(픽셀 차분 3천만 이상). 월드맵은 좌우 끝까지 그림이 차 있다.

분기 자체는 실재하므로, 아직 못 가 본 어떤 맵/줌 상태에서 드러날 여지는
남는다. 다만 **이 폭에서 실제로 열리는 맵 화면들은 깨끗하다.**

### 27.10 아직 확인 안 된 것

* **캔버스보다 좁은 맵.** `Map::MapSetData`(`0xaf04c`)는 `camX` 를
  `mapW - 1 - GetWidth()` 로 클램프한 뒤 음수를 0으로 자르므로
  (`bic r8, r8, r8 asr #31` @`0xaf0e0`), 폭이 400~519 인 맵이 있으면 520 에서
  처음으로 노출된다. 맵 테이블을 전수로 보지는 않았다 — 작은 실내/상점을
  도는 플레이 세션이 필요하다.
* **재생 스크립트가 캔버스를 모른다.** `tools_re/scripts/*` 와 `docs/reference/`
  정답지는 전부 400 폭 좌표로 쓰여 있다. 520 에서는 가운데 정렬되는 탭이
  +60 을 필요로 하고 가장자리 고정은 그대로다. 그래서 **520 빌드는 기존
  도구로 회귀 시험이 안 되고**, `ref_compare.py` 는 520 에서 의미가 없다.
  400 기본값 A/B 가 그대로 통과하는 것이 지금의 안전망이다.
* **폭 상한 1024.** `Texture2D::getWidth2n`(`0x152af0`)이 2^0..2^10 만 훑고
  그 위는 0을 돌려주며 `createTexture`(`0x15331c`)가 0에서 포기한다.
  4.27:1 보다 넓은 화면은 없지만, 상한은 주석이 아니라 코드에 있어야 한다 —
  `widened_canvas` 가 `clamp(WIPI_W, 1024)` 로 갖고 있다.

## 28. 인게임 상단 바를 가운데로 — 그리고 찾을 수 없던 히트테스트 (2026-08-19)

§27.3 이 남겨 둔 흠 하나: 인게임 `map / shop / save` 바가 고정 논리좌표라
넓은 캔버스에서 중앙보다 47픽셀 왼쪽에 앉는다. 사용자 요청으로 후킹했다.

### 28.1 바는 한 루프에서 그려진다

`StateGame::DrawTouchUi` (`0xe05e0`) 안에 셀 세 개짜리 루프가 있다:

```
r6 = 0x93                                              ; 147 — 리터럴 기준점
loop (0xe0cfc):
  CommonUi::DrawInfoRect(fb, r6,      0, 0x1e, 0x0d)   ; 판(밑줄)
  TextMgr::GetCommonText(r7)                           ; r7 = 0x112/0x113/0x114
  BFont::DrawText       (fb, r6+0x0f, 0, str, 1, 0xffffff)
  r6 += 0x32                                           ; 셀 간격 50
```

그래서 x 는 `147 / 197 / 247`(판), 글자는 `+15` 해서 `162 / 212 / 262`.
**두 호출 모두 x 가 인자 2번**이라 같은 `dx` 를 더하면 바 전체가 움직인다.
복귀주소는 `DrawTouchUi+0x750`(판)과 `+0x7a4`(글자).

찾는 데 시간이 걸린 이유: 좌표가 `.text` 어디에도 **즉치로 없다**
(162/212/262 를 전수로 훑어도 이 함수엔 안 나온다 — 루프가 만들어 낸다).
`--watch _ZN5BFont8DrawTextEiiiPcii` 가 `"map"`/`"shop"`/`"save"` 를 인자로
찍어 주면서 한 번에 나왔다. **경계면 트레이스가 아니라 게스트 함수 후킹이
답이었다.**

### 28.2 히트테스트는 못 찾았다 — 그래서 다르게 풀었다

픽셀을 옮기는 것은 절반이다. 게스트의 히트테스트는 여전히 **옛 사각형**에
답하고, 보이는데 눌리지 않는 버튼은 치우친 버튼보다 나쁘다.

그런데 그 히트테스트가 어디에도 없었다:

* 바 탭은 **WIPI 소프트키가 된다** — `Battle::KeyPressed` 가 -7 / -10 을 본다
  (`StateGame::KeyPressed+0x78` 에서 넘어온다).
* `StateGame::KeyPressed` 는 **직접 호출자가 없다**(vtable 로만 불린다).
* `StateGame::TouchPressed` 의 `Check_Rect` 두 호출은 실측하니
  `(0, 30, 190, 210)` 과 `(0, 30, 320, 130)` — 이동 영역이지 바가 아니다.
* `SetTouchKeypad` 가 채우는 사각형 9개는 전부 `GetWidth/GetHeight` 파생 —
  가장자리 고정 게임패드다.
* 162/212/262 즉치는 `.text` 전체에 없다.

계속 파는 대신 **그리기 후크가 이미 아는 사실**을 썼다: 지금 화면에서 바가
실제로 어디 있는가. 옮긴 셀마다 그 사각형을 기록하고,
`hud::Hud::unshift` 가 그 띠 안에 들어온 터치만 같은 `dx` 만큼 되돌린 뒤
게스트에 넘긴다. 게스트는 자기가 믿는 좌표로 자기 히트테스트를 그대로 한다.

**좌표 범위를 박아 두는 것과 다른 점**: 기록은 **바를 실제로 그린 프레임에만**
존재한다(`Vm::frame` 머리에서 지운다). 상단에 다른 것이 있는 화면에서는 띠가
아예 없으므로 탭을 훔치지 않는다.

`dx = (wipi_w - 400) / 2` 다. 400 에서 **0** 이므로 원작 폭에서는 코드 경로가
없고, 그래서 905프레임 A/B 가 바이트 동일로 남는다. 520 에서 60 이고, 바는
원래 캔버스 중앙에 대해 갖고 있던 관계를 그대로 유지한다(400 에서 중앙 +13,
520 에서 중앙 +12.5).

### 28.3 후크 두 개가 같은 주소를 원했다

`BFont::DrawText(iiiPcii)`(`0x445e8`)는 **캐시샵 가격 줄**(§25.4)과 **이 바**가
둘 다 쓴다. `Vm::hooks` 는 주소당 함수 하나이므로 `Vm::boot` 이 그 주소를
소유하고 두 후크에 차례로 넘기는 디스패처를 둔다. 둘 다 복귀주소를 먼저 보고
자기 자리가 아니면 사양하므로 **순서는 무관**하고, 둘 중 하나가 `Vm::call` 로
다시 부른 것은 `lr == RETURN_MAGIC` 이라 양쪽 모두 사양한다 — 그것이 재귀를
막는다.

`Vm::install_hook` 이 공용 설치기다. `install_patches` 와 같은 검사(이미지
안의 ARM 워드인가)를 하고, **같은 주소에 두 번째 후크를 넣으면 에러**다.
실제로 이 작업 중에 그 에러가 한 번 났고(hud 와 shop 이 둘 다 넣으려 했다),
조용한 덮어쓰기 대신 즉시 드러났다.

### 28.4 검증

* 유닛 4개(`hud::tests`): 원작 폭에서 dx 0 · 520 에서 dx 60 · 띠 안의 탭만
  옮겨진다(같은 열의 아래, 같은 행의 왼쪽은 그대로) · 바를 안 그린 프레임은
  탭을 훔치지 않는다.
* **프로브 520 재생**: 바가 `213..332`(중앙 272.5)로 옮겨졌다 — 후크 전
  `153..273`(중앙 213). 정확히 +60. 그리고 **옮겨진 자리를 탭해서**
  save(321)·shop(271)이 동작해 캐시샵이 열렸다(가격 940줄 재도색).
* **오프라인 4장면 905 프레임 A/B: 전부 바이트 동일.** dx 0 에서 후크가 진짜
  아무 것도 하지 않는다는 증거다.
* **S25 실기기**(versionCode 12): 바가 가운데로 왔고,
  `guest 271 -> screen 1220` 탭이 게스트의 211 로 되돌아가 캐시샵이 열렸다.
  같은 흐름에서 부활의 부적 지급도 그대로 돌았다
  (`localserver: granting 1 record(s) for item=26`).

## 29. 인트로의 present 비용 — 스킵이 한 번도 안 걸리고 있었다 (2026-08-19)

§26.3 이 남긴 항목: 인트로는 게스트가 1.2초만 쓰고 present 가 4.8초를 쓴다.
한 번도 프로파일된 적이 없는 구간이라 먼저 **쪼개서 쟀다.**

### 29.1 계측부터 — 총합은 어느 쪽을 칠지 말해 주지 않는다

로그 한 줄에 셋을 더했다: **틱/초**, **draws/초**, 그리고 present 를
`blit`(우리 스케일링 복사)과 `swap`(`buffer_mut` + `present`, 즉 컴포지터에
넘기고 vblank 를 기다리는 시간)으로 나눈 값.

첫 측정이 §23.2 를 뒤집었다:

```
57 ticks/s, 57 draws/s, 100% presented;  blit 2255 ms, swap 2458 ms per 10 s
```

* **`gl.draws` 는 "그림이 바뀌었나"의 대리값이 아니다.** 게스트는
  **매 틱마다** `glDrawArrays` 를 낸다 — 그래서 draws/s == ticks/s 이고
  §23.2 의 present 스킵은 **한 번도 걸린 적이 없다.** 게스트의 *타이머*가
  22 fps 로 묶여 있는 것이지 재블릿이 묶여 있는 게 아니었다. CLAUDE.md 의
  "나머지는 비트 동일 재칠이었다"는 맞는 관찰이었는데, 그 재칠을 **거르지
  못하고 있었다.**
* present 4.7초는 절반이 진짜 일(blit 2.3초), 절반이 기다림(swap 2.5초)이다.
  복사를 아무리 싸게 만들어도 swap 은 줄지 않는다 — 줄이려면 **덜 present**
  해야 한다.

### 29.2 고친 것 둘

**1. 내용으로 스킵한다.** 마지막으로 화면에 올린 프레임버퍼를 들고 있다가
비교해서 같으면 present 자체를 건너뛴다. 520×240 워드 비교(500 KB)는
2,340×1,080 로 확대해 컴포지터에 넘기는 것(9.4 MB + vblank)에 비하면 공짜다.

**2. 행을 재사용한다.** 4.5배에서 원본 한 행이 목적지 4~5행을 먹인다. 1,080행
전부를 표 룩업으로 모으고 있었는데, 실제로 다른 행은 **240개**뿐이다. 나머지는
방금 만든 행의 `copy_from_slice` — 표 2,340번 대신 memcpy 한 번.

### 29.3 결과 (S25, 릴리스 빌드)

| 구간 | present ms/10s | blit | swap | 스킵 |
|---|---|---|---|---|
| 인트로 — 전 | 3777~4717 | 1803~2255 | 1974~2458 | 0 |
| 인트로 — 후 | **516~1104** | **116~255** | **375~824** | **403~430 / ~580** |
| 게임플레이 — 전 | 1177~1199 | — | — | 0 |
| 게임플레이 — 후 | **464~689** | 87~123 | 369~559 | 134~213 |

인트로 present **4.3배**, 게임플레이 **2.4배**. 빈 시간은 게스트가 가져갔다 —
게임플레이 게스트 처리량이 **38 → 40 M ins/s**, 게스트 시간 6.5 → 7.2 s/10s.

### 29.4 그리고 fps 숫자가 거짓말이었다

`stat_frames` 를 present *시도*마다 세고 있었다. 스킵이 생기니 "58 fps 인데
셋 중 둘을 버림"이라는 말이 안 되는 로그가 나왔다. **실제로 화면에 올라간
것만 세도록** 옮겼더니:

| 장면 | 옛 표기 | 실제 그림 갱신 |
|---|---|---|
| 인트로 | 56~58 fps | **8~18 fps** |
| 게임플레이 | 38~39 fps | **15~23 fps** |

인트로의 18 fps 는 게스트 프롤로그의 자기 틱(55 ms)과 정확히 맞는다(§16.6).
**즉 이 게임은 늘 그 속도였고, 우리가 같은 그림을 세 번씩 올리고 있었을
뿐이다.** 옛 숫자는 낮아질 수가 없는 숫자였다 — 무엇을 고쳐도 60 Hz 틱을
셌을 테니까.

> **0.0 fps 는 고장이 아니다.** 부활 대화상자가 떠 있으면 화면이 완전히
> 정지한다(2초 간격 스크린샷 두 장의 차분이 정확히 0). 게스트는 그동안에도
> 38 M ins/s 를 태우지만 그리는 그림은 같다. 스킵이 옳게 도는 모습이다.

## 30. 블리터 둘 더 — 그리고 S25 는 이제 게스트의 천장에 닿았다 (2026-08-19)

§20.4 의 순서는 400 캔버스에서, 그리고 일부 이식 **전에** 잰 것이라 더는
맞지 않는다. 출하 구성(520×240)에서 `ingame` 을 다시 프로파일한 것이 출발점:

```
16.6%  _midas_funcFntDrawString          ┐
 6.1%  _midas_funcFntDrawPixelPreRGB     ├ 폰트 묶음 27.8%
 2.4%  _midas_funcFntDrawPixel           │
 2.7%  MC_grpGetRGBFromPixel             ┘
14.5%  DrawImageStretchClip565  (+2.1% R 변종)
12.9%  ChangeFrameBufferBlending565
11.3%  DrawImageBlendingClip565          ← 이미 이식됨, 이건 사양된 4비트 경로
10.1%  DrawImageMirrorClip565
 7.1%  FillRectBlending
```

크기 대비 값이 가장 좋은 둘을 이식했다.

### 30.1 `ChangeFrameBufferBlending565` — 476바이트, 12.9%

맵 이펙트의 화면 전체 틴트. 내부 루프가 **하나뿐**이라
`DrawImageBlendingClip565` 와 달리 사양할 변종이 없다.

**디스패처가 아니라 565 심볼에 건다.** `Graphic::ChangeFrameBufferBlending`
(`0x54040`)은 `this+0x5c` 로 픽셀 포맷을 고르고 **꼬리 분기**(`b`, `0x54078`)
하므로 `lr` 은 원래 호출자 것이고 `sp` 도 그대로다 — `Vm::run` 의 "PC = lr"
복귀와 `arg(4) = [sp]` 이 둘 다 그대로 성립한다.

**함정: `blend_table565` 를 재사용하면 안 된다.** 그쪽은
`Graphic::RevisionAlpha565`(`0x536f0`)를 거쳐 가중치가 `(alpha>>3)+1` 인데
이 함수는 안 거친다(`0x53d30` 이 맨 `lsr r7,r7,#3`). **보수는 우연히 같다** —
`0x20-(a+1) == 0x1f-a` — 그래서 서로 바꿔 쓸 수 있는 것처럼 보인다. 실제로는
소스 가중치가 테이블 한 칸씩 어긋나고, 불투명 판정도 다르다(`a > 0x1e` 대
`ar > 0x1f`). 화면으로는 안 보이고 905프레임 A/B 에는 치명적이다.

빠른 형태: 출력 채널이 각각 대응하는 입력 채널에만 의존하고 나머지는 전부
루프 불변이라, 픽셀마다의 곱셈 사슬이 **호출당 한 번 만드는 32칸 표 세 개**로
접힌다. 안쪽 루프는 룩업 셋 + OR 둘.

행 간격은 `bpl` 이 **아니라** `width` 다(`pixoff += width`, `0x53e40`).
`GetBpl` 은 부르지도 않는다 — 이 캔버스에서는 결과가 같아서, 스트라이드를
쓰는 "개선"이 리뷰를 통과해 버릴 수 있는 자리다.

### 30.2 `FillRectBlending` — 916바이트, 8.2%

기하는 이미 이식된 `FillRectAlpha` 의 쌍둥이(같은 정규화·클램프·원점·
`this[0x5c]` 분기)지만 **픽셀 산술은 전혀 다른 함수**다. 산술 블렌드가 없고
모든 출력 성분이 블렌딩 테이블에서 나온다.

함정 둘:

* **인자 순서가 `level` 다음 `alpha`**(6번, 7번)다. `FillRectAlpha` 는 6번이
  `alpha` 뿐이었다. 바꿔 써도 타입이 맞고 그럴듯한 그림이 나온다.
* 여기서도 `blend_table565` 는 못 쓴다(§30.1 과 같은 이유). `alpha = 0xff` —
  아홉 호출처 중 여덟이 넘기는 값 — 에서 이 함수는 불투명 분기로 가
  테이블 바이트를 그대로 내보내는데, 저 헬퍼는 섞는다.

여기서는 행 간격이 **`bpl` 이 맞다**(`0x52800`). 두 함수가 반대라는 점이
그대로 함정이다.

### 30.3 결과

| | 게스트 명령어 (ingame, 520) | 프로파일에서 |
|---|---|---|
| 이식 전 | 3,094,964,637 | — |
| `ChangeFrameBufferBlending565` 후 | 2,695,349,900 (**−12.9%**) | 사라짐 |
| `FillRectBlending` 후 | **2,466,728,288** (**누적 −20.3%**) | 사라짐 |

두 이식 모두:

* **사양 0회.** `patch_declined` 가 28,759 로 **변하지 않았다** — 두 함수 다
  내부 루프가 하나뿐이라는 명세의 예측대로다. 이 숫자가 움직였다면 가정이
  틀린 것이고, 고칠 게 아니라 다시 유도해야 한다.
* **오프라인 4장면 905 프레임 A/B 전부 바이트 동일.**
* 프로파일 점유율이 정확히 명세가 말한 만큼 사라졌다.

### 30.4 S25 는 이제 게스트의 천장에 닿았다 — 여기서 멈추는 이유

기기 실측(움직이는 게임플레이, 실제 그림 갱신):

| | 이식 전 | 이식 후 |
|---|---|---|
| 게임플레이 | 14.7~22.6 fps | **21.5~23.1 fps** |
| 틱 | 36/s | 37~38/s |
| 게스트 시간 | 7.1~7.6 s/10s | 6.9~7.0 s/10s |

**그리고 이것이 천장이다.** 게스트의 fps 표(`0x1586b0` = {10,11,12,13,15,16,
17,18,20,22})가 말하듯 이 게임은 **어떤 설정에서도 22 fps 를 넘겨 그리지
않는다**(§16.6). 21.5~23.1 은 그 상한 그 자체다.

> **그러므로 S25 에서 블리터를 더 이식해도 fps 는 안 오른다.** 남은 것들이
> 사는 것은 여유다 — 발열·배터리, 그리고 **S9 처럼 느린 기기의 마진**.
> 다음 이식의 근거는 "S25 가 느리다"가 아니라 "S9 가 천장에 못 닿는다"여야
> 하고, 그 말을 하려면 §26.3 이 남긴 **S9 재측정**이 먼저다.

남은 순서(다시 프로파일한 것):

```
20.9%  _midas_funcFntDrawString      ← 폰트 묶음이 여전히 가장 크다
18.2%  DrawImageStretchClip565
14.2%  DrawImageBlendingClip565      ← 이미 이식된 함수의 사양된 4비트 경로
12.6%  DrawImageMirrorClip565
 7.6%  _midas_funcFntDrawPixelPreRGB
```

## 31. 다섯 갈래 조사 — 확인 모달·게임 속도·창고접속·오브/제련 (2026-08-20)

사용자가 한 번에 다섯 가지를 요구했다: ① 구매 확인 모달에도 취소선/0원 또는 문구
교체, ② 게임 속도가 전반적으로 느리다(메인메뉴·컷신·인게임), ③ 그 다음 발열
최적화, ④ 오브/제련이 전부 구현됐는지 심층 분석, ⑤ 창고접속 메뉴가 커넥팅 후
서버오류로 튕기는 것을 "이용할 수 없는 기능" 안내로 바꾸기.

**이 절은 조사 결과다. 구현은 아직 아무것도 들어가지 않았다** — 이 문서와
`CLAUDE.md` 외에 저장소는 손대지 않았다.

### 31.0 방법 — 그리고 왜 검증 단계가 값을 했는가

심층분석 7개를 병렬로 돌리고, **각각에 적대적 검증 에이전트를 붙여** 주장을
반박하게 했다. 검증은 앞 에이전트의 산출물을 읽지 않고 바이너리에서 다시 유도했다.

결과가 이 방식의 값어치를 말한다 — 검증 5건이 끝난 시점에 **주장 141개 중 41개가
반박되거나 실질적으로 틀렸다.** 대부분은 흠집이지만, **하나는 그대로 구현했으면
905프레임 A/B 를 터뜨릴 것이었다**(§31.7 의 니블 순서).

> **교훈:** 함수 *심볼*에 붙은 주소는 거의 다 맞았다. 틀린 것은 **함수 안쪽 맨
> 주소**(`0xDD894` 같은 것)와, 디스어셈블하지 않고 이름만 보고 한 주장이다.
> 이식 명세가 인용해야 할 것은 심볼+오프셋이지 절대 주소가 아니다.

### 31.1 캐시샵 구매 확인 모달 — 조립 지점은 정확히 둘

`TextMgr::GetCommonText(int)` 는 **멤버 함수라 인덱스가 r1** 이다(r0 이 `this`).
이걸 놓치면 전수 조사가 통째로 헛돈다. r1 을 상수전파해서 `bl 0x143170` 478곳을
전부 훑은 결과, `0x10a`("실제 현금") 또는 `0x10b`("원의;추가 정보이용료가
부과됩니다.")를 넘기는 곳은 **네 줄, 함수로는 둘**이다:

| 사이트 | 함수 | 인덱스 넘기는 곳 | 후크가 걸 복귀주소 |
|---|---|---|---|
| **A** | `StateCashShop::KeyPressed` (0xDD924) | +0x1F8 / +0x20C | **+0x27C** (0xDDBA0) |
| **B** | `StateGame::DrawOverText` (0xDECCC) | +0x34 / +0x44 | **+0xF8** (0xDEDC4) |

* **A** 가 사용자가 본 그 모달이다. 서식은 `.rodata` 0x159BD0 의 `"%s %d%s;%s;%s"`
  이고 `;` 이 줄바꿈, `^` 는 색 이스케이프다. 가격 500 에서 EUC-KR **130바이트**,
  여섯 줄로 렌더된다. `--watch` 실측:
  `StateCashShop::SetPopupState(…, 0x2, …) <- StateCashShop::KeyPressed+0x27c`.
* **B** 는 사망/부활 화면의 나그(`실제 현금 100원…`)이고 **가격이 즉치 100** 이다 —
  카탈로그 필드가 아니다. 서식은 0x159C78 의 `"%s;%s %d%s;%s;%s"`, 162바이트.

확정된 배제 셋:

* `StateCashShop::SetPopupState`(0xDBFC8)는 요금 문자열을 **전혀 안 만든다.**
  `GetCommonText` 호출이 0x32("예")·0x33("아니오")·0x38("OK") 버튼 라벨뿐이다.
  호출처 10곳 중 확인 모달은 0xDDB9C 하나이고, 0xDDD48(= `KeyPressed+0x428`)은
  가방 가득참 오류다 — **함수만 후킹하면 그것까지 갈아끼운다.**
* `StateCashShop::TouchPressed`(0xDD5F4)는 아무것도 조립하지 않는다. 탭을 WIPI
  키 −5/−16 으로 바꿔 `BaseState::KeyPress`(0x44CBC)에 넣고 `KeyPressed` 가 일한다.
  (`mvn r1,#4` 는 **0xDD890** 이다. 0xDD894 라고 적은 조사는 4바이트 어긋났다.)
* `[0x14c]`·`[0x14d]`(통화료별도 변형, 체험판 문구)는 **이 빌드에서 죽은
  문자열**이다. 상수 참조가 없고, 유일하게 도달할 뻔한 가변 인덱스 사이트
  `StateMainMenu::DrawLogo+0x518`(`mov r1,#0x148; add r1,r1,r8`)은 `r8 = 1` 이라
  0x149 로 간다.

**모달 안에서 취소선을 긋는 것은 비싸다.** 본문은 `StatePopup+0x24`(1,024바이트,
`PopupInit` 이 memset)로 memcpy 된 뒤 `StatePopup::Paint`(14 KB) 안에서
`DrawText(iiiiPci)` 로 **줄바꿈되며** 그려진다. 가격이 몇 번째 줄 어디에
떨어졌는지 재현해야 한다. 반면 **문장 교체는 인자 하나**다.

새 한글 바이트를 코드에 넣을 필요도 없다 — 게임 자신의 행으로 조립하면 된다:

```
[0x5b]  = 구입하시겠습니까?
[0x104] = 부활의 부적을 구매하시겠습니까?
[0x109] = 원            [0x10a] = 실제 현금
[0x14a] = (정보료/통화료 무료)
```

교체본 길이: A 135바이트, B 167바이트. **1,024바이트 팝업 영역에 여유롭게 들어간다**
(조사가 "sp+0x10 의 260바이트"를 한계로 든 것은 틀렸다 — `MC_knlSprintk` 는
크기를 안 받으므로 그 프레임은 애초에 한계가 아니고, 실제로 바인딩하는 것은
`popup+0x24` 다).

#### 곁다리로 찾은 기존 버그

`shop::compact_price_hook` 에 `Config::free_shop` 검사가 **없다.** 그리고
`draw_text_plain` 디스패처는 `free_shop || hud.dx != 0` 이면 설치된다
(`lib.rs:935`). 넓은 캔버스는 항상 켜져 있으므로, **`hl5_server.txt` 로 실서버를
가리킨 설치본이 진짜 유료 상점에도 취소선과 `0원`을 그린다.** 대칭을 위해
`detail_price_hook` 에도 같은 검사를 넣는다.

### 31.2 게임 속도 — 상한은 게스트 자신이고, 레버는 `ftime` 하나다

**사용자가 말한 "느림"은 렉이 아니라 게스트의 설계 프레임 상한이다.**

`ftime` 이 게스트의 **유일한** 벽시계 시간원이라는 것이 이번에 못박혔다:
정의되지 않은 dynsym 77개 중 시간 함수는 하나뿐이고, `ftime@plt`(0x31624)의
호출자는 `MIDASKernelManager::currentTime`(0x1522e4) **하나뿐**이며
`MC_knlCurrentTime` 43곳이 전부 거기로 모인다. JNI 진입점·`CletEntry` 콜백 중
타임스탬프를 나르는 것은 없다. 호스트 쪽에서 그것은
`Vm::now_ms`(`lib.rs:517`) → `hle.rs:542` 한 곳이다.

**따라서 `now_ms` 에 계수 S 를 곱하면 게스트가 보는 시간은 그대로이고
(틱 사이는 여전히 45 ms, 경과 델타도 그대로) 게스트시간↔실시간 사상만 바뀐다.**
그래서 **틱 구동 코드와 시계 구동 코드가 같은 배율로** 빨라진다.

이 성질이 다른 후보들을 죽인다:

| 방법 | 판정 |
|---|---|
| **(a) `now_ms` 배속** | **1순위.** 게스트 메모리 무수정, 단일 지점, 위상 동기 유지 |
| (b) fps 표(0x1586b0) 패치 | 3순위·대안. `ParticleMgr` 는 실 ms 를 적분하므로 **이펙트만 1배로 남는다** — 사용자가 말한 증상 그대로 |
| (c) `gv[0x86]` 상향 | **사망.** `CommonUi::InitGameOption+0x4c` 가 유일한 기록자이고 값이 이미 9 = 10항 표의 마지막 |
| (d) `MC_knlSetTimer` 후킹 | 4순위. (b)와 같은 위상 깨짐 + `Vm::patches` 가 표현 못 하는 인자 재작성 |
| (e) `nativeLoop` 더 자주 | **단독으로는 0.** 다만 (a)의 **필수 동반자** — 아래 참조 |

> **재검증 중인 주장 하나 (중요)**: 조사는 `timerLoop`(0x152318)의 10 ms 조기
> 발화 때문에 **게스트의 설계 틱은 45 가 아니라 36 ms = 27.8 틱/s** 이고,
> 따라서 §30.4 의 "S25 는 게스트의 22 fps 천장에 닿았다"가 **틀렸으며** 실제로는
> CPU 바운드로 천장의 ~80% 에 있다고 주장한다. 사실이면 게임플레이에 아직 26%
> 여유가 있다는 뜻이고, 이 프로젝트의 "여기서 멈추는 이유"가 무효가 된다.
> 같은 조사가 **셸의 60 Hz 폴 격자가 36 ms 마감을 50 ms 로 반올림해** 게스트를
> 20 틱/s 에 묶고 있다고도 주장한다(`nativeLoop` 한 번에 게임 틱 최대 하나).
> **둘 다 §31.9 에서 독립 재현으로 확정됐다** — 설계 틱 36.0 ms 실측,
> 폴 격자가 곧 틱 상한(dt=16 이면 137틱, dt≤4 면 183틱). §30.4 는 틀렸다.
> **다만 §31.9 는 이 절의 처방도 셋 바꿨다(격자 4 ms → 12 ms,
> `Hud::span` 동반 수정, `speed` 기본값 1.5 → 1.0). 구현 전에 §31.9 를 읽어라.**

#### S 의 정직한 천장

측정된 틱당 게스트 명령어: 인트로 210 K, 컷신 833 K, 메인메뉴 1.09 M, 정지
게임플레이 1.21 M. S25 의 17.2 ns/명령어로 환산하면 3.6 / 14.3 / 18.8 / 20.8 ms
이고, 움직이는 게임플레이는 기기 실측 31.1 ms 다. 36 ms 예산에 대고 보면:

| 장면 | 실제로 나오는 배속 |
|---|---|
| 인트로 | ~4.0 |
| 컷신·대사 | ~2.5 |
| 메인메뉴 | ~1.9 |
| **전투** | **~1.16** |

**전투는 어떤 S 로도 안 빨라진다.** 전투의 해답은 시계가 아니라 §31.4 의 이식이다.
그리고 S > 1.15 는 전투를 5 ms 바닥 경로에 올려 **코어 하나를 100% 로 고정한다** —
③ 발열과 정면으로 부딪히므로 기본값은 보수적이어야 한다.

배속이 실제로 바꾸는 것들(부작용이 아니라 결과):
`HERO::SetExpGoodTime`(0x8d7cc)의 시간제 버프가 실시간으로 S배 빨리 탄다,
`SlotInfo::SaveSlotData`(0x8c8bc)의 플레이타임이 S배로 부푼다(세이브에 기록됨),
네트워크 마감(60 s 워치독 / 181 s 유휴 / 5.9 s 접속 한도)이 1/S 로 줄어든다,
BGM·효과음은 실시간으로 재생되므로 장면과 어긋난다.

#### ~~헤드리스로 증명 가능한 인수 기준~~ — 실행 불가능하다 (§31.9)

여기 적었던 것: 프로브는 가상시계라 `--speed S` + `dt` 는 `--speed 1` + `dt×S`
와 정의상 동일하므로 두 실행의 덤프가 바이트 동일이어야 한다.

**검증이 셋을 깼다.** ⑴ 산술이 틀렸다(제안대로 `Vm::frame` 이 전진을 스케일하면
`speed 2 / dt 66` 은 132 ms 를 전진해 4배 어긋난다. 동등한 짝은 `(s,d)` 대
`(1, s·d)` 이고 `speed 2` 의 짝은 `dt 16.5` 인데 `dt` 는 `i64` 다). ⑵ 프로브에
dt 노브가 아예 없다(`const FRAME_MS: i64 = 33`). ⑶ **결정적으로**, 스케일은
`virtual_clock == false` 분기에 사는데 프로브는 언제나 `true` 다 — **프로브
시험은 출하되는 줄을 절대 실행하지 못한다.**

**따라서 이 기능에는 호스트 쪽 인수 기준이 존재하지 않는다.** 유일한 정직한
증거는 기기 10초 통계 줄에서 **틱 레이트가 실제로 `27.8 × speed` 에 닿는지**
보는 것이다. 곱셈이 일어난 것만 확인하는 시험은 아무 것도 증명하지 않는다.

### 31.3 present 는 더 이상 병목이 아니다 — 대신 설명 안 되는 것이 하나 생겼다

S25 설치본(versionCode 17, HEAD 빌드)에서 10초 창 26개를 읽었다.

* **present 는 벽시계의 4~10%** 다(타이틀 7.4~7.7%, 인트로 4.5~10.6%, 정적
  메인메뉴 0.16~0.18%). **§26.3 의 "인트로는 present 바운드"는 §29 이후 더는
  성립하지 않는다.** 반면 `vm.frame()` 은 어디서나 **62~70%** 다.
* `swap` 은 vblank 대기가 **아니다.** 디스플레이가 120 Hz 인데 앱은 초당 23회
  이하로 present 하므로 BufferQueue 가 찰 일이 없다. 실제 정체는 벤더 포크의
  softbuffer 가 present 마다 **2340×1080 스테이징 버퍼를 픽셀당 32비트 스토어로
  전부 스위즐**하는 것이다 — 124,800픽셀짜리 그림을 내보내려고 프레임당 ~30 MB 를
  CPU 로 민다(**20.25배 증폭**).
* `self.last_fb == fb` 비교는 틱당 24~45 µs, 벽시계의 0.13% — **혼자서는 손댈
  가치가 없다.**

> **설명되지 않는 것**: **정적 메인 메뉴가 화면을 한 픽셀도 안 바꾸면서
> `vm.frame()` 에 벽시계의 46~68% 를 태운다.** present 는 0이다. 이것이 present
> 최적화(최대 7.5%)보다 **큰 숫자**인데 어디로 가는 돈인지 아무도 모른다.
> 조사는 "게스트 재칠 ~22회/s 가 청구서"라고 했지만 검증이 그 추론을 깼다 —
> 그 데이터로는 빈 틱과 재칠을 구분할 수 없다.

**따라서 다음 세션의 첫 작업은 계측 셋이다** (각 3줄, 위험 0):
`t_acquire`(락 시간을 swap 에서 분리 — 값은 이미 `lib.rs:527` 에서 계산되는데
`t_swap` 에 묻힌다), `t_gl`(`gl_draw` 소요), **게스트 재칠 카운터**
(`MC_grpFlushLcd` 0x14d898 진입 수 — 프레임당 경로는 `TimerCb` →
`CUSTOM_grpFlushLcd`(0x3fcb8), 0x9df1c/0x9e018). 이 셋이 없으면 메뉴의 6.5초가
`gl_draw` 인지, 재칠의 게스트 해석인지, 빈 틱의 게스트 해석인지 갈리지 않고
**셋은 서로 다른 처방을 요구한다.**

present 자체를 줄이는 선택지는 순서대로: (a) softbuffer `resize()` 로 버퍼를
캔버스 크기로 잡고 SurfaceFlinger 가 하드웨어로 확대(20 줄, 다만 **필터가
bilinear 라 글자가 흐려질 수 있다** — 기기 스크린샷으로 판정), (b) 셸에 EGL +
GLES2 프레젠터를 넣고 `GL_NEAREST` 쿼드 하나(원본이 하던 방식,
`hl5_core/src/gl.rs` 와는 완전히 무관 — 그쪽은 GL 라이브러리를 링크하지 않는다),
(c) 더티 렉트는 **하지 않는다**(같은 7.5% 상한에 위험만 크다).
그리고 (a)를 하더라도 `sw == fw && sh == fh` 일 때 `copy_from_slice` 로 가는
**명시적 빠른 경로를 넣어야 한다** — 지금 코드는 그 경우 `step_y = 65536` 이라
행 재사용 분기가 죽고 픽셀마다 인덱스 조회를 돈다.

### 31.4 장면별 프로파일 — 컷신이 게임에서 가장 비싼 장면이다

전체 리플레이 프로파일은 다섯 장면이 섞여서 **이식 순서를 정하는 데 못 쓴다.**
서로 엄격한 접두사인 짧은/긴 스크립트 쌍을 만들어 샘플 수를 뺐다(차분법).
그리고 **매번 프레임을 덤프해서 어느 화면인지 눈으로 확인했다** — 첫 두 시도가
엉뚱한 화면을 쟀다(520 캔버스에서 `tap 200 200` 은 프롤로그에 떨어지고,
`ingame.txt` 의 `tap 240 200` 은 퀘스트 팝업의 OK 를 빗나간다. OK 는 x≈315).

| 장면 | 프레임당 게스트 명령어 | 지배 원인 |
|---|---|---|
| 인트로/프롤로그 | 99.8 K | Blending + Mirror (절대량이 작다) |
| 타이틀 | 265.9 K | Blending 사양 경로 ≈42% |
| **메인 메뉴** | 261.9 K | **폰트 스택 79%** |
| 게임플레이 HUD | 604.9 K | **Blending 사양 4비트 경로 62.7%** |
| **컷신** | **1.50 M** | **`DrawImageStretchClip565`(+R) 76%** |

**컷신은 게임플레이의 2.5배이고 이 게임에서 가장 비싼 장면인데 한 번도 측정된 적이
없었다.** 사용자의 "컷신도 느리다"가 여기서 설명된다.

**사용자가 지목한 세 곳이 정확히 세 가지 이식에 대응한다:**

1. **게임플레이** ← `DrawImageBlendingClip565` 의 **이미 사양 중인 4비트 루프**
   (`patch.rs:638-640` 의 `color_bit & 0xf == 4` 리턴). 게임플레이 −62.7%,
   타이틀 −42%, 인트로 −59%, 메뉴 −11%. **새 엔트리가 아니라 기존 이식의 사양
   해제**라 앵커/클립/팔레트/블렌드표가 전부 이미 있다.
   * 앞으로 이식할 루프: 정방향 `xform::NORMAL` 0x5d1d8..0x5d460 (~624 B).
   * **니블 순서 주의** — §31.7. 상위 니블 → `dst+0`, 하위 니블 → `dst+2`.
   * 행 길이는 `w/2`(`asr fp,fp,#1` @0x5d1dc). 팔레트 조회 뒤는 기존 8비트 경로와
     바이트 동일.
   * 4비트 디스패처는 0x5cca0: `#0x10 → 0x5d160`(= 0x5d1d8 루프),
     `#8 → 0x5defc`, `#4 → 0x5e460`, `#0xc(ROT180) → 0x5cd00`.
     **처음에는 `xform::NORMAL` 하나만 옮기고 나머지는 계속 사양한다.**
   * 인수: `patch_declined` 가 ingame 리플레이에서 28,759 로부터 **크게 떨어져야
     한다**(§30 의 두 이식과 달리 이번엔 움직이는 게 정상이다).
2. **컷신** ← `DrawImageStretchClip565`(0x5a188, 1,500 B) + 쌍둥이
   `…565R`(0x584a0, 1,464 B). 컷신 −65.6%(R 포함 −75.8%), 타이틀 −16.5%.
   게임플레이엔 0.
   * 프롤로그는 기존 이식과 같은 모양(`FrameBufferInfo` 조립 →
     `Graphic::RevisionAlpha565`(0x536f0) → `Graphic::CalcAnchor`(0x52fe4)를
     게스트에서 호출).
   * 옮길 것은 행 루프 0x5a3bc..0x5a6d0 과 열 본체 0x5a430..0x5a644 (~704 B).
     10.10 고정소수 소스 X, 팔레트 조회, `SKIP_COLOR` 검사(0x5a460), 클립 4비교
     (`this+0x40e0/e4/e8/ec`), `this+0x6d` 의 32×32 표 블렌드.
     **여기서는 `blend_table565` 가 맞다**(`RevisionAlpha565` 를 거치므로 —
     §30.1 의 함정과 반대다).
   * `level == 0x10` 은 사양한다(게이트 0x5a288-0x5a294, 분기 대상 0x5a744 가
     `Graphic::ChangeGrayPal`(0x582f4)로 샌다). 완전 이식은 0x5a668..0x5a694 와
     0x5a6fc 도 포함해야 하고, 아니면 `colourbit == 4` 와 `r4 == 4` 를 **명시적으로
     사양**해야 한다.
3. **메인메뉴** ← `_midas_funcFntDrawPixelPreRGB`(0x39798, 704 B) +
   `_midas_funcFntDrawPixel`(0x39a58, 100 B), 합쳐 ~800바이트. 메뉴 −49.6%.
   * **값의 절반은 산술이 아니라 트램폴린이다** — PreRGB 가 픽셀을
     `memcpy@plt`(0x39880)로 쓴다. 폰트 픽셀마다 HLE 왕복이 한 번(메뉴 636회/프레임,
     게임플레이 1,712회/프레임).
   * **함정 1**: PreRGB 는 순수 함수가 아니다. `_midas_rgb_r0/g0/b0` 에
     **되쓴다**(0x399bc/0x399d0/0x399d4). 이식이 그 스토어를 재현해야 한다.
   * **함정 2**: `FntDrawPixel` 은 globals 를 로드하는 게 아니라
     `MC_grpGetRGBFromPixel(arg2, &r0, &g0, &b0)` 로 **픽셀을 분해**한 뒤 PreRGB 로
     꼬리호출한다. 로드만 옮기면 묵은 색으로 그린다.
   * `_midas_lcdRGB == 2`(RGB565) 분기만 옮기고 나머지는 사양.
     `MC_grpGetRGBFromPixel`(0x1496a4)/`MC_grpGetPixelFromRGB`(0x14972c)의
     `kernel[0x4f4] == 0x10` 분기는 게스트 재진입 대신 인라인한다.

그 다음은 `_midas_funcFntDrawString` 의 글리프 비트 루프
(0x3b898..0x3b9d8 = +0x1140..+0x1280, 메뉴 `FntDrawString` 비용의 **82.4%**;
91% 는 +0x0cc0..+0x0d80 군집까지 더해야 나온다. 5,500바이트 전체를 옮기면
EUC-KR 선두바이트·줄바꿈 폭·정렬을 전부 떠안게 되므로 **비트 스캔만 후킹하는
좁은 안**을 먼저 견적낸다)와 `DrawImageMirrorClip565`(0x56e18; 실제 루프 본체는
~40 B(8비트 소스, 타이틀이 쓰는 쪽)와 ~68 B(4비트 소스). 인트로 −38.8%,
타이틀 −22.9%, 게임플레이 −9.3%).

**하지 않을 것**: `Graphic::ChangeBlendingPal` — 차분법으로 게임플레이 비용이
**정확히 0** 이다(47,155 − 47,155). 전체 리플레이 점유율 2.0~2.4% 는 접두사
구간의 일회성 작업이었다. `DrawImageRotateEx5Clip565` 는 진짜지만 3.59% 다.

부가: **부팅마다 `desRound` 가 25.8 M 명령어**를 먹는다(`StaticUtil::LoadResDecrypt`
경유 리소스 복호화, S25 에서 ~0.7초). 프레임 항목이 아니라 부팅 항목이고,
CLAUDE.md 의 "교과서 DES 는 세이브를 깬다" 경고가 그대로 적용된다.

**인터프리터는 손대지 않는다.** 디코드는 이미 4096항 LUT(`arm.rs:125-168`)이고
`GuestMem` 은 단일 경계검사(`mem.rs:174-252`)다. 레지스터 뱅크 우회로를
실측했더니 **잡음 바닥 안**이었다(같은 트리 4회 반복이 18.81~19.86 s = 5.6% 산포).
"3~7% 더 느리다"는 n=2 짜리 결론이라 못 쓰고, 정확히는 **유의미한 차이 없음**.
남은 구조적 레버는 디코드 블록 캐시뿐이고 별개 프로젝트다.

> **벤치 규율**: 프로브 처리량이 부하에 따라 46~54 M ins/s 로 흔들린다.
> 두 빌드를 비교하는 측정은 **반드시 교차 실행**해야 한다.

### 31.5 창고접속 — 커넥트 타임아웃이 아니라 `1/3` 무응답이다

증상은 "커넥팅이 오래 뜨다가 서버오류 후 메인메뉴"였고, 원인은 접속 실패가 아니다.
오프라인 프로브로 재현한 실제 순서:

```
netConnect(2) → 인프로세스 서버가 1/1 답함 → 1/5 답함
              → 게스트가 카테고리 1 서브커맨드 3 (별명 조회) 송신
              → localserver: "1/3 — no handler"
              → 마지막 무응답 송신으로부터 5,912 ms 후
                NETWORK::CheckServerConnectLimitTime → setError(302)
              → "서버상태가 좋지 않습니다…(302)" → StateNetMenu 파괴
```

`localserver.rs` 와 `hl5_server/server.py` 가 답하는 프레임은 **정확히 여섯 개**
(`1/1, 1/5, 9/1, 6/2, 6/5, 6/7`)뿐이다. 카테고리 1/3, 7, 2, 3, 4, 8 은 전부
무응답이다 — **즉 배포된 실서버도 똑같이 실패한다.**

#### 그리고 게스트가 스스로 "이용할 수 없는 기능"을 띄운다

이 절에서 가장 값진 발견이다. 임시 파이썬 서버로 `1/3`(result 0 + 별명)과 `7/1`
을 답해 주자 **창고접속 메뉴가 오류 없이 열렸고, 창고관리가 실제 창고 화면을
띄웠다.** 그리고 카테고리를 `result != 0` + EUC-KR 메시지로 거절하자 게스트가
`ProcPacket_*` → `NETWORK::CreateMessagePopup`(0xC8310) 경로로 **자기 자신의
원버튼 팝업에 그 문구를 띄웠다 — 게스트 후크 0줄.**

> 사용자가 요구한 UX 가 **`localserver.rs` 만으로** 된다. 그것도 메뉴 진입점이
> 아니라 **플레이어가 실제로 누른 항목 위에** 뜬다.

#### 여섯 항목 분류 (menu_text 28..33, 2×3 그리드)

| 항목 | 분류 | 근거 |
|---|---|---|
| 창고관리 | **로컬 가능** | `4/6`(목록) + `4/7`(소지금) + 변경 서브커맨드. 캐시샵이 쓰는 `_NET_ITEM_` 빌더 그대로 |
| 슬롯변경 | **로컬 가능** | 패킷을 아예 안 보낸다 |
| 캐쉬상점 | 이미 됨 | `free_shop` |
| 판매거래소 / 구매거래소 | **서버 필수** | 카테고리 2 — 다른 플레이어 |
| 우편함 | **서버 필수** | 카테고리 3 — 다른 플레이어 |

미해결 하나: 합성한 창고 레코드가 `NetStorageItem::NewNetStorageItem`(0xa6ac4)에
올바른 인자로 도달하고 모델을 채우는데 **그리드는 여전히 비어 보인다.**
`DrawStorageUI`(0x131e7c)가 모델에 없는 무엇을 읽는지 아직 모른다.

#### `localserver_ab.rs` 와의 충돌

그 테스트의 계약은 "`hl5_server/server.py` 와 바이트 동일"이다. 새 핸들러를
그냥 넣으면 그 뜻이 깨진다. **`Session::with_grants` 가 이미 쓰는 규율을 그대로
쓴다** — 새 응답을 플래그 뒤에 두어 맨 `Config::local_server` 세션은
파이썬 서버와 바이트 동일로 남긴다. 아니면 `server.py` 에도 같은 핸들러를 넣고
`tools_re/gen_localserver_fixtures.py` 로 픽스처를 재생성한다.

### 31.6 오브/제련 — 전부 구현돼 있고 실제로 돈다

**결론: 손댈 것 없음.** 근거는 정적 분석이 아니라 실행이다.

* **네트워크 의존성 0.** 22개 루트(모든 `StateInNpcMenu` 진입점, 모든
  `RefineItem::Apply*/Is*/Get*`, `Mission::CheckOrbCombine/CheckMissionRefine`,
  `EventProc` NPC 핸들러 둘, 두 표 로더)에서 직접분기 BFS — 네트워크 적중 0.
  오탐 둘은 이름에 "Socket"이 든 `ItemTable::MakeSocket`/`MakeSocketItemCompose`.
  **양성 대조**: 같은 도구가 `StateNetMenu::Process` 에서는 즉시
  `→ WriteData → PacketWrite_Notice → sendData → writeSocket` 을 찾는다.
* **4/8바이트 심볼은 스텁이 아니라 GCC 꼬리호출 썽크다.**
  `KeyHighRefine`(0x11472c) = `mov r2,#1; b KeyRefine`,
  `KeyLowRefine`(0x114734) = `mov r2,#0; b KeyRefine`,
  `DrawLowRefine` = `mov r1,#0; b DrawRefine`, Touch 둘은 `b TouchRefine`.
* **상태 8개 전부 진입 경로가 있다.** `KeyPressed`(0x118f94)의 8항 점프테이블,
  `Battle::CallNpcMenu` 호출 8곳 전수: `Event_UiEngineer` 가 3·4·5·7·2 를,
  `Event_UiBlackSmith` 가 6 을 연다.
  (`addls pc, pc, rX, lsl #2` 관용구 주의 — **case 0 은 `pc+8`**, 즉 폴스루
  기본 분기 **다음** 칸이다. 순진하게 읽으면 모든 case 가 하나씩 밀린다.)
* **명명 함정**: 상태 3 = `LowRefine` = **일반제련**(깨짐·파괴 있음),
  상태 4 = `HighRefine` = **고급제련**(절대 파괴 안 됨). 후자는 코드가 아니라
  **데이터**다 — `probability.dat` 의 고급 행은 임계값 2~4가 전부 1000 이라
  파괴 분기가 도달 불가다. 저급은 9레벨에서 60% 깨짐 / 6% 파괴.
* **데이터 전부 존재, 꼬리 바이트 0으로 파싱**: `probability.dat`(614 B),
  `smith_0..2.dat`(`HERO+0x277` 이 2로 클램프되므로 요청 가능한 티어가 정확히
  이 셋), `item_11/12/13/14/15/18.dat`.
* **별도 제련 비용표는 없다.** `GetItemRefineMoney`(0xa0900) = `price × (제련도+1)`.
  실측: 가격 200 아이템 → 비용 200.
* **소비 아이템**: 고급제련 = `kind 18 / index 8`(고급제련석, 캐시템),
  저급제련 = `kind 11 / index 10`(제련석, 일반템), 오브결합 = `kind 12`(오브,
  index 14..26 의 하이퍼*) + `kind 18`(index 는 픽커가 넘기는 값 그대로.
  게스트가 특별 취급하는 것은 row 9(달성의부적)뿐이고 성공 배수를 2배로 한다).
  **`shop.rs` 의 `GrantTable` 이 이미 그 (kind, index) 를 지급한다** —
  이 빌드에서는 유료 절반이 무료다.
* **실행 검증**: 기존 리플레이는 NPC 에 도달하지 않으므로(`Battle::CallNpcMenu`
  호출 0회) 임시 하네스로 게스트 자신의 `Battle::CallNpcMenu` 를 불렀다.
  화면 5개 전부 한글 정상 렌더. `ApplyItemRefine` 60회 구동 → 제련도 0→7 →
  저급 굴림에서 깨짐 → 이후 no-op(게스트 자신의 `cracked` 검사, `item+0x167`).
  분포가 `probability.dat` 와 일치.
* **세이브 왕복 OK.** 제련 결과가 게스트 자신의 DES 경로
  (`SaveAll → SaveBagData → SaveEncryptFile → MX_desEncrypt → startDes`, 전부
  게스트 심볼)로 기록되고 `LoadBagData` 가 `refine=7, cracked=1` 을 그대로 읽는다.
  **호스트 DES 는 관여하지 않으므로 "교과서 DES" 함정은 해당 없다.**
* **패치 A/B 32프레임 바이트 동일.** 이 화면들이 사양된 블리터 변종을 30프레임당
  ~315회 때리지만 **속도만 손해고 정확성은 무손실**이다.
* **RNG 는 전부 게스트 안에 있다.** `StaticUtil::Random`(0x143c08)은 게스트
  전역의 64비트 LCG 이고, 시드는 `onStartApp+0x27c` 에서 딱 한 번
  `RandSeed(MC_knlCurrentTime() % 31)`. 안드로이드에서 `now_ms` 는
  `FIXED_CLOCK_MS(1_700_000_000_000) + 부팅후경과` 이므로 실제 시드는
  `(13 + 부팅후경과ms) % 31` 이다 — 원본 분포를 원하면 안드로이드 셸만
  `SystemTime::now()` 로 바꾸면 되고, **프로브는 절대 바꾸면 안 된다**
  (고정 에포크가 905프레임 A/B 재현성의 근거다).

> **정정 (검증)**: "100% 로컬"은 과하다. 정확히는 **오브/제련 코드가 네트워크
> 트래픽을 발생시키는 경로는 없다.** 제련이 띄우는 팝업은
> `SetPopupState` → (vptr+0x70/+0x74 = `BaseState::CallPopupState`) →
> `StatePopup` 로 가고, `StatePopup::Process`(0x13fa80)는 모든 팝업에서 NETWORK
> 함수 **9개**(패킷 펌프 4개 포함)를 호출한다. `NETWORK+0x358` 이 0 이면 전부
> 무효라 오프라인에서는 아무 일도 없다 — 결론은 같고 근거가 다르다.
>
> **`BaseState` vtable 규약**: vptr = `_ZTV9BaseState` + 8.
> vptr+0x1c = `KeyPress`, **vptr+0x70/+0x74 = `CallPopupState` 두 오버로드**
> (조사가 `ChangeState`/`StateMgr` 라고 한 것은 틀렸다), vptr+0x80 =
> `CallNpcMenuState`.

#### 남은 위험 하나 — 회귀 시험에 이 화면들이 없다

905프레임 4장면 A/B 가 `StateInNpcMenu` 를 **한 번도 지나가지 않는다.**
`DrawRefine`(4,820 B) + `DrawOrbCombine`(4,664 B) + `DrawNormalMix`(2,996 B) +
`DrawCollection`(6,592 B) = **19 KB 의 드로잉 코드가 인수 기준 밖**이다.
§31.4 의 블리터 이식이 이 화면들을 조용히 깨도 아무도 모른다.

처방: 프로브에 `--npc-menu <0..7>` 을 넣어 스크립트가 끝난 뒤 게스트 심볼
`_ZN6Battle11CallNpcMenuEas` 를 `[0, state, 0]` 으로 부르고 N프레임 돌린다
(≈10줄, `Vm::call` 만 쓴다. 되돌리기는 `BaseState::ReturnState`). 이것은
`EventProc::Event_UiEngineer` 가 0x70054 에서 하는 **동일한 호출**이다.
**`patch.rs` 가 아니라 프로브에 둔다** — 화면을 바꾸는 스위치니까.

### 31.7 검증이 잡아낸 정정 — 그대로 구현했으면 틀렸을 것들

**바이트 동일 A/B 를 터뜨렸을 것 (치명):**

* **4비트 블렌딩 루프의 니블 순서가 반대였다.** 조사는 "상위 니블 → `dst+2`,
  하위 → `dst+0`"이라고 했다. 실제는 0x5d1f8 `add r0, ip, #1` / 0x5d1fc
  `add r0, lr, r0, lsl #1`(= `dst+2`)와 0x5d200 `add r1, lr, ip, lsl #1`(= `dst+0`)
  이고, 상위 니블(`asr ip,sl,#4` @0x5d218)이 **`dst+0`**, 하위가 **`dst+2`** 다.
  그대로 옮겼으면 **픽셀 쌍이 전부 뒤집힌다.**

**이식 명세가 틀렸을 것:**

* 두 번째 4비트 본체 0x5df80..0x5e1f0 은 ROT180(0x0c)이 **아니라 xform 0x08** 이다.
  **ROT180 의 4비트 본체는 0x5cd00 에서 시작한다.**
* `DrawImageStretchClip565` 의 `level == 0x10` 분기는 `ChangeBlendingPal` 이
  아니라 **`Graphic::ChangeGrayPal`(0x582f4)** 로 샌다.
* `_midas_funcFntDrawPixel` 은 globals 를 로드하는 게 아니라
  **`MC_grpGetRGBFromPixel` 로 픽셀을 분해**한 뒤 PreRGB 로 꼬리호출한다.
* `Graphic::ChangeBlendingPal` 의 게임플레이 비용은 **0** 이다.

**숫자 정정:**

* 메뉴 `FntDrawString` 의 지목된 루프는 91% 가 아니라 **82.4%**.
* 타이틀의 Blending 사양 경로 몫은 45% 가 아니라 **≈42%**, 인트로는 **≈59%**.
* 인터프리터 우회로 "3~7% 더 느림" → **유의미한 차이 없음** (n=2 대 5.6% 잡음).
* 프로브 처리량은 **46~54 M ins/s**, 부하 의존.

**기타:**

* `mvn r1,#4` 는 0xDD894 가 아니라 **0xDD890**.
* `BaseState::CallPopupState`(0x45560)는 `PopupSet` 을 직접 안 부른다 —
  `_ZnwjPci`(0xABD1C)로 0xC48 을 할당하고 **생성자**(0x141CD4)를 부르며,
  그 생성자가 0x141D48 에서 `PopupSet`(0x13B788)을 부른다.
* `TextMgr::GetText` 의 누산은 `blob + Σ(len_k + 2) + 1` 이다.
* **`Vm::hook_calls` 는 모든 후크가 공유하는 카운터 하나다**(`lib.rs:616`).
  "확인 횟수만큼 증가"를 인수 기준으로 쓸 수 없다.
* 창고접속의 `netConnect(2)` 다이얼은 `mov r1,#2` @**0x12504C** + `bl` @0x125058
  이다. 0x124FD8 은 `mov r1,#4` = 업데이트/패치 다이얼이다.
* `NETWORK::setError` 가 띄우는 팝업은 **모드마다 다르다**(`[this+0x34c]` 10항
  점프테이블, 베이스 0xC91F4): 0→팝업 26, 1→27, **2→StateNetMenu 0x24 또는
  StateCashShop 3**, 3~7·>9→`CreateMessagePopup`, 8→57, 9→59.
  "항상 팝업 59"는 틀렸다.
* `RecvParsering`(0xCDEEC)의 카테고리 배정: 0 System, 1 Login, **2 ItemTrade,
  3 Mail, 4 Inventory** … — 조사 초안이 `netConnect` 모드 번호와 와이어 카테고리
  번호를 뒤섞었다.
* `common_text[337]`("취소했습니다.")은 죽은 문자열이 아니다 —
  `MTX_Result_CB+0x9c`(0xCE47C)가 쓴다.
* 팝업 24 로 가는 문이 유일하지 않다 — **팝업 26 이 같은 핸들러**(0x124AC8)로
  간다(점프테이블 0x1248C8 의 0x124924·0x12492C 두 항목).

### 31.8 조사 산출물의 위치

에이전트 원본 결과(JSON)는 세션 스크래치패드에 남아 있고 세션이 끝나면 사라진다.
**이 문서가 그 요약이며, 재현에 필요한 주소·명령·수치는 전부 위에 옮겨 적었다.**
다음 세션이 알아야 할 작업 순서는 `CLAUDE.md` 의 "다음 세션이 이어서 할 것".


### 31.9 속도 레버의 적대적 검증 — 결론이 바뀌었다 (2026-08-20)

§31.2 의 주장 12개에 붙인 검증이 **10개를 재현 확인하고 8개를 반박했다**(일부는
같은 주장의 다른 측면). 그중 **셋은 계획을 바꾼다.**

#### 확정된 것 — §30.4 는 틀렸다

**게스트의 설계 틱 주기는 45 ms 가 아니라 36 ms(=27.8 틱/s)다.** 검증이 독립적으로
`timerLoop`(0x152318, 0x148 B) 전체를 디스어셈블해 발화 조건이 `now(64) > due(64)`
이고 실패 시 `+10 ms` 를 더해 다시 비교한다는 것을, 그리고 재무장이 자기교정
(always relative to `now`)임을 확인했다. **세 장면(타이틀 포함)에서 36.0 ms 로 실측.**

> **따라서 `ANALYSIS.md` §30.4 의 "게스트의 fps 표는 22 가 상한이므로 S25 는
> 천장에 닿았다"는 틀렸다.** 그 문장이 현재 블리터 작업을 멈추는 근거인데,
> 근거가 `timerLoop` 이 실제로 내주는 것이 아니라 **fps *표* 값**에 얹혀 있었다.
>
> 그리고 §30.4 의 "틱 36/s" 는 **게스트 틱 레이트가 아니다** —
> `stat_ticks`(`hl5_android/src/lib.rs:427`)는 **호스트 루프 반복**을 센다.
> 같은 로그 줄의 두 숫자가 서로 다른 것을 재고 있었다.

같이 확정된 것들:

* **`nativeLoop` 한 번에 게임 틱 최대 하나** — 구조적으로도(타이머 리스트를
  한 번만 순회) 경험적으로도 확인. **따라서 셸의 폴 격자가 틱 양자화기다.**
  실측: 같은 6,600 ms 를 dt=16 으로 돌리면 137틱(20.8/s), dt=1/2/4 로 돌리면
  183틱(27.7/s).
* fps 표(0x1586b0)와 주기 표(0x158688)를 읽는 다른 코드는 **없다**(전 워드 스캔).
  `gv[0x86]` 의 유일한 독자는 `TimerCb+0x74`(0x9de24). 단 `InitGlobalValue` 의
  `memset(gv,0,0x1de8)` 도 그 바이트를 건드리므로, 9 를 보장하는 것은 유일성이
  아니라 **순서**다(index 0 = 10 fps).
* `ParticleMgr::process`(0xd1a74)가 **실 ms 델타를 적분한다** — 확인. fps 표만
  올리면 이펙트가 1배로 남는다는 판단의 근거가 맞았다.
* 게스트의 두 `select` 는 모두 `timeval {0,0}` 을 넘긴다 — 확인.
* 세 마감 상수 전부 재현: 5,912 ms → `setError(302)`, 60,000 ms → 300,
  181,024 ms → 301.

#### 계획을 바꾸는 반박 셋

**1. 폴 격자 분리는 "게스트 비용 0" 이 아니다.** `nativeLoop` =
`timerLoop` + `updateScreen`(0x1520c4)이고 `updateScreen` 은
`Texture2D::update`(0x153138)를 **무조건** 부르며 그 모든 경로가 `glDrawArrays`
로 끝난다. 계측: **`gl.draws` == 프레임 수가 정확히 일치**(6600/6600,
3300/3300, 200/200). 즉 **폴 한 번마다 `gl.rs::gl_draw` 의 전체 캔버스 패스가
한 번** 돈다 — 데스크톱 x86 400×240 에서 빈 폴 하나가 **0.246 ms**(dt=1 대
dt=2 를 같은 183틱으로 격리해서 뺀 값; dt=2 대 dt=4 는 0.252 ms 로 일관).
게스트 쪽은 빈 폴당 ~157 명령어뿐이라 "인터프리터 비용 0"은 맞고
"프레임 비용 0"은 틀렸다.

> **따라서 4 ms 격자는 쓰지 마라. 12 ms 면 36 ms 주기를 정확히 회복한다**
> (`ceil(36/12)*12 = 36`) — 추가 블릿이 4 ms 의 1/4 이다.

그리고 이득이 균일하지 않다. `about_to_wait` 는 **작업 *전에*** `next_frame =
now + FRAME` 을 세우므로 26 ms 짜리 게임플레이 루프는 이미 연달아 돈다.
그 경우를 모델링하면 26 + (싼 폴) + 16.7 = **42.7 ms 주기 = 23.4 틱/s** 로,
§30.4 의 기기 실측 21.5~23.1 fps 를 **독립적으로 재현한다**. 12 ms 격자는
그것을 38 ms = 26.3/s 로 옮긴다 — **+13%**, +35% 가 아니다.
싼 장면에서는 20.8 → 27.7/s = **+33%** 가 실측된다.

**2. 폴을 분리하면 `Hud::span` 이 깨진다 — §28 이 고친 바로 그 UI 다.**
`Vm::frame` 이 첫 줄에서 `hud.frame_start()` 로 span(바를 그린 자리)을 지운다
(`lib.rs:1257`). 터치는 `window_event` 에서 오므로 **가장 최근 프레임이 틱
프레임이었을 때만** `unshift` 가 걸린다. 그 확률이 `틱레이트 / 폴레이트` 다 —
지금 ~1/3(20/60)이 4 ms 격자에서 **~1/9**(27.8/250)로 떨어진다. 인게임
map/shop/save 바를 누르기가 눈에 띄게 어려워진다.
`hud.rs` 의 테스트 `a_frame_that_did_not_draw_the_bar_steals_no_taps` 가 지금
동작을 못박고 있으니, **틱한 프레임에서만 span 을 지우거나 마지막 비어 있지 않은
span 을 유지**하도록 같이 고쳐야 한다.

**3. `Config::speed` 의 기본값은 1.0 이어야 한다 — 1.5 는 안 된다.**
S25 게임플레이는 현재 ~23 틱/s 에서 게스트 6.9~7.0 s/10 s 를 쓴다. 설계 레이트
27.8/s 만 회복해도 **~8.4 s/10 s** 가 필요하다. 여기에 1.5배는 41.7 틱/s =
**초당 1.25초의 게스트 CPU** — 불가능하다. 그리고 못 따라가면
`TimerCb` 의 5 ms 바닥(0x9dfc8) 때문에 타이머가 영구히 "이미 지남"이 되어
**틱 레이트는 안 오르는데 밀리초로 표현된 모든 양은 그대로 1.5배가 된다**:
`ParticleMgr::process` 가 1.5배 델타를 먹고, `HERO::ProcAutoExpGoodTime`
(0x8d5a4)이 버프를 1.5배로 태우고, `SaveSlotData` 가 플레이타임을 1.5배로
부풀리고, 워치독이 1/1.5 로 줄어든다. **§31.2 가 경고한 위상 깨짐보다 더
나쁘다.**

> 1.0 을 기본으로 넣고, 1.0 초과는 **측정된 여유가 있을 때만** 켜는 실험으로
> 둔다. 검증 기준은 "곱셈이 일어났다"가 아니라 **기기 로그의 틱 레이트가 실제로
> `27.8 × speed` 에 닿았는가** 다.

#### 그 밖의 정정

* **`Vm::now_ms` 의 독자는 하나가 아니다.** 게스트가 보는 것은
  `hle.rs:542`(`ftime`) 하나지만, 호스트 쪽에 프로브 셀프테스트 셋이 있다
  (`hl5_probe/src/main.rs:937, 942, 976` — `waited == 2000`,
  `sec == vm.now_ms()/1000`). 시계를 스케일하면 **`--selftest` 29개가 깨진다.**
* **`ftime` 이 게스트에게 가는 유일한 벽시계 값은 아니다.** `stat` 이 호스트
  파일의 **실제 `st_mtime`** 을 구조체 +80 에 실어 주고
  (`files.rs:525, 606-611`), `MC_fsFileAttribute`(0x14c0b8)가 0x14c114 에서
  그 워드를 읽어 1000 으로 나눠 쓴다. 호출처는 `StaticUtil::LoadRes+0x50` 과
  `_midas_funcDspCrossRoad*` 셋. **스케일된 `ftime` 과 스케일 안 된 이 값이
  서로 어긋난다** — 지금은 무해하지만 기록해 둔다.
* **§31.2 가 제안한 바이트 동일 인수 기준은 실행 불가능하다.** ⑴ 산술이 틀렸다
  — `Vm::frame` 이 가상 전진을 스케일하면 `speed 2 / dt 66` 은 132 ms 를 전진해
  `speed 1 / dt 33` 과 4배 어긋난다. 동등한 짝은 `(s, d)` 대 `(1, s·d)` 이고
  `speed 2` 의 짝은 `dt 16.5` 인데 `dt` 는 `i64` 다. ⑵ 프로브에 dt 노브가 없다
  (`const FRAME_MS: i64 = 33`, `main.rs:566`). ⑶ **가장 중요한 것** — 스케일은
  `virtual_clock == false` 분기(`lib.rs:521`)에 사는데 프로브는 언제나
  `virtual_clock == true` 다. **프로브 시험은 출하되는 줄을 절대 실행하지 못한다.**
  정직한 인수 증거는 기기 실행(10초 통계 줄의 틱/fps/게스트 ms)과 오디오·애니메이션
  동기 확인뿐이다.
* **`speed` 는 f32 가 아니라 f64 나 정수 유리수로.** `elapsed_us * speed` 를
  f32 로 하면 가동 한 시간 뒤 ~0.25 ms 단위로 양자화된다.
* **폴 격자 변경만으로도 게임 속도가 최대 1/3 빨라진다.** 이것은 원본 의도의
  복원이지 회귀가 아니지만, **905프레임 A/B 는 그것을 볼 수 없다** — 프로브의
  `FRAME_MS` 가 상수 33 이고 변경은 셸 쪽이다. **"A/B 가 통과했다"를 "아무것도
  안 바뀌었다"로 읽지 마라.**
* 주소 라벨 정정(숫자는 전부 맞았고 라벨만 틀렸다):
  `SlotInfo::SaveSlotData` 는 **0x8c804**(0x8c8bc 는 +0xb8),
  `CheckServerConnectLimitTime` 은 **0xc93a4**(0xc93d0 은 +0x2c),
  `CheckNoKeyPressed` 는 **0xc9330**(0xc9364 는 +0x34),
  0x13fbe4 는 `StatePopup::Process+0x164`. **상수 자리는 `심볼+오프셋` 으로 써라.**
* `connect_real` 의 `5/speed` 조정은 논리는 맞지만 **출하 빌드가 안 지나는
  경로**다(`server.is_none()` → 인프로세스 서버). 필수 아님.

#### 그래서 ②의 순서가 바뀐다

1. **폴 격자 12 ms + `Hud::span` 수정** — 싼 장면 +33%, 게임플레이 +13%.
   이것만으로 사용자가 말한 "메뉴·컷신이 느리다"의 3분의 1이 사라진다.
2. **블리터 셋**(§31.4) — 게임플레이의 CPU 를 실제로 비운다.
3. **`Config::speed`, 기본 1.0** — 2가 끝나 여유가 측정된 뒤에만 1.0 초과를 시험.

## 32. §31 의 조사를 구현으로 옮겼다 — 그리고 둘이 더 붙었다 (2026-08-20)

§31 은 조사만 하고 코드는 한 줄도 안 바꿨다. 이 절은 그 실행 기록이다. 순서는
`CLAUDE.md` 의 계획 그대로 — 위험 0 인 계측부터, 회귀 장면을 확보한 뒤 블리터.

작업 중에 사용자가 둘을 더 요구했고 그것도 여기 있다: **⑥ 캐시샵에서 산 오브가
가방에서 아무 메뉴도 안 뜬다**(§32.9, 호스트 버그였다), **⑦ 화질**(§32.10,
모든 프레임에서 스캔라인 두 줄이 사라지고 있었다).

### 32.1 계측 셋 — 그리고 §31.3 의 "설명 안 되는 것"이 설명됐다

§31.3 이 남긴 미해결: **정적 메인 메뉴가 화면을 한 픽셀도 안 바꾸면서
`vm.frame()` 에 벽시계의 46~68% 를 태운다.** present 는 0 인데 present
최적화의 상한(7.5%)보다 큰 숫자였고, 그 돈이 `gl_draw` 인지 게스트의 재칠인지
빈 틱인지 갈릴 방법이 없었다.

셋을 넣었다. 셋 다 게스트를 한 바이트도 바꾸지 않는다.

| 계측 | 어디에 | 무엇을 |
|---|---|---|
| `t_acquire` | `hl5_android` | `buffer_mut()` 를 `t_swap` 에서 분리 (값은 이미 계산돼 있었고 `t_swap` 에 묻혀 있었다) |
| `gl {n} ms` | `hl5_core::gl::GlState::draw_time` | `gl_draw` 전체 소요. **폴마다** 도는 비용이라 present 통계에 안 보인다 |
| `repaints/s` | `Vm::probes` (신규) | `MC_grpFlushLcd` 진입 수 = 게스트 자신의 "화면에 올려라" |

`Vm::probes` 는 `patches`·`hooks` 와 같은 비트맵 디스패치를 쓰는 **세 번째 맵**
이지만 성격이 다르다 — 후크도 패치도 아니고 **아무 것도 바꾸지 않는 카운터**다.
그래서 출하 경로에 켜 둘 수 있고 오프라인 A/B 가 계속 같은 것을 증명한다.
`Vm::count_symbol` / `Vm::probe_count` / `Vm::guest_repaints`, 프로브
`--count <심볼>`.

**S25 실측이 답을 줬다 (정적 메인 메뉴, 10초 창 3개):**

```
0.0 fps, 12.3~12.5 M guest ins/s (guest 6124~7033 ms per 10 s)
50~58 ticks/s, 23.4~23.9 repaints/s, 290~361 skipped
gl 427~463 ms, acquire 0 ms, blit 0 ms, swap 0 ms
```

* `gl` 은 벽시계의 **4.3~4.6%** 다 — 호스트의 확대 패스는 답이 아니다.
* present 는 0 ms(전부 스킵). 화면이 안 바뀐다는 것은 맞다.
* **게스트가 그림이 똑같은데도 초당 24회 화면을 다시 그린다.** 재칠 한 번에
  게스트 CPU 26 ms. 즉 §31.3 의 6.5초는 **빈 틱도 `gl_draw` 도 아니라 진짜
  재렌더**이고, §31.4 가 이미 말한 대로 그 재렌더의 79% 가 폰트 스택이다.

> **처방이 확정됐다**: 메뉴는 폰트 이식으로만 싸진다. present 를 더 줄이거나
> 폴을 줄이는 것으로는 못 산다.

곁다리 하나 — 프로브에서 `gl draw pass` 는 프레임 수와 정확히 같고
(`--frames 600` → 600 calls) `MC_grpFlushLcd` 는 그 절반이다(300). §31.9 의
"폴 한 번마다 `gl_draw` 한 번"이 헤드리스에서도 성립한다.

### 32.2 기기 측정의 함정 하나 — 화면이 꺼져 있으면 `draws` 가 얼어붙는다

처음 측정한 10초 창 여섯 개가 전부 `0.0 fps, 0 draws/s, gl 0 ms` 였다.
스크린샷은 완전한 검정. 코드 회귀로 읽고 한참을 뒤졌는데, **폰 화면이 꺼져
있었을 뿐이다.** 화면을 깨우고 셰이드를 내리면 같은 빌드가 24 fps 를 낸다.

게스트는 그동안에도 25 M ins/s 를 태우고 `repaints/s` 는 13~21 이었다 — 즉
**"게스트가 돌고 있다"가 "화면이 갱신된다"의 증거가 아니다.** `am start` 뒤에
`input keyevent KEYCODE_WAKEUP` 을 넣고 `dumpsys window | grep mCurrentFocus`
로 우리 액티비티가 포커스를 잡았는지 확인한 다음 재는다.

### 32.3 ① 구매 확인 모달 — 게임 자신의 문장으로 조립한다

`free_shop` 이 가격 줄에 `~~500원~~ 0원` 을 그려도 확인 모달은 여전히
`실제 현금 500원의 / 추가 정보이용료가 부과됩니다. / (통화료 부과)` 라고
말한다. 후크 둘, 둘 다 `hl5_core/src/shop.rs`, 둘 다 `Config::free_shop` 게이트.

| | 후크 대상 | 복귀주소 필터 | 갈아끼우는 인자 |
|---|---|---|---|
| A 확인 모달 | `StateCashShop::SetPopupState` (0xDBFC8) | `StateCashShop::KeyPressed+0x27C` | `a[2]` = msg |
| B 부활 나그 | `BFont::DrawText(iiiPcii)` (0x445E8) | `StateGame::DrawOverText+0xF8` | `a[4]` = str |

디스어셈블로 확인한 것: A 의 조립은 `GetCommonText(0x10a)`·`(0x10b)`·`(0x149)`·
`(0x130)` + 가격(`ldrh [r3,#0x12]`, 할인 갈래) → `MC_knlSprintk("%s %d%s;%s;%s")`
→ `SetPopupState(this, 2, msg, 0, 0, 9)`. B 는 같은 모양에 가격이 **즉치 100**
(`mov ip,#0x64` @+0x7C)이고 `DrawText(this, fb, halfW, halfH-0x32, str, 1, 0x00ffffff)`.

교체본은 **게임 자신의 행만** 쓴다(새 한글 바이트 0, ASCII 만 추가):

```
A: [0x10a] " 0" [0x109] ; [0x14a] ; [0x5b]
   → 실제 현금 0원 / (정보료/통화료 무료) / 구입하시겠습니까?      52 바이트
B: [0x104] ; [0x10a] " 0" [0x109] ; [0x14a]
   → 부활의 부적을 구매하시겠습니까? / 실제 현금 0원 / (정보료/통화료 무료)
```

문장은 `TextMgr::GetCommonText` 를 **게스트에서 호출해서** 조립한다 —
`c/csv/common_text.dat` 를 호스트에서 다시 파싱하면 그것이 크로노스의
"측정기 두 개" 함정의 재판이다. 조립 결과는 캐시한다(B 의 자리는 draw 라서
그 화면이 떠 있는 동안 매 프레임 불린다).

**실측 (호스트, `--free-shop` + `ingame.txt`)**: `free-shop: Confirm text
replaced, 52 bytes at 0x01aa6a78`, 그리고 `out/shot/c5-buy1.png` 에 세 줄이
그대로 렌더된다 — 예/아니오 버튼(`kind = 2`)도 그대로다. 구매도 여전히 완주해
엘릭서(20)이 가방에 들어온다.

#### 같이 고친 기존 버그

`shop::compact_price_hook` 에 `Config::free_shop` 검사가 **없었다.** 그 후크를
제공하는 `draw_text_plain` 디스패처는 `free_shop || hud.dx != 0` 에 설치되고
넓은 캔버스는 폰에서 항상 켜져 있으므로, **`hl5_server.txt` 로 실서버를 가리킨
설치본이 진짜 유료 상점에도 취소선과 `0원` 을 그렸다.** 대칭을 위해
`detail_price_hook` 에도 같은 검사를 넣었다.

### 32.4 회귀 장면 — `--npc-menu`, 그리고 19 KB 가 인수 기준에 들어왔다

§31.6 이 남긴 위험: 905프레임 4장면이 `Battle::CallNpcMenu` 를 **0회** 부르므로
`DrawRefine`+`DrawOrbCombine`+`DrawNormalMix`+`DrawCollection` = **19 KB 의
드로잉 코드가 A/B 밖**이었다. 블리터 이식이 그 화면을 조용히 깨도 아무도 모른다.

프로브에 스크립트 명령 `npc-menu <0..7>` / `npc-return` 과 CLI `--npc-menu`
`--npc-frames` 를 넣었다. `Battle::CallNpcMenu`(0x45A28)와
`BaseState::ReturnState`(0x44E20)는 **둘 다 `this` 를 안 쓴다** — 각자
`gv+0x27C` 에서 현재 상태를 읽어 vtable(슬롯 0x80 / 슬롯 4)로 간다. 그래서
`[0, state, 0]` 이 편법이 아니라 인자가 죽어 있다는 뜻이다. `EventProc::
Event_UiEngineer+0x70058` 이 하는 것과 **같은 호출**이다.

새 장면 `tools_re/scripts/npcmenu.txt` (336 프레임): ingame 접두사로 게임플레이에
들어간 뒤 상태 0..7 을 차례로 열고 닫는다. 판매·일반제련·고급제련·오브결합·
조합·도감 다섯 화면이 전부 한글까지 정상 렌더된다 (`out/shot/n1..n8`).

**A/B: 336/336 바이트 동일** (현재 패치 상태로). 즉 §30 까지의 이식은 이 화면을
깨지 않았고, **인수 기준이 905 → 1,241 프레임**이 됐다.
러너는 `tools/ab_905.sh` (오프라인·장면별 신규 storage·같은 순서 — §18.4 와
§25.6 의 함정 둘을 스크립트에 못박았다).

### 32.5 ② 폴 격자 12 ms — 그리고 `Hud::span` 을 같이 고쳐야 하는 이유

§31.9 가 확정한 것: `nativeLoop` 한 번에 게임 틱은 최대 하나이므로 **셸의 폴
주기가 곧 게스트 틱 상한**이고, 게스트의 설계 주기는 45 가 아니라 36 ms 다.
16.667 ms 격자에서 36 ms 마감은 50 ms 로 반올림되어 게스트가 20 틱/s 에 묶인다.

`hl5_android` 의 한 격자를 둘로 갈랐다:

* `POLL = 12 ms` — 게스트 몫만 (`vm.frame` + 로그 드레인 + `audio_flush`).
  `ceil(36/12)*12 = 36` 으로 주기를 정확히 회복한다.
* `FRAME = 16.667 ms` — present 만, `draws != presented_draws` 게이트 그대로.
* `WaitUntil(min(next_poll, next_frame))`, 두 마감 다 `now` 기준 재무장.

**12 ms 다. 4 ms 가 아니다** — 폴 하나마다 `gl_draw` 의 전체 캔버스 패스가 한 번
돌기 때문이다. S25 실측으로 그 값이 이제 있다: **폴당 0.94 ms**(gl 723 ms /
770 draws). 12 ms 격자에서 83 폴/s = 벽시계의 7.8%, 4 ms 격자면 250 폴/s =
**23%** 였을 것이다.

#### `Hud::span` — 안 고치면 §28 의 인게임 바가 더 나빠진다

`Vm::frame` 첫 줄이 `hud.frame_start()` 로 span(바를 그린 자리)을 지우고 있었다.
그런데 **프레임은 그 질문의 옳은 시계가 아니다**: 폴 대부분은 타이머 목록을 돌고
아무 것도 그리지 않으므로, 매 프레임 지우면 span 이 대부분의 프레임에 없고
터치가 `unshift` 될 확률이 `틱레이트/폴레이트` ≈ 1/3 이 된다. `map/shop/save`
탭 셋 중 둘이 아무 일도 안 하고 있었다.

번역이 실제로 알아야 하는 것은 *"지금 화면에 있는 그림에 바가 들어 있는가"* 이고,
그것은 **게스트가 재칠할 때만** 바뀐다. 그래서:

* `Hud::frame_start(painted: bool)` — 직전 프레임이 **그림을 올렸고** 바를 안
  그렸을 때만 span 을 버린다(= 플레이어가 그 화면을 떠났다).
* 아무 것도 안 그린 프레임은 span 을 그대로 둔다.
* `saw()` 가 프레임의 첫 셀에서 span 을 교체하고 나머지 셀에서 넓힌다.

`painted` 는 §32.1 의 `MC_grpFlushLcd` 카운터에서 온다 — 그래서 그 카운터를
`Vm::boot` 이 **무조건** 설치한다. 단위시험 둘을 추가했고
(`frames_that_painted_nothing_keep_the_bar_pressable`,
`a_repainted_bar_replaces_the_previous_span_rather_than_widening_it`), 기존
`a_frame_that_did_not_draw_the_bar_steals_no_taps` 는 그 뜻으로 이름과 내용을
바꿨다(`a_painted_frame_without_the_bar_steals_no_taps`).

> **1,241프레임 A/B 는 이 변경을 볼 수 없다** — 프로브의 `FRAME_MS` 는 상수
> 33 이고 변경은 셸 쪽이다. "A/B 통과"를 "아무것도 안 바뀌었다"로 읽지 마라.

### 32.6 블리터 넷 — 오프라인 1,241 프레임 A/B 전부 바이트 동일

§31.4 의 장면별 프로파일이 지목한 셋과, 그것을 이식하다가 **프로파일이 스스로
드러낸 넷째**를 이식했다. 전부 `patch.rs`, 전부 `--no-patch` 와 프레임 바이트
동일(newgame 119 + ingame 543 + shop 134 + online 109 + **npcmenu 336** = 1,241).

| 이식 | 지목 근거 | 결과 (ingame 게스트 명령어) |
|---|---|---|
| `DrawImageBlendingClip565` 4비트 `xform 0x10` (사양 해제) | 게임플레이 62.7% | 3,276.8 M → 2,711.9 M (−17.2%) |
| `DrawImageStretchClip565` + `…565R` (신규 2개) | 컷신 76% | → 2,195.1 M (−19.1%) |
| `_midas_funcFntDrawPixel` (신규) | 메인메뉴 폰트 스택 49.6% | → 1,833.2 M (−16.5%) |
| **`level == 0x10` 회색 팔레트 경로** (사양 해제) | 아래 | → **1,609.1 M (−12.2%)** |

| 장면 | 이식 전 | 이식 후 | |
|---|---|---|---|
| newgame | 246.1 M | 159.0 M | **−35.4%** |
| **ingame** | 3,276.8 M | **1,609.1 M** | **−50.9% (2.04배)** |
| shop | 292.8 M | 221.3 M | −24.4% |
| online | 279.9 M | 212.8 M | −24.0% |
| **npcmenu** | 1,811.7 M | **811.7 M** | **−55.2% (2.23배)** |

사양(`patch_declined`)은 ingame 40,168 → **11,032**.

#### 4비트 니블 순서는 두 곳에서 확인했다

§31.7 의 정정이 맞다: **상위 니블 → `dst+0`, 하위 → `dst+2`.** 함수 안에서는
`+0x718 add r1,lr,ip,lsl #1`(= dst+0)에 상위(`+0x730 asr ip,sl,#4`)가 가고
`+0x710/+0x714`(= dst+2)에 하위가 간다. 그리고 **쓰는 쪽** —
`Graphic::GbmFillRect`(0x55110) `+0x7C..+0xA4` — 가 같은 배치를 쓴다:
`tst r3,#1` 로 짝수 열은 `andeq ip,ip,#0xf`+`orreq`(상위에 쓴다), 홀수 열은
`bicne`+`orrne`, 그리고 바이트 포인터는 홀수 열 뒤에만 전진한다.

같이 확정된 것 둘: **소스 행 스트라이드는 `ceil(img_w/2)`** — §31.4 가 인용한
`asr fp,fp,#1` 은 *출력 쌍 개수*이고 다른 값이다. 그것을 스트라이드로 쓰면
홀수 폭 소스가 행마다 반 픽셀씩 누적해서 밀린다. 그리고 **홀수 `sx` 는 별도
루프가 아니라 같은 쌍 루프의 서론**이다(`+0xC98..+0xDBC` 가 `b +0x6F0` 으로
합류한다).

#### 컷신 쌍둥이는 한 몸으로 옮겼다 — 세 곳만 다르다

`…565R` 은 전치(transpose)일 뿐이다: 목적지가 소스 열마다 한 칸 **아래로**,
소스 행마다 한 픽셀 **오른쪽으로** 간다. 함정 둘이 진짜 함정이었다 —
`CalcAnchor` 에 `(dh, dw)` 를 넘긴다(비-R 은 `(dw, dh)`), 그리고 **Y 뒤집기가
정확히 반대**다: 비-R 은 xform 4·0xC 에서 뒤집고(상수는 `src_h - 1`), R 은 그
둘에서만 **뒤집지 않는다**(`+0x1F4 cmp r4,#0xc / beq`,
`+0x1FC cmp r4,#4 / rsbne`, 그리고 `+0x1F0 cmp r4,#8` 은 뒤집는다). 비-R 규칙을
그대로 복사하면 컷신 R 호출 1,126 중 566 이 위아래로 뒤집힌다.

인자 10·11 은 목적지 크기가 아니라 **배율 퍼센트**이고, 두 축의 10.10 스텝은
몫이 아니라 **몫 + 1** 이다(`+0x218 add r8,r3,#1`). 둘 다 그대로 옮기지 않으면
"그럴듯하지만 틀린" 스프라이트가 나온다. 클립은 `CalcClipLCD` 의 출력도
`MC_GrpContext` 의 클립도 아니라 **객체 필드 `this+0x40E0..EC`** 다.

#### 폰트는 산술이 절반, 트램폴린이 절반

`_midas_funcFntDrawPixelPreRGB` 의 유일한 저장 명령이 `memcpy@plt` 다. 즉 **폰트
픽셀마다 HLE 왕복 한 번**이고, ingame 리플레이에서 그것이 325만 번이다. 이식 뒤
`HLE calls served` 가 6,613,930 → **3,363,616** 으로 떨어진다 — 절반이 그 호출
하나였다.

세 가지를 그대로 옮겼다: **순수 함수가 아니다**(`+0x224/+0x238/+0x23c` 가 블렌드
결과를 `_midas_rgb_{b,r,g}0` 에 되쓴다 — 안 옮기면 두 번째 픽셀부터 색이 틀린다),
**`opacity == 0` 조기 탈출이 없다**(표를 그대로 통과한다), 그리고 표는
`_midas_internalFntOpacity` 에서 **읽는다**(다시 계산하지 않는다 — `MX_fntInit`
이 안 돌았으면 표가 0이고 게스트는 검게 그린다).

**좁게 옮긴 것 둘, 둘 다 인수 기준이 없기 때문이다.** `_midas_drawDIR != 0`
(회전 IME 자판의 축 재사상 — `imeLocal_QWERTY_Paint` 가 실제로 그 전역을 세우고
이 체인으로 그린다. 그런데 **어느 리플레이도 IME 를 열지 않는다**), 그리고
`_midas_funcFntDrawPixelPreRGB` 진입점 자체(`_midas_funcFntGroup_Draw+0x1cc` 가
유일한 다른 호출자이고 다섯 장면에서 0회). 둘 다 사양하면 원본이 돈다.
13개 전역 주소는 `install_patches` 가 **한 번** 풀어 `Vm::fnt` 에 담는다 —
픽셀마다 `HashMap` 조회 13번은 이식이 버는 것을 그대로 반납한다.

#### 넷째는 프로파일이 스스로 드러냈다 — `level == 0x10` 은 변종이 아니라 서론이다

셋을 이식한 뒤 `--profile` 이 여전히 `DrawImageBlendingClip565` 를 22.9% 로
올렸고, `--profile-in` 은 **이미 이식한 4비트 `xform 0x10` 루프**(+0x700..+0x900)
를 지목했다. 사양 이유별 카운터를 임시로 넣어 갈랐다:

```
level0x10=898  decl4bit=8607  decl8bit=2425  tblfail=0  palfail=0
4비트 NORMAL 행 루프(0x5D1D8) 진입: 70,044회
```

**898 번의 사양이 그 함수에 남은 비용의 절반이었다.** `level == 0x10` 은 별도
바디가 아니다 — `+0x130 beq` 가 가는 0x5DD34 의 전부는
`Graphic::ChangeGrayPal(this, img)` + `b +0x134` 이고, 그 `+0x134` 는 팔레트를
읽는 `+0x138` 바로 앞이다. 공용 출구 `+0x19c` 가 같은 바이트를 다시 보고
`Graphic::RestorePal` 을 부른다. 즉 **이미지 팔레트를 회색으로 바꿔 두고
평범하게 그린 뒤 되돌리는 것**이고, 그 사이의 그리기는 이 파일에 이미 있던
루프다. 큰 블릿(미니맵)이라 호출 수가 적어도 비용이 크다.

> **그리고 여기서 A/B 가 값을 했다.** 처음 이식본은 1,241 중 **360 프레임이
> 달랐다**(ingame 180 + npcmenu 180) — 미니맵이 회색 대신 알록달록했다. 원인은
> 블렌드 표 인덱스다. 그것을 담는 레지스터는 `sl` 이고, `level` 을 거기 넣는
> 명령 `+0x12c movne sl, fp` 이 **바로 회색 분기가 건너뛰는 명령**이다. 거기서
> `sl` 이 실제로 들고 있는 것은 `+0x11c subs sl, r0, #0` 이 남긴
> `CalcClipLCD` 의 반환값이고, 그 지점에 도달했다는 것 자체가 그 값이 **0**
> 이라는 뜻이다. 즉 **회색 블릿은 표 0을 쓴다.** `level`(0x10)로 인덱스하면
> 16 KB 를 넘어 객체의 다른 필드를 표로 읽는다 — 화면에는 색으로 보이고,
> 스크린샷 한 장으로는 "회색이 아니네" 정도로만 보인다.

`ChangeGrayPal` 은 팔레트를 **제자리에서 바꾸므로**, 그 호출 뒤의 모든 탈출
경로가 되돌려야 한다(사양하는 둘까지). 하나만 빠뜨리면 스프라이트가 영구히
회색이 되고, A/B 에는 "어느 장면 중간부터 시작해서 끝까지 계속되는 차이"로
나타난다. `restore_and` 가 그 자리다.

#### 남은 것 (다시 프로파일한 순서, ingame)

```
26.2%  _midas_funcFntDrawString        ← 글리프 비트 루프 (픽셀 쓰기는 이미 이식됨)
22.9%  DrawImageBlendingClip565        ← 남은 사양: 4비트 xform 8 (8,738회)
22.2%  DrawImageMirrorClip565          ← 아직 손 안 댐
 4.0%  desRound                        ← 부팅 항목 (리소스 복호화)
 2.6%  DrawImageRotateEx5Clip565
```

4비트 xform 8 의 바디는 `--profile-in` 으로 +0x14C0..+0x1600 확인.

#### 기기 실측 (S25, 릴리스 빌드, 10초 창)

| 장면 | 이식 전 게스트 ms/10s | 이식 후 | |
|---|---|---|---|
| **정적 메인 메뉴** | 6,124~7,033 | **3,990~4,533** | **−35%** |
| 타이틀 | 6,753~6,815 | 5,782~5,992 | −12% |
| 정지 게임플레이 | 7,701~7,791 | 7,511~7,527 | −3% |

**정지 게임플레이가 거의 안 움직인 것은 정직하게 적어 둘 값이다.** 프로브의
ingame 리플레이 −50.9% 는 인트로·컷신·로딩이 큰 비중이고, 정지한 게임플레이
한 프레임은 그 평균이 아니다. 게스트 명령어는 35 → 18.5 M/s 로 반이 됐지만
벽시계는 거의 그대로다 — 호스트 코드가 인터프리터가 쓰던 시간을 거의 그대로
쓴다는 뜻이고, 픽셀마다 경계검사 있는 `read_u16`/`write_u16` 을 하는 이식의
한계다. 게임플레이를 더 싸게 만들려면 남은 셋(위)이 필요하다.

같이 관측된 것: `ticks/s` 가 50~58 → **73~77** 로 올랐고(폴이 싸져서 12 ms
격자에 가까워졌다) `gl` 이 427~463 → **910~986 ms/10s** 로 배가 됐다 — 폴이
많아진 만큼 §32.5 가 예고한 대로다. `repaints/s` 는 24.7~25.7 로 설계값
27.8 에 접근한다.

### 32.7 ⑤ 창고접속 — `localserver.rs` 만, 게스트 후크 0줄

증상은 "커넥팅 뒤 서버오류(302)로 튕김"이었고 원인은 접속 실패가 **아니다**:
소켓은 열리고 `1/1`·`1/5` 가 답해지고, 그 다음 게스트가 **`1/3`(자기 별명)** 을
묻는데 아무도 답하지 않아 5,912 ms 뒤 `CheckServerConnectLimitTime` 이
`setError(302)` 를 내고 `StateNetMenu` 를 파괴한다. (그 5,912 는 측정값이 아니라
`+0x2c movw r6,#0x1388` + `+0x30 add r6,r6,#0x390` 이라는 코드 상수다.)

해법에 게스트 후크가 필요 없다는 것이 이 절의 요점이다. **이 메뉴에서 닿는 모든
`ProcPacket_*` 이 같은 모양**이다 — `readNet32` → `NETWORK+0x368`(result),
`readNetBuf` → `NETWORK+0x36C`(message), 그리고 `result != 0` 이면
`NETWORK::CreateMessagePopup(NETWORK+0x36C)`. 그래서
**"이용할 수 없는 기능"은 UI 변경이 아니라 응답**이고, 게스트가 자기 팝업으로
**플레이어가 실제로 누른 항목 위에** 띄운다.

답해야 하는 둘 (둘 다 실측):

* **`1/3`** — `[i32 0][blob ""][blob 별명]`. `ProcPacket_Login` 의 subcmd-3 팔이
  세 번째 blob 의 **15바이트**를 `NETWORK+0x359`(`GetNickNamePtr`)로 memcpy 한다.
  보고할 이름이 없으므로 설치본의 `PHONENUMBER` 앞 15자를 쓴다(ASCII, 설치마다
  다름). 화면에는 게스트가 `…님 반갑습니다` 로 조립해서 띄운다.
* **`7/1`** — `[i32 0][blob ""][blob 안내문]`. `ProcPacket_Notice`(0xC84EC)는
  **조용히 답할 수 없는 유일한 프레임**이다: `result != 0` 이면 message 를,
  `result == 0` 이면 *두 번째* blob 을 무조건 팝업으로 띄운다(`+0x84`). 그래서
  거기에 오프라인 안내를 싣는다 — 빈 blob 은 빈 상자가 된다.

그 밖의 카테고리 2·3·4·8 은 `[i32 1][blob "현재 이용할 수 없는 기능입니다."]` 로
거절한다. **`0/2`(`SendAlivePacket`)는 답하지 않는다** — `WriteData+0x1c` 가
`NETWORK+0x0b` 를 지우고 `PacketWrite_System` 만 그것을 세우며, `WriteData` 의
꼬리는 그 플래그를 보고 워치독 시각을 찍기 **전에** 돌아간다. 즉 킵얼라이브는
5,912 ms 타이머를 무장하지도 "Connecting" 을 띄우지도 않는다.

**실측 (`--free-shop --offline-menus`, `tools_re/scripts/warehouse.txt`):**

```
localserver: nickname -> "000000000000000"
localserver: 2/3 declined — offline (이용할 수 없는 기능)
localserver: 4/6 declined — offline (이용할 수 없는 기능)
localserver: 0/2 — no handler (same as the deployed server)
setError 0회
```

화면(`out/shot/w*.png`): 창고접속이 **오류 없이 열려** 여섯 항목이 다 보이고,
별명 환영 팝업과 오프라인 안내 팝업이 게스트 자신의 OK 박스로 뜨고,
판매거래소를 누르면 그 화면 위에 **"현재 이용할 수 없는 기능입니다."** 가 뜬다.
슬롯변경은 패킷을 아예 안 보내므로 그냥 된다.

#### 왜 창고관리를 "채워서" 열지 않았나

채울 수는 있다. `4/6` 을 `[i32 result][blob msg][i32 capacity]` + 슬롯당 레코드
`[i32 slot][u8 flag][_NET_ITEM_]` 로 답하면 그리드가 찬다. 그리고 §31.5 의
미해결 항목이 여기서 풀렸다: **그 `capacity` 는 용량이면서 동시에 레코드 개수**
이고, `DrawStorageUI` 가 페이지를 `capacity >> 4` 로 센다 — 그래서 16 미만이면
`NewNetStorageItem` 이 몇 번 성공했든 **그리드가 통째로 비어 보인다.** 빈 슬롯
레코드는 정확히 `[i32 slot][u8 0][i32 0]` 9바이트다(`readNetItem` 은 count ≤ 0
에서 바로 돌아온다). 상한은 80 슬롯(`sizeof(NetStorageItem) = 0x1E8`, 포인터
배열 `+0x08..+0x148`)이고 `NewNetStorageItem` 은 슬롯 인덱스를 **검사하지
않는다** — 호스트가 막아야 한다.

그래도 채우지 않았다. **넣을 수는 있는데 잊어버리는 창고는 닫힌 창고보다 나쁘다** —
입고는 `4/10` → `4/11` 이고 아무것도 영속화하지 않는 상태에서 그것을 답하면
아이템이 영구히 사라진다. 호스트 쪽 저장소가 생긴 뒤의 다음 증분으로 남기고,
조리법은 위에 적어 뒀다.

#### `localserver_ab.rs` 의 계약은 지켰다

새 응답은 전부 `Session::offline` 뒤에 있다. `Session::new`/`with_grants` 는
여전히 여섯 프레임(`1/1 1/5 9/1 6/2 6/5 6/7`)만 답하고, 그것이 그 테스트가
"`hl5_server/server.py` 와 바이트 동일"을 계속 뜻하는 이유다. 스위치는
`Config::offline_menus`(프로브 `--offline-menus`, 안드로이드는 서버 미설정 시 자동).

#### §31.5 의 분류 정정

측정된 프레임은 §31.5 의 표와 다르다: 판매거래소 → **2/3**, 구매거래소 →
**4/7**, 우편함 → **8/3**(카테고리 3 이 아니다). 그리고 `netConnect(2)` 다이얼은
`StateMainMenu::KeyPressed` 가 아니라 **`StateMainMenu::Process+0x7B4`** 다.
카테고리 3(우편 본체)만은 `CreateMessagePopup` 이 아니라
`StateNetMenu::SetPopupState(0x25|0x26|0x27, msg)` 를 쓰는데, 우편함의 첫
프레임이 `8/3` 이므로 실무적으로는 닿지 않는다.

#### 리플레이에서 걸린 것 둘

* **창고접속은 세이브가 있어야 다이얼한다.** 빈 storage 로는 창고접속 →
  슬롯선택에서 `empty` 셋을 보여 주고 거기서 끝난다 — `netConnect` 조차 없다.
  그래서 `warehouse.txt` 는 **세이브가 이미 있는 storage 를 요구**한다
  (`npcmenu.txt` 를 같은 디렉터리에 먼저 돌린다). 인게임에서 타이틀로 돌아가는
  길이 없다(`key back` 은 캐릭터 시트를 연다).
* **타이틀 탭 뒤 첫 키 입력은 하이라이트를 움직이지 않는다.** 프레스마다 덤프를
  떠서 확인했다: 1번째는 게임시작 그대로, 2번째가 익스퍼트, 3번째가 창고접속.
  두 번만 누르면 **익스퍼트**에 들어가는데 그쪽 슬롯선택 화면이 창고접속의
  것과 거의 같고 패킷을 하나도 안 보내므로, 실수가 "핸들러가 안 걸린다"로 보인다.

### 32.8 ② `Config::speed` — 기본 1.0, 그리고 왜 프로브가 시험할 수 없는가

`ftime` 이 게스트의 유일한 벽시계원이므로 `Vm::now_ms` 한 곳에 계수를 곱하면
틱 구동과 시계 구동이 **같은 배율로** 빨라진다(§31.2). 넣었다:
`Config::speed`, 기기 스위치 `<files>/hl5_speed.txt`(0.1..=8.0 밖은 거절하고
로그에 남긴다), 기본 **1.0**.

기본값이 1.0 인 이유는 §31.9 그대로다 — 못 따라가면 틱 레이트는 안 오르는데
밀리초로 표현된 모든 양만 배가 된다. 다만 §32.6 이 여유를 만들었으므로 1.0
초과는 **측정 뒤** 켜는 실험으로 남는다.

**스케일은 `virtual_clock == false` 분기에만 있다.** 가상시계 모드에서는
호출자가 시계의 주인이고 `advance_time` 은 게스트 자신의 밀리초(예: `select`
타임아웃)를 먹으므로 거기서 곱하면 그것들이 망가진다. 결과적으로
**프로브는 출하되는 줄을 실행하지 못한다** — 그래서 시험할 수 있는 부분만
순수 함수 `scale_elapsed` 로 떼어 단위시험 셋을 붙였다: 1.0 은 옛 식과 글자
그대로 같음, 2시간 구간 단조성(f32 계수는 한 시간 뒤 ~0.25 ms 로 양자화된다),
말도 안 되는 계수는 포화. 정직한 인수 증거는 기기 로그의 `repaints/s` 가 실제로
`27.8 × speed` 에 닿는지 하나뿐이다.

기록해 둘 어긋남 하나: `stat` 은 게스트에게 파일의 **실제 `st_mtime`** 을 주고
(`files.rs` → `MC_fsFileAttribute` 0x14C0B8) 그 값은 스케일되지 않는다. 그것을
`ftime` 과 비교하는 게스트 코드는 없다.

### 32.9 ⑥ 가방에서 오브를 눌러도 아무 것도 안 뜬다 — 지급 표가 틀렸다

사용자 보고: 캐시샵에서 오브를 사고 가방에서 눌러도 사용하기·버리기 어느 것도
안 뜬다.

**기대 절반은 원래 그렇다.** `StateInGameMenu::KeyBagMenu`(0x1091E0)가 가방
메뉴를 여는 유일한 곳이고, `+0xA2C` 가 `ItemInfo+0x14`(kind)를 읽어 19항
점프테이블로 간다. **kind 12(오브)는 팝업 4 = 버리기 하나뿐이다.** 오브에
사용하기는 없다 — 오브는 엔지니어의 오브결합 화면에서 쓰고, 그 화면의 픽커는
`BagItem::SetItemFilter(6)` 이며 필터 6의 판정은 한 명령이다
(`SetItemFilter+0x3D0 cmp r1,#0xc`, kind 12 만).

**그런데 "아무 것도 안 뜬다"는 다른 증상이고, 그것은 호스트 버그였다.**

`item_18.dat` 레코드의 **끝에서 두 번째 바이트**가 사용 분류다.
`CashItemInfo::CopyData+0x68` 이 그것을 `ItemInfo+0x134` 에 두고,
`KeyBagMenu+0xAF4` 가 kind 18 에 대해 그 바이트로 분기한다: **2 → 팝업 4,
3 → 팝업 5, 그 밖에는 `bne` 로 빠져 팝업이 아예 없다.** 값은 49행을 정확히
가른다:

```
0  계정 서비스   창고확장(nt), 프리미엄판매, 별의 문장, 유료전환 …  가방 아이템 아님
1  랜덤 상자     작은오브원석, 오브원석, 하이퍼오브, 유물함 ×4
                 — 일곱 행 전부 자기 설명에 그렇게 쓰여 있다:
                   "랜덤 오브를 5개 얻습니다", "랜덤 아이템을 획득합니다"
2  다른 데서 소모  고급제련석, 부활의 부적, 공허 무기 …  → 버리기
3  가방에서 사용   엘릭서, 오토루팅, 혼 …  → 사용하기 / 버리기
```

**분류 1은 상점 진열품이지 지급물이 아니다.** 원래 서버가 굴려서 진짜 아이템을
보냈다. `shop.rs::GrantTable::build` 의 규칙 `(kind 18, index = 카탈로그의 a)` 는
30행 중 24행에 맞고 **6행에 틀렸다** — 그 여섯은 어느 화면도 받아 주지 않는
kind-18 토큰을 쥐여 준다. 가방은 툴팁만 그리고 `KeyBagMenu` 는 팝업 없이
돌아간다. 사용자가 산 하이퍼오브(카탈로그 id 7, `a`=7)가 정확히 그 경우다.

| 카탈로그 | 이름 | `a` | 분류 | 고친 결과 |
|---|---|---|---|---|
| 6 | 오브원석 | 6 | 1 | **kind 12, index 1..13 중 5개** (설명: 랜덤 오브 5개) |
| 7 | 하이퍼오브 | 7 | 1 | **kind 12, index 14..26 중 1개** |
| 18~21 | 유물함 ×4 | 15..18 | 1 | **거절** — 대상 테이블을 유도할 수 없다 |
| 46 | 공허의 목걸이 | 43 | 2 | **kind 10, index 17** (아래) |

`item_12.dat` 는 실측으로 0=빈소켓, **1..13 = 실제 오브 13종**, 14..26 =
하이퍼\*1~3, 27..39 = 2~4, 40..52 = 3~5.

**곁다리 버그 하나 더.** 공허의 목걸이는 분류 2라 메뉴는 뜨지만 kind 18 index 43
으로는 **영영 착용할 수 없다** — 게스트의 재사상(`ProcPacket_Shop+0x388`)이
index 44..48(공허 무기 다섯)만 덮기 때문이다. 대상은 유도된다:
`item_10.dat`(장신구) **17행이 공허의 목걸이**이고 그 테이블의 유일한 공허
장신구다. 옆의 다섯 무기가 선례다. (이것은 와이어에서 관측한 것이 아니라 표에서
유도한 것이다.)

굴림은 **`build()` 가 아니라 `records()` 에서** 한다 — 아니면 누가 사든 늘 같은
오브가 나온다. `Arc` 뒤라서 `AtomicUsize` 커서를 쓴다: 결정적이라 리플레이가
재현되고, `rand` 도 게스트의 `StaticUtil::Random` 도 건드리지 않는다(§31.6 의
LCG 와 A/B 가 거기 걸려 있다).

**실측 (같은 구매, `out/orbbug/real.txt`):**

```
이전:  SetPopupState 호출 0회 — 탭해도 아무 일도 없다
이후:  localserver: granting 1 record(s) for item=7
       StateInGameMenu::SetPopupState(…, 0x4, …) <- KeyBagMenu+0xaf0
```

화면(`out/orbbug/shot/R-5tap2.png`): 가방에 **하이퍼뇌제1~3** 이 들어 있고
**버리기** 메뉴가 뜬다.

단위시험 아홉을 붙였다(랜덤 상자가 진짜 아이템을 주는지, 연속 구매가 달라지는지,
계정 서비스가 거절되는지, 목걸이가 재사상되는지, `item_18.dat` 가 없으면 전부
거절하는지). 1,241 프레임 A/B 는 `free_shop` 이 꺼져 있으므로 그대로 통과.

**기기 확인 (2026-08-20, S25 릴리스 빌드).** 캐시샵에서 하이퍼오브를 사면
로그가 `localserver: granting 1 record(s) for item=7` 을 내고 가방에
**하이퍼뇌제1~3** 이 들어오며 누르면 **버리기** 가 뜬다 — §32.9 가 오프라인에서
예측한 그대로다.

> **그런데 고치기 전에 산 것은 되돌아오지 않는다.** 같은 폰의 다른 세이브에
> 남아 있던 `오브원석`·`하이퍼오브` 는 **kind 18 토큰 그대로** 저장돼 있어
> 지금 빌드에서도 눌러도 아무 메뉴가 안 뜬다. 지급 표는 **와이어에 나가는
> 새 레코드**를 고치는 것이고 세이브 안의 `ItemInfo` 는 손대지 않는다.
> 사용자에게는 "고쳤다는데 내 오브는 그대로다"로 보이므로 먼저 말해야 한다.
> (되돌리려면 세이브의 아이템을 재사상해야 하는데, 그것은 §23 이 (B)안을
> 기각한 것과 같은 이유로 하지 않는다 — 실패하면 가방이 깨진다.)

> **함정 하나 더: 가방이 꽉 차 있으면 지급이 조용히 사라진다.** 처음 시험한
> 세이브는 16/16 이었고, 서버 로그에는 `granting 1 record(s)` 가 남았는데
> 가방에는 아무것도 늘지 않았다. 팝업도 없다. 기기에서 지급을 확인할 때는
> **빈 칸이 있는 세이브**로 하라.

**유도할 수 없어서 남긴 것 둘**: 하이퍼오브가 세 밴드(1~3 / 2~4 / 3~5) 중
어느 것을 굴렸는지, 유물함이 어느 테이블을 굴렸는지. 둘 다 2011년 운영 DB 에
있었고 와이어에 관측 가능한 형태로 나온 적이 없다. 그리고 오브원석의 "5개"는
**한글 설명에만** 있다 — 카탈로그의 `count` 필드는 1이다.

### 32.10 ⑦ 화질 — 스캔라인 두 줄이 사라지고 있었다

세 갈래(호스트 확대 / 게스트 해상도·색심도 / 렌더러 자체)를 병렬로 조사했다.
**셋 다 같은 결론에 도달했다**: 눈에 보이는 결함은 호스트의 확대 한 곳이고,
게스트 쪽에는 회수할 것이 없다.

#### 먼저, 진짜 버그 — 모든 프레임에서 두 줄이 화면에 못 갔다

`gl.rs::gl_draw_body` 가 표본 표를 `(x - x0) as f32 / dw` 로 만들고 있었다.
이 쿼드는 1:1 이다(게스트가 `glViewport(w,h)` + `glOrthof(0,w,0,h)` 를 캔버스
크기 텍스처에 낸다). 그러면 항등사상이어야 하는데 아니었다: `(x/w)*w` 가 어떤
`x` 에서 아주 살짝 아래로 떨어지고, 절단 캐스트가 그것을 `x-1` 로 만든다.

* 모델: 240행 캔버스에서 {63, 125, 126, 127}이 어긋난다 → **63행과 127행은
  화면에 한 번도 안 간다.**
* 실측: 이 저장소의 프레임 덤프 **전부**에서 63행과 125행이 바로 위 행과
  바이트 동일하다. 서로 무관한 세 장면에서 확인.
* 기기에서는 4.5배라 4픽셀 띠 두 개가 사라진 셈이다.

고치는 법은 픽셀 **중심**을 표본하는 것이다(`(x + 0.5) / w`). 2..=1024 의 모든
크기에 대해 소스 줄이 정확히 한 번씩 쓰이는지 전수 시험을 붙였다.
**`f64` 로 바꾸는 것으로는 안 고쳐진다** — 정밀도가 아니라 펜스포스트다.

`--no-patch` A/B 는 양쪽이 같은 `gl.rs` 를 쓰므로 영향이 없다: 수정 뒤에도
**1,241 프레임 전부 바이트 동일**이고, 새 덤프에는 중복 행이 **0개**다.

#### 그 다음, 계단 — 픽셀아트 스케일러가 맞는 도구다

내용물은 2011년 피처폰 아트다: 4,795개 이미지의 **92.9%가 16색**, 최빈 스프라이트
20×20, 투명은 키 컬러 한 색, 글꼴은 1비트 8×11·16×11. 그것을 4.5배로 늘리면 모든
글자 획과 스프라이트 키 가장자리가 계단이 된다.

부드러운 필터는 이 입력에 틀린 도구이고, 그것을 **재 봤다**:

| | 없는 색 만들기 | 1px 획 최대값 | 계단 밀도(글자/게임플레이) | CPU ms/프레임 |
|---|---|---|---|---|
| nearest (기존) | 0% | 100% | 2.31% / 7.35% | 0.55 |
| **Scale2x** | **0%** | **100%** | **1.26% / 3.55%** | **0.20** |
| Scale3x | 0% | 100% | 0.79% / 2.24% | 0.50 |
| 바이리니어 | 24% / 61% | 92% | — | **15.3** |
| xBR 급 | — | — | — | **6.8** |

**Scale2x 를 넣었다** (`hl5_android`, `<files>/hl5_scale.txt` 로 끌 수 있고 기본
켬). 출력 픽셀은 전부 입력 넷 중 하나라 **없는 색을 만들지 않는다**(실측: 628색
→ 628색, 만든 색 0). 실제 프레임에서 화소의 3.90%만 바뀐다.

**S25 실측(타이틀, 같은 장면):** `blit` 239~248 → **359~374 ms/10 s**,
present 1,022~1,049 → 1,365~1,416 ms/10 s, 그리고 **fps 는 25.0 → 24.5~24.7 로
사실상 그대로.** 프레임당 0.5 ms 다.

#### 하지 않기로 확정된 것 — 다시 조사할 필요 없다

* **32비트 컬러는 죽었다.** 스위치는 `nativeInitDisplay` 의 `colorType` 하나이고
  (`setWIPIDisplayInfo` 가 `kernel+0x4F4` 를 0x10/0x20 으로 갈라 놓고
  `Graphic::Graphic+0x94` 가 그것을 `this+0x5C` 셀렉터로 옮긴다) 실제로 켜 봤다.
  결과: **스프라이트 블리터 여덟이 `bx lr`** — 타이틀 화면의 non-black 픽셀이
  **0 / 96,000**, 메인 메뉴가 65. 게임이 그림을 아예 안 그린다. 게다가 그 모드의
  `Graphic` 포맷은 5-5-5 라 565 **보다 나쁘다**. 폰트만 진짜 32비트 경로가 있다.
* **캔버스를 키우면 선명해지는 게 아니라 작아진다.** 아카이브의 에셋 세트는
  **하나**고(경로 리터럴 190개 전수, `_hd` 도 두 번째 트리도 없다), 글리프
  크기도 **11 px 하나**다(`eng.fnt` 8×11 95자, `kor.fnt` 16×11 582자, 둘 다
  1비트, 확대 코드 없음 — 굵게/외곽선만 있다). 실측: 밝은 픽셀 수가 400×240 에서
  2,471, 480×480 에서 2,478 — **잉크는 그대로고 사이만 벌어진다.** 비용은
  1022×472 에서 **3.12배**. 그리고 §27.10 이 추정한 **1024 텍스처 천장이
  관측됐다**: 1040 폭이면 `getWidth2n` 이 실패하고 프레임 전체가 흰색이 된다.
* **높이 사다리는 없다.** `cmp GetWidth(),#240/#320/#480` 열넷은 **전부**
  `bl Graphic::GetWidth` 바로 뒤이고, `GetHeight` 를 비교하는 곳은 0곳이다.
  그 사다리는 앵커만 고른다.
* **디더링은 지울 그라데이션이 없다.** 색 변환은 ≤256 엔트리 팔레트 위에서
  일어나고 이미 포스터화돼 있다.
* **에셋이 축소돼 그려지는 곳은 없다** — 회수할 화질이 없다.

#### 남은 선택지 (사용자 결정)

1. **캔버스 585×270** — 2340/585 = 1080/270 = **정확히 4.0배**. 지금 4.5배라
   열이 4·5 픽셀로 갈리는 것(260런 + 260런, 실측)이 없어진다. 게스트 +4.5%
   (부팅·인트로·메뉴 실측; 게임플레이·컷신 미측정). `Config::wipi_h` 를 프로브에
   노출해야 한다.
2. **캔버스 400×240 으로 되돌리기** — 반대 방향. 11 px 글꼴이 물리적으로
   **커진다**. 좌우 검은 띠. §27 이 미관을 이유로 일부러 버린 거래이므로 사용자
   판단이다.
3. **Scale3x** — 계단이 한 번 더 줄고(2.31→0.79%) 비용은 약 2배.
4. **EGL + GLES2 프리젠터** — present ~0, 셰이더 xBR 이 열린다. ~300줄.
5. **기기 WQHD+** — 코드 0줄, 배율 정확히 6.0, 패널 리스케일 없음, repaints −2.4%.

#### 조사 방법에 대한 메모

이 절의 세 갈래에는 적대적 검증을 붙였는데 **셋 중 셋이 개인 지출 한도로 실패
했다.** 그래서 위의 "하지 않기로 확정된 것"은 검증되지 않은 단일 조사다 — 다만
전부 **실행 실측**(32비트로 켜서 화면이 비었다, 캔버스를 키워서 픽셀을 셌다)이라
디스어셈블만 한 주장보다는 단단하다. 반면 §32.10 이 **구현한** 둘은 이 세션이
직접 확인했다: 스캔라인은 덤프에서 중복 행이 사라진 것으로, Scale2x 는 색이
안 늘어난 것과 기기 fps 로.

### 32.11 이 세션이 남긴 인수 기준

```
bash tools/ab_905.sh <tag>            # 5장면 1,241프레임, patched vs --no-patch
bash tools/ab_905.sh <tag> new        # 기준선을 재사용해 patched 쪽만 (빠르다)
cargo test -p hl5_core                # 32개 (§33 이 canvas 3, hud 1 을 더했다)
cargo run -q -p hl5_probe -- --selftest                       # 29개
cargo run -q -p hl5_probe -- --boot --free-shop --offline-menus \
    --storage <세이브 있는 dir> --script tools_re/scripts/warehouse.txt
```

`tools/ab_905.sh` 가 §18.4(오프라인)·§25.6(장면별 신규 storage·같은 순서)·
동시 `cargo run` 의 락 경합(전용 `CARGO_TARGET_DIR`) 셋을 다 못박고 있다.

## 33. 캔버스를 화면이 정수배로 확대하도록 맞춘다 (2026-08-20)

§32.10 이 화질의 측정을 다 끝내 놓고 선택만 남겨 뒀다. 그 선택을 했고, 그
전제였던 "높이를 움직여도 되는가"를 §27.8 과 같은 방법으로 못박았다.

### 33.0 먼저 기기 측정이 일의 순서를 바꿨다

S25 릴리스 빌드, 10초 창, `repaints/s`(= 게스트 자신의 재칠, §29.4):

| 장면 | `repaints/s` | 천장 27.8 대비 | guest / 10 s | present / 10 s | 호스트 루프 |
|---|---|---|---|---|---|
| 프롤로그 | 20.0 | (게스트가 55 ms 고정) | 1024 ms | 1658 ms | 80/s |
| 타이틀 | 24.4 | 88% | 5859 ms | 1396 ms | 49/s |
| **게임플레이** | **25.8** | **93%** | 4877 ms | 1822 ms | 56/s |

§29.4 의 마지막 실측이 "인트로 8~18 / 게임플레이 15~23" 이었으니 §32.6 의 블리터
넷이 기기에서도 값을 했다 — **게임플레이 15~23 → 25.8.**

**그리고 이 숫자가 §32.8 의 인수 기준을 지금은 통과 불가능으로 만든다.**
`Config::speed` 1.25 는 `27.8 × 1.25 = 34.75 repaints/s` 를 요구하는데 기기는
25.8 이다. §31.9 가 경고한 그대로 "못 따라가면 틱 레이트는 안 오르는데 밀리초로
표현된 모든 양만 배가 된다" — **레버와 아무 상관 없는 이유로 실패한다.**
곧 `-Speed` 빌드 스위치를 만들어 봐야 시험이 아니라 증상만 본다. 여유가 먼저다:

* present 가 repaint 당 **7.06 ms** (1822/258). repaint 당 총 38.8 ms 이고 천장은
  36 ms 이므로 **present 만 없애도 게임플레이가 천장에 붙는다** — 루프가
  단일 스레드라 present 시간이 그대로 더해지기 때문이다.
* 게스트는 repaint 당 **18.9 ms** (4877/258). 남은 블리터 셋
  (`FntDrawString` 26.2% + 4비트 xform 8 22.9% + `DrawImageMirrorClip565` 22.2%
  = 71.3%)을 거기 곱하면 **13.5 ms** 인데, **그 비율은 오프라인 프로브의 ingame
  프로파일에서 온 것이고 기기에서 잰 값이 아니다.** 자릿수를 잡는 용도로만 써라.

즉 순서는 **블리터 셋 → EGL 프리젠터 → 그 다음에 speed 시험**이고, 그것은
추론이 아니라 위 표에서 나온다.

### 33.1 높이 240 은 게스트 안에 없다 — 전수 조사

§27.2 는 폭 440/480/520 을 실측했고 §27.8 은 폭 400 을 전수 조사했다. **높이는
한 번도 240 에서 움직인 적이 없다.** 그래서 같은 조사를 높이에 했다.

**1. 게스트에는 높이 사다리가 자체가 없다.** 여섯 게터의 반환값이 그 뒤에서
무엇과 비교되는지 `.text` 전체에서 훑으면:

```
cmp Graphic::GetWidth(),  #240   10곳   DrawTouchCancelUi, DrawTouchInfoToggle,
                                        DrawSkillBelt, mainKeyPressed, DrawTouchUi,
                                        SetTouchKeypad, TouchPressed,
                                        TouchPropertyMenu, TouchSkillMenu, TouchBagMenu
cmp Graphic::GetWidth(),  #320    2곳
cmp Graphic::GetWidth(),  #480    2곳
cmp Graphic::GetHeight(), #상수   0곳   ← 없다
```

`GetHeight` 의 비교 14곳과 `FrameBufferInfo::GetHeight` 의 13곳은 **전부 다른
레지스터와의 비교** — 즉 런타임 클립이다. 폭에는 지켜야 할 분기가 셋 있고
(585 와 555 는 520 과 똑같이 480 위쪽이라 같은 칸에 떨어진다) **높이에는
지킬 분기가 없다.**

**2. 세로 배치는 게터로 한다.** `GetHeight` **310호출 / 89함수**,
`GetHalfHeight` **140호출 / 103함수** (가로는 314/93, 209/111).
`FrameBufferInfo::GetWidth` 52/29, `GetHeight` 37/27.

**3. `.text` 의 `#240` 270곳을 전부 분류해도 화면 높이는 0곳이다.**

```
196  주소지정        [pc,#0xf0] · [sp,#0xf0] · [rX,#0xf0] — 리터럴 풀·스택 슬롯·구조체 필드
 37  add / sub       포인터 산술 (LzmaDecode, BFont::DrawTextField, internalAuthCB …)
 14  mov             ↓
 13  비트마스크      and rX, rY, #0xf0 — 0xf0 은 바이트의 상위 니블
                     (Graphic::FillRectAlpha / ProcAlphaRGB / DrawRing, 픽셀 블렌딩)
 10  cmp             ↓
```

`cmp` **10곳은 전부 위의 폭 사다리**(`bl GetWidth` 직후)다. `mov` 14곳은:
**10곳이 `_ENEMY_G_DATA` 스트라이드**(전부 `mov ip,#0xf0` 바로 옆에 `mul` —
`Monster::setEnemyG`, `Map::MonsterAdd`, `Map::MonsterChange`, `Map::LoadData`,
`Map::LoadImage`, `Map::MapEnemyG_set`, `Map::MonsterResChange`), 2곳이 `memset`
길이(`NetMailList::InitAll`, `NetTradeMailList::InitAll`), 1곳이
`internalAuthCB`, 1곳이 HSB 색상각(`ConvertRGBtoHSB` 의 `moveq r3,#0xf0`).
**y 좌표나 높이로 쓰인 곳 0곳.**

**4. 그래서 이상적인 모양이 그대로 나온다.** `Graphic::FillRoundRect` 가 x 를
`FrameBufferInfo::GetWidth()`, y 를 `GetHeight()` 로 클립한다 — 상수가 아니다.

> **오탐 하나를 기록해 둔다.** 첫 조사에서 그 함수가
> `cmp GetHeight(), #255` 로 잡혔다. 실제로는 게터 반환 두 명령 뒤에
> `ldr r0,[sp,#0x2c]` 가 `r0` 을 **재정의**하고 그 비교는 알파 == 255 다.
> 게터 뒤를 따라가는 스크립트는 **추적 중인 레지스터를 쓰는 명령에서 반드시
> 끊어야** 한다. 안 끊으면 "높이 사다리가 있다"는 반대 결론이 나온다.

> **capstone 함정.** `Cs.disasm` 은 제너레이터라 **해독 못 하는 첫 워드에서
> 멈춘다.** 이 이미지에서는 `0x32a8c`, 즉 300,021 워드 중 **1,298개**(0.4%)만
> 읽고 끝난다. 실패 지점마다 다음 워드에서 재시작해야 99.7% 가 나온다.
> 재시작 없이 돌리면 위 표가 **전부 0** 으로 나오고, 그것은 "하드코딩이
> 하나도 없다"는 **정답과 구분되지 않는다.** §27.8 의 숫자
> (314/310/209/140)를 재현하는 것이 이 스크립트의 실질적인 자기검증이었다.

### 33.2 규칙은 "270" 이 아니라 "배율이 정수가 되는 캔버스"

상수를 박으면 S25 에서만 맞는다. `hl5_core::widened_canvas` 가 유도한다:

```
k       = max(1, floor(화면짧은쪽 / 240))     정수 배율
캔버스  = (화면긴쪽 / k, 화면짧은쪽 / k)      단, 폭이 [400, 1024] 를 벗어나면 사양
```

| 표시 | k | 캔버스 | 배율 | 그전 |
|---|---|---|---|---|
| **2340×1080 (S25)** | 4 | **585×270** | **4.0** | 520×240, 4.5 |
| 2220×1080 (S9) | 4 | 555×270 | 4.0 | 493×240, 4.5 |
| 1920×1080 | 4 | 480×270 | 4.0 | 426×240, 4.5 |
| 1280×720 | 3 | 426×240 | 3.0 | 426×240, 3.0 |
| 960×540 | 2 | 480×270 | 2.0 | 426×240, 2.25 |
| 800×480 | 2 | 400×240 | 2.0 | 400×240, 2.0 |

4.5 배는 캔버스 행 하나가 화면 행 넷에 앉았다 다섯에 앉았다 한다 — 최근접
확대가 놓을 수 없는 것이 정확히 반 픽셀이다. 4.0 이면 캔버스 픽셀 하나가 정확히
4×4 다. **Scale2x 를 앞에 두면 사슬 전체가 정수가 된다**:
`585×270 → 1170×540 → ×2.0 → 2340×1080`.

**사양이 있는 이유.** `긴쪽/k` 가 400 밑으로 떨어지는 화면에서 400 으로 클램프해
올리면 캔버스가 `긴쪽/배율` 보다 넓어져 그림이 화면 밖으로 밀린다. 그런 표시는
옛 분수 배율을 받는다 — 여전히 균일하고 여전히 옳으며 다만 픽셀 정확이 아니다.
`k` 가 짧은쪽을 정확히 나누지 않을 수도 있다(483 → k=2 → 241×2 = 482): 배율은
그래도 정확한 정수이고 남는 것은 **k-1 픽셀 미만**이다.

### 33.3 비용 — 기기에서는 사실상 0

같은 S25, 같은 릴리스 경로, 같은 세이브(건슬링어 Lv2), 10초 창:

| 장면 | 520×240 | **585×270** | guest / 10 s | present / 10 s | gl / 10 s |
|---|---|---|---|---|---|
| 프롤로그 | 20.0 | 19.7~20.3 | 1024 → 1472 ms | 1658 → 1735 ms | 778 → 934 ms |
| 타이틀 | 24.4~24.6 | 24.2~24.5 | 5859 → 5896 ms | 1396 → 1468 ms | 322 → 399 ms |
| **게임플레이** | 25.5~26.0 | **25.3~25.9** | 4877 → **4815** ms | 1822 → 1940 ms | 443 → 559 ms |

**`repaints/s` 는 세 장면 다 노이즈 안이다.** 캔버스가 26.6% 커졌는데 프레임
레이트가 안 움직인다. 늘어난 것은 `gl` 뿐이고(+26.2%, 픽셀 수가 늘어난 +26.6% 와 같은 값이다) 그것은
프레임의 5.6% 짜리 항목이다. 게임플레이의 게스트 시간은 오히려 1.3% 줄었다 —
장면 내용 차이지 개선이 아니다.

오프라인(프로브, `newgame` 전체)에서는 게스트 명령어가
**164,404,608 → 178,713,413 (+8.70%)**. 픽셀은 +26.6% 인데 명령어는 +8.7% 다 —
게스트의 일 대부분이 캔버스 전면 패스가 아니라는 뜻이고, 기기에서 프레임이 안
움직인 이유가 이것이다.

기기 부팅 로그: `canvas: surface 2340x1080 -> logical 585x270`,
`boot ok in 171037024 guest instructions`.

**보이는 것**: 메인 메뉴가 세로로도 가운데 정렬됐다(그전에는 위로 몰리고 아래가
비어 있었다). 게임플레이는 위아래로 장면을 더 보여준다. 글자와 스프라이트는
배율이 4.5 → 4.0 이므로 **11% 작아진다** — §27 이 미관을 이유로 한 거래와 같은
방향이고, 그것이 이 선택의 유일한 대가다.

### 33.4 인수 기준

1. **1,241 프레임 5장면 A/B 가 400×240 에서 그대로 통과할 것** — **통과.**
   119 + 543 + 134 + 109 + 336, `0 differing`. 프로브는 `--screen` 이 없으면
   `(WIPI_W, WIPI_H)` 를 쓰므로 이 변경이 닿지 않는다.
2. **같은 A/B 가 출하 캔버스 585×270 에서도 바이트 동일일 것** — **통과.**
   호스트 블리터가 585 폭의 스트라이드 산술을 처음 겪는데 1,241 프레임 전부
   동일하다. 덤프가 실제로 585×270 이고 로그가
   `hud: canvas 585 wide, in-game bar shifted +92 px` 를 낸 것으로 플래그가
   들어간 것을 확인했다. `tools/ab_905.sh` 에 `PROBE_FLAGS` 통로를 냈다:
   `PROBE_FLAGS="--screen 2340x1080" bash tools/ab_905.sh <tag>`.
   양쪽에 같이 가므로 비교가 재는 것은 여전히 패치뿐이다.
3. **단위시험** — `hl5_core` 28 → **32개**. 실제 표시 여섯의 캔버스·배율을
   못박고, 240..2160 × 다섯 종횡비를 훑어 (a) 캔버스가 항상 합법이고
   (b) `fit_screen` 의 `min` 배율에서 어느 축도 넘치지 않으며
   (c) 가로가 400 이상인 가로 화면은 레터박스가 **캔버스 한 픽셀 미만**이고
   (d) 정수 분기의 여유가 양축 모두 `k` 미만임을 확인한다.
   `hud.rs` 는 `dx` 가 **음수가 되지 않음**을 같은 훑기로 확인한다 — `dx` 는
   클램프가 없어서 400 보다 좁은 캔버스면 바를 왼쪽으로 밀고 엉뚱한 띠에서
   탭을 훔친다. 그것을 막는 것이 `widened_canvas` 의 400 바닥이고, 이 시험이
   그 연결을 지킨다.
4. **오프라인 재생이 폴트 없이 완주할 것** — `newgame` 585×270 에서 178.7 M
   명령어, 폴트 0, `MC_grpFlushLcd` 597회 / `gl` 1194회로 520×240 대조군과
   **같은 횟수**.

### 33.4.1 조사는 스크립트로 남아 있다

`tools_re/dimcensus.py` 가 위 A·B 표를 그대로 낸다 — 게터 호출 수, 반환값이
무엇과 비교되는지, 그리고 임의의 즉치가 `.text` 어디에 어떤 형태로 나타나는지.
**두 축 어느 쪽이든 다시 움직이기 전에 이것부터 돌려라.**

```powershell
python tools_re/dimcensus.py payload/libHeroesLore5.so --imm 240 400
```

배너의 커버리지가 **99.7%** 인지 먼저 볼 것. 낮으면 위의 capstone 함정에
걸린 것이고, 그때 나오는 "하드코딩이 하나도 없다"는 정답과 구분되지 않는다.

### 33.5 하지 않은 것과 그 이유

* **`-Speed` 빌드 스위치** — 릴리스 APK 는 debuggable 이 아니라
  `<files>/hl5_speed.txt` 를 adb 로 못 쓴다(`run-as: package not debuggable`).
  그래서 스위치가 필요하긴 한데, §33.0 이 보인 대로 **지금 시험하면 레버와
  무관하게 실패한다.** 여유를 만든 뒤에 붙인다.
* **Scale3x** — 계단이 2.31% → 0.79% 로 더 줄지만, 정수 배율이 된 지금
  Scale2x 사슬이 `585→1170→2340` 으로 정확히 맞물린다. Scale3x 는 1755 가 되어
  2340 과 다시 어긋난다. **정수성을 깨는 값을 그 자리에 넣지 마라.**
* **캔버스 400×240 되돌리기** — §27 이 없앤 좌우 검은 띠가 돌아온다.

## 34. EGL/GLES2 프리젠터 — 마지막 홉을 GPU 로 (2026-08-20, 원격 세션)

§33.0 이 1순위로 지목한 그 작업이다: S25 게임플레이에서 present 가 repaint 당
**7.06 ms**, repaint 총비용 38.8 ms, 게스트 천장 36 ms — 루프가 단일 스레드라
present 를 없애는 것만으로 게임플레이가 천장(27.8 repaints/s)에 붙는다는 산수.
그 7 ms 가 사던 것은 CPU 가 2340×1080 워드를 softbuffer 스테이징에 다시 쓰는
일이었고, 그것은 GPU 가 태어나서 하는 일이다.

**이 세션에는 기기가 없다. 아래에서 "됐다"는 전부 "코드가 들어갔고 타입이
맞는다"까지이고, 밀리초·fps 는 하나도 측정되지 않았다.** 측정 항목은 §34.4.

### 34.1 무엇을 바꿨고 무엇은 그대로인가

`hl5_android/src/egl.rs` (신규) + `lib.rs` 의 present 분기. **의존성 추가 0** —
필요한 심볼 수십 개를 시스템 `libEGL.so`/`libGLESv2.so` 에 직접 `#[link]` 로
선언했다(NDK 가 링크 스텁을 준다. 크레이트를 더 얹는 것은 이 프로젝트에서
얻는 게 없다 — 원본 앱도 GLSurfaceView 위에서 정확히 이 API 를 썼다).

그대로인 것 — 이것이 설계의 반이다:

* **콘텐츠 비교 present 스킵**(§29)과 **CPU Scale2x**(§32.10)는 손대지 않았다.
  이 모듈이 대체하는 것은 사슬의 **마지막 홉 하나**: "(2배 된) 캔버스를 창
  크기로 CPU 복사 + softbuffer 로 전달"이 "텍스처 한 장 업로드 + NEAREST
  풀윈도 쿼드"가 된다. 바뀐 프레임에만 불리는 것도 그대로다.
* **softbuffer 경로는 살아 있는 폴백이다.** EGL 초기화가 거절하거나 present 가
  실패하면 그 판에서 EGL 을 은퇴시키고(`want_egl` 영구 해제) 다음 프레임부터
  기존 CPU 경로로 돈다. 화면이 늦는 게임이지 안 나오는 게임이 아니다.

### 34.2 화질 계약이 어떻게 지켜지는가 — 코드가 아니라 규약으로

* **NEAREST 는 프래그먼트 중심 표본이다** — `floor((x + 0.5) * src / dst)`,
  §32.10 이 `gl.rs` 에 박은 것과 같은 픽셀 중심 규약이다. 단, 이것이 대체하는
  셸의 창 확대 루프는 **가장자리 표본**(`floor(x * src / dst)`)이었다. 두
  floor 는 **배율이 정수일 때 모든 픽셀에서 일치**하고 출하 캔버스는 §33 이
  정수를 보장하므로(585×270 → 1170×540 → ×2.0 → 2340×1080) 출하 그림은
  동일하다. **비정수 배율(분할 화면, 폴드)에서는 soft/egl 이 한 텍셀
  위상만큼 다를 수 있다** — 회귀가 아니라 알려진 차이다.
* **NPOT 합법 조건이 곧 화질 계약이다.** GLES2 코어에서 1170×540 같은 비2제곱
  텍스처는 CLAMP_TO_EDGE + 밉맵 없음 + NEAREST/LINEAR 일 때만 합법인데, 그
  셋이 정확히 우리가 원하는 설정이다.
* **알파 1.0 강제는 장식이 아니다.** 프레임버퍼는 `0x00RRGGBB` — 알파 바이트가
  0 이다. 알파 채널이 있는 EGL 컨피그에 그대로 얹으면 **완전히 투명한 창**이
  된다. 셰이더가 `vec4(t.b, t.g, t.r, 1.0)` 으로 스위즐하며 1.0 을 박는다
  (스위즐 자체는 GL_BGRA 확장을 피하는 무비용 우회다).
* **varying 은 highp.** S25 에서 UV 가 1/2340 씩 걷는데 mediump(fp16)의
  가수는 10비트다 — NEAREST 경계가 어긋나 열이 여기서 겹치고 저기서 빠진다.
  §32.10 펜스포스트와 같은 계열의 버그를 셰이더 정밀도로 다시 만드는 길이라
  `GL_FRAGMENT_PRECISION_HIGH` 관용구로 막았다.
* **디더 끔.** GLES 는 디더가 기본 ON 이고, 색을 만들지 않는다는 §32.10 의
  선택과 충돌한다.
* **`eglSwapInterval(0)`.** 페이싱은 셸의 FRAME 격자 소유다. vsync 대기를
  게스트가 도는 그 스레드에 되돌려 놓으면 이 모듈이 없앤 비용이 이름만 바뀌어
  돌아온다. 티어링은 없다 — SurfaceFlinger 는 통짜 버퍼를 합성하고, 28/s 미만
  present 대 120 Hz 라 BufferQueue 가 차지 않는다(§31.3).

수명: `EGLSurface` 는 액티비티와 함께 죽는 `ANativeWindow` 를 감싸므로
`suspended` 에서 통째로 파괴하고 `resumed` 에서 재구축한다(디스플레이 연결
포함 — 이 프로세스에서 EGL 을 말하는 곳이 여기뿐이다).

통계 줄은 모양을 지켰다: `blit` = 텍스처 업로드, `swap` = draw + swapBuffers,
`acquire` = 0. 두 경로의 10초 줄이 같은 자리끼리 비교된다.

### 34.3 곁에서 고친 것 셋

* **미뤄진 boot 를 아무도 다시 부르지 않았다.** `boot` 는 `resumed` 가 0×0
  서피스를 주면 "따라오는 Resized 를 기다린다"고 로그까지 남기며 미루는데,
  `Resized` 핸들러는 `boot` 를 부르지 않았다 — 그 경로로 들어가면 VM 이 영영
  만들어지지 않고 검은 화면으로 끝난다(모든 소비자가 `vm` None 에서 조기
  복귀한다). `boot` 는 멱등이고 크기를 재검사하므로 `Resized` 에서 부르는 한
  줄이 수리의 전부다. 이 세션의 적대적 리뷰가 잡았다 — 디프가 만든 버그가
  아니라 디프가 스친 기존 버그다.
* **`Resized` 가 재그리기를 스스로 막고 있었다.** 리사이즈 핸들러는 "버퍼
  내용이 미정의니 지금 present 한다"고 적어 놓고 present 를 불렀는데, §29 의
  콘텐츠 비교가 그 present 를 "이미 보여준 프레임"으로 스킵했다 — 참이었던
  것은 옛 지오메트리지 새 버퍼가 아니다. `last_fb` 를 비우고 부르도록 고쳤다
  (양 경로 공통).
* **`build.rs` 가 `HL5_BOOT` 을 추적하지 않았다.** `-Boot` 만 바꾼 재빌드가
  낡은 오브젝트를 조용히 재사용했다 — `-Server` 를 위해 만든 장치가 정작 둘째
  스위치에 안 붙어 있었다. 이번에 `HL5_BOOT`/`HL5_SPEED`/`HL5_PRESENT` 를 전부
  등록했다.

### 34.4 스위치와, 기기에서 재야 할 것

빌드: `-Speed 1.25`(§33.5 의 그 스위치), `-Present soft`. 파일:
`<files>/hl5_present.txt` = `egl`(기본) | `soft`. 릴리스 APK 는 debuggable 이
아니라 파일을 못 쓰므로 빌드 스위치가 출하 빌드의 유일한 레버다.

기기에서 잴 것 — 전부 아직 안 쟀다:

1. **부팅 로그에 `presenter: EGL/GLES2 on <GPU>`** 가 나오는가. 안 나오고
   `EGL presenter unavailable` 이면 폴백으로 돌고 있는 것이다 — 그것도 로그에
   이유가 남는다.
2. **10초 줄**: 게임플레이 `repaints/s` 가 25.8 에서 천장 27.8 로 붙는가,
   `blit`(업로드)·`swap` 이 얼마인가. §33.0 의 산수가 예측이고 이것이 측정이다.
3. **같은 장면 soft/egl 스크린샷 픽셀 대조.** **정수 배율에서는** 같아야
   한다(§34.2 — 비정수 배율은 한 텍셀 위상 차이가 정상이다). 물론 이
   저장소에서 "같아야 한다"는 재기 전 단계의 문장이다.
4. **suspend/resume 왕복**(홈 → 복귀, 화면 끔/켬, 회전 잠금 상태 확인) 뒤에도
   그림과 터치가 맞는가 — 서피스 수명 관리가 §34.2 대로 도는지는 이것으로만
   보인다.
5. **`Config::speed` 시험은 이 다음이다** — §33.0 의 순서 그대로, 프리젠터가
   여유를 실제로 만들었는지 2번으로 확인한 뒤에 `-Speed 1.25` 를 시험한다.

### 34.5 이 세션이 하지 않은 것

* **남은 블리터 셋**(§32.6 꼬리: `FntDrawString`/4비트 xform 8/`Mirror`) —
  게스트 디스어셈블과 A/B 재생이 필요한데 이 환경에는 `payload/` 가 없다
  (라이선스 데이터는 저장소 밖). 기기까지 있는 세션의 몫.
* **창고 입출고**(§32.7) — 4/6 목록의 조리법은 적혀 있지만 입고 `4/10` 의
  요청 페이로드와 `4/11` 응답, 출고 서브커맨드는 디스어셈블리로만 확정된다.
  형식을 추측으로 박는 것은 이 저장소가 §25 에서 두 번 당한 그 함정이고,
  "넣을 수는 있는데 잊어버리는 창고는 닫힌 창고보다 나쁘다"(§32.7)는 결정을
  뒤집을 근거도 없다.
  빈 그리드만 여는 것(4/6 만 답하기)도 지금의 정직한 팝업보다 나쁘다고 판단해
  거절 그대로 두었다.

## 35. 기기가 §34 의 예측을 반증했다 — 그리고 진짜 천장 셋 (2026-08-21, S25 실측)

§34 는 코드만 있고 밀리초는 하나도 없는 절이었다. 이 절은 **갤럭시 S25 Ultra
실물**에서 그 다섯 항목을 전부 재고, 그중 하나가 **틀렸다는 것**을 확인하고,
틀린 이유를 좇아 세 개의 다른 천장을 찾아 치운 기록이다.

측정은 전부 **벤치 패키지** `kr.heroeslore.five.bench` 로 했다 — 디버그 서명이
릴리스 설치본(=사용자의 세이브)을 제자리 업데이트할 수 없어서, `debug` 빌드타입에
`applicationIdSuffix = ".bench"` 를 붙여 **나란히** 설치했다. 부수 효과가 더
컸다: 벤치는 debuggable 이라 `run-as` 로 `<files>/hl5_*.txt` 를 쓸 수 있고,
그래서 이 절의 스위치 훑기 열 몇 번이 재빌드 0회로 끝났다.

### 35.0 §34.4 다섯 항목의 결과

| # | 항목 | 결과 |
|---|---|---|
| 1 | 부팅 로그의 프리젠터 줄 | **통과.** `presenter: EGL/GLES2 on Adreno (TM) 830` |
| 2 | 게임플레이 `repaints/s` 25.8 → 27.8 | **실패 — 예측이 틀렸다.** §35.1 |
| 3 | soft/egl 픽셀 대조 | **통과.** §35.2 |
| 4 | suspend/resume 왕복 | **통과, 그리고 버그 둘을 드러냈다.** §35.5 |
| 5 | `Config::speed` | **통과 — 처음으로.** 1.25 에서 33.6 repaints/s (§35.6) |

### 35.1 present 를 68% 없앴는데 게임 속도가 안 움직였다

같은 빌드 안에서 `hl5_present.txt` 만 뒤집어 잰 게임플레이(같은 세이브, 같은
걷기 스크립트, 네 조합 교대 측정):

| 프리젠터 / 핀 | `repaints/s` | guest ms/10s | present ms/10s |
|---|---|---|---|
| soft, 핀 없음 (출하 상태) | 24.4 | 5,096 | **1,528** |
| soft, 프라임 핀 | 24.6 | 4,491 | 1,449 |
| egl, 핀 없음 | 23.8 | 6,164 | **676** |
| egl, 프라임 핀 | 23.7 | 4,755 | **492** |

**present 가 1,528 → 492 ms 로 줄었는데 `repaints/s` 는 네 줄 모두 ~24 다.**
§33.0 의 산수("present 만 없애도 천장에 붙는다")는 present 가 유일한 초과분이라는
전제 위에 있었고, 그 전제가 틀렸다. 벽시계의 **절반이 놀고 있는데도** 게스트
틱이 안 오른다면 그것은 CPU 천장이 아니다.

> **교훈으로 적어 둘 것: "A 를 줄이면 B 가 오른다"는 A 가 병목일 때만 참이고,
> 병목인지는 A 를 줄여 보기 전에는 모른다.** §34 는 그 검증을 다음 세션에
> 미뤘고, 미룬 것이 옳았다 — 다만 결론은 반대로 나왔다.

### 35.2 (§34.4 항목 3) soft 와 egl 은 애니메이션 말고는 같다

정지 화면이 필요해서 **월드맵**을 썼다(게임플레이는 걷지 않으면 present 가 전부
스킵되고, 걸으면 매 프레임이 다르다). 같은 세이브에서 두 번 부팅해 같은 자리에서
찍었다:

* egl vs soft: 2,527,200 픽셀 중 **7,532개(0.298%)** 가 다르고, 전부
  `x 976..1363, y 268..475` 한 상자 안이다. 다른 4x4 블록 524개 중 **421개가
  16/16 서브픽셀 전부** 다르다 — 표본 위상 차이라면 얇은 1픽셀 이음매가 화면
  전체에 퍼져야 한다.
* **같은 프리젠터(soft)로 1초 간격 두 장**: **7,620개(0.302%)**, 경계상자
  `x 976..1363, y 268..475` — **똑같은 상자.**

즉 그 상자는 퀘스트 `?` 표시와 깜빡이는 위치 점이고, 그 밖의 99.7% 는 두 경로가
**동일**하다. §34.2 의 "정수 배율이면 픽셀 동일"이 실측으로 섰다.

### 35.3 첫째 천장 — 스케줄러가 프라임 코어를 안 쓴다

앱 스레드 전부의 `task/*/stat` 39번 필드를 70초 x 4회 초당 표본:
**cpu 6·7 에 올라간 표본이 하나도 없다.** 그 둘은 1,017 MHz 바닥에서 놀고 있었다.

| | 퍼포먼스 클러스터 | 프라임 클러스터 |
|---|---|---|
| `cpuinfo_max_freq` | 3,532 MHz | **4,473 MHz** |
| 같은 셸 루프, 두 번 | 350 / 352 ms | **197 / 200 ms** |

**1.77배** — 클럭비 1.27배보다 크다(코어가 더 넓다). `top-app` 의 cpuset 은
`0-7` 이므로 막혀 있는 것이 아니라, EAS 가 듀티 60% 짜리 스레드를 올릴 이유를
못 찾은 것이다. 에너지로는 옳고 지연으로는 틀린 배치다.

`hl5_android/src/affinity.rs` 가 부팅과 매 2초마다 `sched_setaffinity` 로
**가장 빠른 클러스터**(하드코딩이 아니라 `cpuinfo_max_freq`/`related_cpus` 에서
유도)에 자기 스레드를 묶는다. 스위치는 `<files>/hl5_affinity.txt` = `prime`(기본)
| `off`, 빌드는 `-Affinity`.

* **`taskset` 으로는 못 한다.** `run-as` 로 앱 uid 가 돼도
  `Permission denied` — SELinux 는 `setsched` 를 **대상** 프로세스의 도메인으로
  거르고 셸 도메인은 그것이 아니다. 자기 자신에게 거는 것만 항상 허용된다.
* **한 번만 걸면 안 된다.** 홈에서 돌아오면 `not pinned (sched_setaffinity(6-7)
  refused)` 가 찍힌다. `resumed` 는 프로세스가 아직 `background` cpuset
  (`0-1,4-5`)에 있을 때 도착하고, 교집합이 비어 커널이 거절한다. 2초마다 다시
  거는 것이 그래서 필요하다(실측: 02:13:34 거절 → 02:15:16 복구).

효과: 타이틀 장면 guest 6,400 → **5,330 ms/10s (-17%)**.
그런데 `repaints/s` 는 여전히 안 움직였다 — 둘째 천장이 있었다.

### 35.4 둘째 천장 — 12 ms 격자가 실제로는 15.4 ms 였다

§32.5 는 `ceil(36/12)*12 = 36` 으로 12 ms 를 골랐다. 그 산수는 **루프가 요청한
시각에 깨어난다**는 전제를 쓴다. 그것을 재려고 `wake_due` / `t_late` 를 넣었고
(10초 줄의 `late`), 답은:

```
late 2,243 ms per 10 s  /  655 iterations  =  반복당 3.4 ms 늦게 깨어난다
```

즉 실효 격자는 15.4 ms, 게스트의 36 ms 마감은 `ceil(35/15.4)*15.4 = 46 ms`
로 반올림돼 **21.7~24 repaints/s**. 절반이 놀면서도 안 오르던 이유가 이것이다.

고정 격자를 훑으면(게임플레이, egl+핀):

| `hl5_poll.txt` | `repaints/s` | ticks/s | CPU |
|---|---|---|---|
| 12 (그때의 기본) | 23.4 | 65 | 54% |
| 6 | 25.8 | 101 | -- |
| 4 | 26.6 | 147 | 64% |
| **2** | **27.4** | **306** | **69%** |

2 ms 는 설계 천장 27.8 에 닿지만 **초당 318번 깨어나고**, 웨이크업 한 번이
이벤트루프를 통과하는 데 ~0.8 ms 의 CPU 를 쓴다 — 코어의 15포인트를 게임 속도
14% 와 맞바꾸는 거래다.

**그래서 격자를 촘촘히 하는 대신 겨눴다** (`App::plan_next_poll`):
게스트는 매 게임 틱마다 `MC_grpFlushLcd` 를 부르므로 틱 간격은 **관측 가능**하다.
관측 간격의 EWMA 를 두고, 다음 틱이 임박할 때까지(`interval - 8 ms`) 굵게 자고
그 다음부터만 2 ms 로 훑는다. 게스트가 아예 재칠을 멈추면 그대로 고정 2 ms 격자로
퇴화하므로 **실패 모드가 비용이지 정지가 아니다**.

| | `repaints/s` | ticks/s | `late` | CPU |
|---|---|---|---|---|
| 고정 2 ms | 27.4 | 306 | 3,873 ms | 69% |
| **겨냥 (기본)** | **27.2** | **134** | **26 ms** | **65%** |

웨이크업이 58% 줄고 `late` 가 3,873 → 26 ms 로 무너진다. 프롤로그(게스트가 틱을
55 ms 로 고정하는 구간, §16.6)에서도 EWMA 가 따라간다 — 상수를 안 박은 이유가
그것이다.

같은 변경으로 **`FRAME` 격자를 웨이크업에서 뺐다.** 프레임버퍼는 폴 안에서만
바뀌므로 다른 시각에 깨어나 보는 것은 아무것도 못 찾는 웨이크업이다. `FRAME` 은
비율 제한으로만 남았다(게스트 27/s 대 화면 60/s 라 실제로 걸리지 않는다).

### 35.4.1 셋째 천장 — 호스트가 같은 그림을 초당 300번 다시 합성했다

폴을 촘촘히 하면 `gl` 이 486 → 1,400 ms/10s 로 늘었다. `updateScreen` 이
**폴마다** `glDrawArrays` 를 내고 `hl5_core::gl` 이 그때마다 캔버스 전체를
합성하기 때문이다 — 그런데 게스트는 자기 36 ms 타이머에서만 다시 그린다. 즉
**열에 아홉은 직전과 같은 그림을 다시 만들고 있었다.**

`gl.rs` 에 두 가지를 넣었다:

* `tex_gen` — 텍스처 업로드(`glTexImage2D`/`glTexSubImage2D`/`glDeleteTextures`)
  마다 올라가는 세대 카운터. 이 크레이트가 텍스처 픽셀의 **자기 사본**을 들고
  있고 쓰는 곳이 그 셋뿐이라 정확하고 공짜다.
* `DrawKey` — 픽셀 루프가 읽는 전부(바운드 텍스처, 세대, 목적지 사각형, 텍스처
  좌표 경계, 채움색, fb 크기). 직전 드로우와 같으면 **`fb` 에 이미 답이 들어
  있다**. 정점/텍스처좌표 배열은 게스트 메모리라 GL 호출 없이 바뀔 수 있으므로,
  포인터가 아니라 **이미 계산된 사각형과 경계**를 키에 넣는다.

그리고 `fb_gen`(픽셀 루프가 실제로 돈 횟수)을 셸의 present 판정에 연결했다.
그전에는 `gl.draws` 가 폴마다 오르므로 필터가 통과하고, 그 뒤의 **전체
프레임버퍼 비교**가 매번 돌았다.

| (게임플레이, 폴 12 ms) | 이전 | 이후 |
|---|---|---|
| `gl` | 486 ms/10s | **141~169 ms** |
| present | 535 ms | 366 ms |
| `skipped`(프레임버퍼 비교로 걸린 수) | 180 | **0** |

**인수 기준**: `tools/ab_905.sh` 의 다섯 장면 **1,241 프레임**을 이 변경 전
(4188dbc)과 후로 각각 찍어 대조 — `newgame 119 / ingame 543 / shop 134 /
online 109 / npcmenu 336`, **전부 바이트 동일, 0 differing**. 표준
patched-vs-`--no-patch` A/B 도 1,241 프레임 전부 동일하고, `--selftest` 29개
통과.

### 35.5 (§34.4 항목 4) 왕복은 통과했고, 그 김에 배터리 버그가 나왔다

홈 → 화면 끔 → 화면 켬 → 잠금해제 → 복귀 뒤에도 그림이 맞고, 가운데로 옮긴
인게임 바의 `map` 탭이 그대로 먹는다(§28 의 터치 역변환이 새 EGL 서피스에서도
산다). 그런데 **홈으로 나간 동안 CPU 를 재 보니**:

```
포그라운드   67% of one core
홈으로 나감  84% of one core      <- 더 많다
```

프레젠트가 없어서 루프가 **더 빨리** 도는 것이다. `suspended` 는 오고
`vm.pause()`(게스트의 `nativePauseApp`)도 불리지만, 게스트의 타이머가 도는 것은
셸이 `nativeLoop` 을 계속 부르기 때문이다 — 안 보이는 게임이 안드로이드가
프로세스를 죽일 때까지 계속 진행된다. `asleep` 플래그로 `about_to_wait` 를
`ControlFlow::Wait` 에 세웠다:

```
홈으로 나감  0%      화면 끔  0%      복귀 66%
```

CLAUDE.md 가 "화면이 꺼져 있으면 게스트는 그동안에도 25 M ins/s 를 태운다"고
적어 둔 그것이고, 이제 안 태운다.

### 35.6 (§34.4 항목 5) `Config::speed` 가 처음으로 시험을 통과했다

§33.0 은 "블리터와 프리젠터로 여유를 만들기 전에는 레버와 무관한 이유로
실패한다"고 못박았다. 여유가 생긴 뒤 `hl5_speed.txt = 1.25`:

```
33.6 repaints/s   (목표 27.8 x 1.25 = 34.75 의 96.7%),  CPU 68%
```

**곱셈이 일어난 것을 본 게 아니라 틱 레이트가 실제로 따라 올라간 것을 봤다** —
§32.8 이 요구한 바로 그 시험이다. 출하 기본값은 1.0 그대로다.

### 35.7 처음과 끝 — 같은 바이너리, 같은 장면, 같은 세션

`hl5_present.txt`/`hl5_affinity.txt`/`hl5_poll.txt` 로만 갈라 재고, 입력은 아예
넣지 않았다(터치 주입 자체가 CPU 를 쓴다):

| | 출하 상태 (soft, 핀 없음, 고정 12 ms) | 이 세션 이후 (egl, 프라임 핀, 겨냥 폴) |
|---|---|---|
| **`repaints/s`** | 23.5~24.3 | **27.2** (설계 27.8) |
| **CPU** | **72%** of one core | **66%** |
| present / 10 s | 1,602 ms | 440 ms |
| `gl` / 10 s | 240 ms | 170 ms |
| 백그라운드 CPU | 84% | **0%** |

**게임이 13% 빨라지면서 CPU 를 6포인트 덜 쓴다.** 타이틀 장면도 같은 방향이다
(23.8 → 27.2).

> 대조군 쪽에도 `gl` 생략(§35.4.1)이 이미 들어 있으므로, 진짜 출하본과의 차이는
> 위 표보다 조금 더 크다. 같은 바이너리로 재는 쪽을 택한 것은 그 편이 **무엇을
> 재고 있는지 말할 수 있기 때문**이다.

### 35.8 측정하다 걸린 함정 넷 (전부 한 번씩 값을 치렀다)

* **`-Install` 이 매번 옛 릴리스 앱을 띄운다.** `build_android.ps1` 은
  `kr.heroeslore.five` 를 `am start` 하는데, 그 빌드에는 §35.5 의 수정이 없어서
  백그라운드에서 코어 하나를 태운다. 게임플레이 A/B 네 줄이 그 상태로 측정됐고,
  10초 줄에 **다른 프로세스의 통계가 섞여 나오는 것**(`heap 1637 KB`, `0 draws/s`)
  으로 겨우 알아챘다. 벤치 스크립트는 이제 **둘 다** force-stop 한다.
* **죽은 캐릭터는 정지 화면이다.** 레벨 1 워리어는 고블린 사이에서 몇 초면
  죽고, 부활 모달은 완전히 정적이라 present 가 전부 스킵되어 `0.0 fps` 가 된다.
  코드 회귀로 읽기 딱 좋다. 벤치는 부활 → **무리에서 벗어난 뒤** 측정한다.
* **고정 격자의 "12 ms" 는 틱 *시작* 기준이다.** 겨냥 폴을 넣으면서 재무장을
  틱 *종료* 기준으로 바꿨더니, 대조군(`hl5_poll.txt = 12`)이 초당 33폴이 아니라
  24폴이 됐다 — 다른 실험이 돼 버린다. `plan_next_poll` 이 고정 분기에서만
  `started + poll` 을 쓰는 이유다.
* **잠금 화면은 `NotificationShade` 라는 이름으로 나온다.** `mCurrentFocus` 가
  그것이면 우리 액티비티는 포커스가 없고, 그 상태의 측정은 전부 0 이다.
  `dumpsys window | grep isKeyguardShowing` 으로 먼저 확인할 것.

### 35.9 이 세션이 하지 않은 것

* **남은 블리터 셋**(`FntDrawString` / 4비트 xform 8 / `Mirror`, §32.6 꼬리) —
  §35.3~35.4.1 이 게스트 시간을 이미 크게 줄였고, 게임플레이는 이제 게스트
  천장에 붙어 있어서 **지금 이식해도 `repaints/s` 는 안 오른다**(오르는 것은
  `Config::speed` 를 올렸을 때의 여유다). 순서가 §33.0 때와 달라졌다.
* **`late` 3.4 ms 의 출처** — winit/android-activity 의 `ALooper_pollOnce`
  경로 어딘가다. 겨냥 폴이 그것을 *우회*했을 뿐 없애지는 않았다.
* **S9 재측정** — 사용자 지시로 이번 세션은 S25 만 만졌다.

## 36. 공용창고가 오프라인으로 돈다 (2026-08-21)

§32.7 은 창고관리를 **일부러** 거절했다: "넣을 수는 있는데 잊어버리는 창고는 닫힌
창고보다 나쁘다". 그 판단은 옳았고, 이 절은 그 전제를 없앤 기록이다 — 호스트 쪽
저장소가 생겼으므로 거절을 걷어냈다.

**S25 실물에서, 서버도 인터넷도 없이**: 창고가 열리고, 가방의 총을 올리면
창고 0번 칸에 들어가고, 다시 내리면 **같은 72바이트가 가방으로 돌아온다.**

```
localserver: 4/6 warehouse listing — 0/16 slots used
localserver: 4/7 warehouse money -> 0
localserver: 4/10 deposit -> slot 0 (72 bytes)
localserver: 4/11 committed
localserver: 4/6 warehouse listing — 1/16 slots used
localserver: 4/3 withdraw slot 0 x1 (72 bytes)
localserver: 4/6 warehouse listing — 0/16 slots used
```

### 36.1 카테고리 4 의 와이어 — 전부 디스어셈블에서 나왔다

`NETWORK::PacketWrite_Inventory` (0xC9BB8, 요청) 과
`NETWORK::ProcPacket_Inventory` (0xCACA8, 응답) 두 함수가 전부를 정한다. 요청
쪽은 `sub r7,r7,#1` / `cmp r7,#9` 로 **1..10**, 응답 쪽은 `cmp r1,#0xb` 로
**1..12** — 그리고 둘 다 `addls pc, pc, rX, lsl #2` 라 **case 0 이 pc+8**(§31)이다.

| sub | 방향 | 요청 | 응답 | 다음에 클라이언트가 하는 일 |
|---|---|---|---|---|
| **4/6** | 목록 | 없음 | `[i32 result][blob msg][i32 count]` + count×`[i32 slot][u8 state][_NET_ITEM_]` | **스스로 4/7 을 보낸다** |
| **4/7** | 화폐 | 없음 | `[i32 result][blob msg][i32 money]` | — |
| **4/10** | 입고 | `_NET_ITEM_` (writeNetItem) | `[i32 result][blob msg]` | 가방에서 지우고 저장 → **4/11** |
| **4/11** | 커밋 | 없음 | `[i32 result][blob msg]` | **4/6** 재요청 |
| **4/3** | 출고 | `[i32 slot][i32 count]` | `[i32 result][blob msg][_NET_ITEM_]` | 가방에 넣고 저장 → **4/6** |
| **4/9** | 아이템정리 | 없음 | `[i32 result][blob msg]` | **4/6** 재요청 |
| **4/5** | 버리기(창고) | `[i32 slot][i32 count]` | `[i32 result][blob msg]` | **4/6** 재요청 |
| **4/2** | 화폐 환전 | `[i32 amount]` | `[i32 result][blob msg]` | 소지금 −= amount → **4/7** |
| 4/1, 4/8, 4/12 | — | | | 창고 UI 가 보내지 않는다 |

`slot` 은 `(i8)NETWORK[0x7D1] + 16 × (i8)NETWORK[0x7D0]` = 칸 + 16×페이지, 즉
평평한 색인이다. 수량은 `NETWORK[0x7D4]`.

세 가지가 처음 읽을 때 반드시 걸린다:

* **`4/6` 의 `count` 는 레코드 수이면서 용량이다.** 그 arm 은
  `SetSize((short) count)` 를 부른 다음 정확히 `count` 개를 읽는다(0xCB2E8,
  0xCB35C). 그래서 "용량 16, 레코드 1개"는 표현할 수 없다 — 16칸 창고에 물건
  하나면 **레코드 16개**, 열다섯은 빈 것이다.
* **16 미만이면 그리드가 통째로 비어 보인다.** `DrawStorageUI` 가 16칸 단위로
  페이지를 세고 탭을 최대 다섯 개 그린다. `warehouse::DEFAULT_CAPACITY` 는
  그래서 **80**(다섯 페이지) — 사설 창고에는 용량을 팔 상대가 없으므로 상한이
  곧 출발점이다. 옛 파일의 `capacity 16` 은 불러올 때 **위로만** 늘린다:
  빈 칸을 덧붙이는 것은 공짜이고, 줄이는 것은 아이템을 고립시키는 유일한
  연산이라 아예 제공하지 않는다.
* **`4/6` 의 성공 경로 끝이 `WriteData(4, 7)` 다** (0xCB364). 목록만 답하고
  멈추면 클라이언트가 자기가 보낸 프레임의 답을 기다린다. 예전 세션이 `4/7` 을
  구매거래소의 것으로 적어 두었는데, 그것이기도 하지만 **먼저 창고관리를 여는
  두 번째 절반**이다.

### 36.2 호스트는 아이템을 이해하지 않는다 — 그게 설계다

`hl5_core/src/warehouse.rs` 의 한 칸은 **`_NET_ITEM_` 와이어 바이트를 그대로**
들고 있다. 파싱된 아이템이 아니다.

근거는 대칭성이다. `writeNetItem(fd, ItemInfo*, count)` (0xC8854)가 쓰는 필드와
`readNetItem` (0xC8578)이 읽는 필드가 **같은 순서, 같은 폭**이다:

```
[i32 count][i8 kind][i8 index][u32 len][len bytes 이름]
kind > 10 이면 여기서 끝  (짧은 형식 = 10 + len)
그 밖에는 [i64]×3 [i16]×2 [i8]×2 [i16]×3 [i8]×4 [i8]×4 [i8]×6 [i8]×4 [i16]×2
                                                         = 정확히 58바이트
```

그 58바이트는 장비 스탯·소켓·제련 상태이고 창고는 그중 아무것도 바꾸지 않는다.
다시 인코딩하려면 폭 열여덟 개를 다 맞혀야 하고, **하나가 어긋나면 그 뒤 전부가
밀린다** — §25 가 두 번 값을 치른 실패 방식이다. 바이트를 들고 있으면 그 위험이
정의상 없어진다.

> **그 결정이 곧바로 값을 했다.** 이 세션의 적대적 검증이 입고 arm 에서
> 못 보고 지나칠 뻔한 것을 찾았다: `kind <= 10` 이고 아이템의 `+0x138`/`+0x140`
> 이 둘 다 0이면 클라이언트가 **`gv+0x30`/`gv+0x38`(로그인 잔액 상태)을 그 두
> 64비트 칸에 찍어 넣고** 보낸다(0xC9C94..0xC9CA8). "가방 아이템을 그대로
> 보낸다"로 모델링한 서버는 와이어와 안 맞는다. 바이트를 보관하는 서버는
> 그것을 알 필요조차 없다.
>
> 그 검증은 같은 문장의 다른 절도 반증했다: `BagItem::GetBagItemPtr(a, s)` 는
> `bag[a*16+s]` 가 아니다 — 색인을 부호 있는 16비트로 자르고 **음수면 NULL 을
> 돌려준다**(0x9F69C의 `movlt r0,#0`), 그리고 게스트 자신의 다른 호출자들은
> 전부 그 NULL 을 검사한다.

파일은 `<files>/hl5_warehouse.txt` — 사람이 읽는 텍스트, 페이로드만 hex:

```text
# hl5 warehouse v1
capacity 16
money 0
slot 0 state 0 item 0000000102110000000531b1e7…
```

쓰기는 임시파일 후 rename 이라, 중간에 죽어도 반쪽 창고가 남지 않는다.
슬롯 줄이 없는 칸은 빈 칸이고, 못 읽는 줄은 **그 줄만** 버린다(한 줄 때문에
여든 칸을 잃는 것이 더 나쁘다).

### 36.3 아이템을 잃지 않게 하는 규칙 셋

1. **꽉 찬 창고는 거절한다.** 게스트는 `result == 0` 을 보는 즉시
   `BagItem::DeleteBagItemCategoryIndex` 로 자기 사본을 지운다(0xCADA0). 그래서
   "예"는 **넣을 자리가 있을 때만** 해야 하고, 판정이 응답을 만들기 *전*에
   끝나야 한다. `Warehouse::deposit` 이 `None` 을 돌려주면
   `창고에 빈 자리가 없습니다.` 가 게스트 자신의 팝업으로 뜬다.
2. **빈 칸에서의 출고는 발명하지 않는다.** 없는 슬롯은 `result != 0` +
   `그 칸은 비어 있습니다.`
3. **화폐는 음수로 내려가지 않는다.** `4/2` 는 성공 응답을 받았을 때만 게스트가
   소지금을 깎으므로, 거절이 곧 양쪽의 일치다.

부분 출고는 레코드의 **선두 i32(수량)** 하나만 고쳐 쓴다 — 이 모듈이 읽는 유일한
필드이고, 스무 개 중 다섯을 꺼내면 다섯이 나가고 열다섯이 남는다.

### 36.4 인수 기준

* **`hl5_core` 단위시험 32 → 50개.** 새로 18개: `warehouse.rs` 9개(빈 창고 =
  한 페이지, 왕복 바이트 동일, 부분 출고, 정리, 화폐 하한, 파일 왕복, 잘못된
  줄, 용량 클램프), `localserver.rs` 9개(목록 프레임의 **9바이트 레코드**를
  손으로 걸어 확인, 입고→커밋→재목록→출고, 부분 출고, 꽉 찬 창고 거절, 빈 칸
  거절, 화폐 왕복, 정리, **`Session::new()` 에는 창고가 아예 없다**, 새 세션이
  디스크에서 이전 세션의 물건을 본다).
* **`tests/localserver_ab.rs` 5개 그대로 통과** — `Session::new` /
  `with_grants` 는 창고를 갖지 않으므로 "Python 서버와 바이트 동일"이 계속
  참이다. 창고는 `Session::offline` 에만 붙는다.
* **1,241 프레임 5장면 A/B 그대로 통과** — 그 재생은 `--offline-menus` 를 쓰지
  않으므로 이 코드 경로가 아예 없다.
* **기기**: 위의 로그 일곱 줄과 스크린샷 — 창고 그리드(16칸, 거래용 화폐 0),
  `올리기/버리기` 메뉴, `창고로 올리시겠습니까?`, 창고 0번 칸의 총,
  `내리기/버리기`, `싱글가방으로 내리시겠습니까?`, 그리고 가방으로 돌아온 총.

### 36.4.1 공지에서 창고를 뺐다

`7/1` 의 문구가 `오프라인 모드입니다.;창고, 거래소, 우편함은;서버가 있어야
이용할 수 있습니다.` 였다. 창고가 도는데 그렇게 말하면 **다음 화면이 하는 일의
반대**를 알려 주는 것이므로, 이제
`오프라인 모드입니다.;거래소와 우편함은;서버가 필요합니다.` 다.

`;` 로 나눈 각 토막이 **혼자서 팝업 폭에 들어가야 한다** — 게스트는 폭으로
줄바꿈하므로 한 단어 긴 줄은 음절 중간에서 갈린다. 첫 시안이 기기에서
`있습니` / `다.` 로 끊겼고, 그래서 마지막 토막을 짧게 고쳤다.

### 36.4.2 거래용 화폐 환전 버튼을 지웠다

싱글가방 패널 왼쪽 아래의 코인 무더기가 그것이고, 오프라인에서는 **일방통행
문**이다:

* 게스트에는 거래용 화폐를 **되돌리는 경로가 없다.** 카테고리 4 의 화폐 프레임은
  `4/2`(지갑 → 창고)와 `4/7`(잔액 보고) 둘뿐이고, 반대 방향을 요청하는 UI 가
  없다.
* 그 화폐가 쓰이는 곳은 거래소이고, 거래소는 사람이 상대편에 있어야 한다.
  오프라인에서는 닫혀 있다(§32.7).

즉 넣을 수만 있고 꺼낼 수 없으며, 사는 것도 못 산다. 그런 버튼은 없는 것보다
나쁘다 — 기능처럼 보이면서 플레이어의 골드를 먹는다. **실제로 먹었다**: 이
세션의 기기 시험 중 스트레이 탭 하나가 그것을 실행해 소지금 1,000 이 전부
거래용 화폐로 넘어갔고, 게임 안에서 되돌릴 방법이 없다.

후크 **셋**으로 지우고 되돌린다(`warehouse::install_hooks`,
`Config::offline_menus` 일 때만):

| | 후크 | 판정 |
|---|---|---|
| 아이콘 | `Graphic::DrawImage` | 복귀주소가 `DrawStorage+0x130`(=0x132DA0) 하나일 때만 사양 |
| 대화 | `StateNetMenu::SetPopupState` | 팝업 id 가 `0x1e` 일 때만 |
| **환불** | `StateNetMenu::DrawStorage` | 잔액 > 0 일 때 지갑에 돌려주고 0 으로 만든다 |

* 아이콘 쪽은 그 한 호출이 `DrawImage(GetFB(), origin.x+0xd, origin.y+0xb5,
  gv+0x16a0, 12)` — 패널 왼쪽 아래다. `Graphic::DrawImage` 는 게임 전체의
  스프라이트 블릿이므로 **복귀주소가 판정의 전부**이고, 그 밖의 호출은
  `Ok(false)` 로 게스트 코드로 넘어간다.
* 대화 쪽을 호출자가 아니라 **id 로** 거르는 이유: 아이콘만이 입구가 아니다.
  `SetPopupState` 의 호출 지점 60곳을 전부 훑어 `mov r1, #0x1e` 를 찾으면
  **정확히 둘**이고 둘 다 이 화면이다 — 터치의 0x12E3B4(`TouchStorageUI`)와
  키의 0x12D798(`KeyStorage`). 그래서 id 로 걸러도 다른 화면의 대화를 삼킬 수
  없다.

그 팝업이 환전이라는 것은 번호가 아니라 **게스트가 그 팝업에 무엇을 넣는지**로
확인했다: 0x12E384..0x12E390 이 `HERO::GetBagItemPtr` → `BagItem::GetMoney` 의
결과를 인수로 실어 보낸다 — 지갑의 잔액을 상한으로 삼는 것은 환전 대화가 하는
일이고, 이 화면의 다른 팝업은 그것을 묻지 않는다.

### 36.4.3 이미 넘어간 잔액은 지갑으로 되돌린다

버튼을 지워도 **이미 들어간 돈**이 남는다 — 위의 1,000 이 그것이다. 오프라인에는
꺼내는 경로도, 쓸 곳도 없으니 그대로 두는 것은 플레이어의 돈을 구덩이에 두는
것이다. 그래서 세 번째 후크가 창고관리를 열 때 한 번 되돌린다:

```
gv+0x1474      -> HERO*
gv+0x1970      -> (i8) 저장 슬롯
HERO::GetBagItemPtr(hero)          -> BagItem*
BagItem::IncreaseMoney(bag, 잔액)   (0xA28E0 — 게스트 자신의 입금 경로)
HERO::SaveBagData(hero, 슬롯)      (0x8F4F4 — 입고 arm 이 쓰는 그 호출)
그리고 창고 잔액을 0 으로, 파일에 저장
```

두 필드와 두 호출은 전부 **게스트가 같은 화면에서 하는 것을 베낀 것**이다:
`ProcPacket_Inventory` 의 입고 arm(0xCADA4..0xCADC0)이 정확히 그 gv 오프셋으로
`SaveBagData` 를 부른다. 그래서 "이 화면에서 가방이 살아 있다"는 가정이 아니라
관측이다.

* **왜 이주(migration)가 아니라 상시 규칙인가**: 버튼이 없고 서버가 없으면 잔액은
  전에 넣은 것일 수밖에 없다. "창고관리를 열 때 돌려준다"는 규칙은 0 으로
  수렴하고 그 뒤로는 영원히 아무 일도 하지 않는다. 실서버를 설정하면 이 후크
  자체가 설치되지 않으므로 정상 계정의 잔액을 건드릴 수 없다.
* **게스트 함수만 부른다.** 게스트 메모리에 직접 쓰지 않는다 — §16.10 이 정렬
  A/B 를 위해 요구하는 규칙이고, 후크는 항상 `Ok(false)` 로 사양하므로 화면은
  게스트가 그대로 그린다.

기기 실측(건슬링어, 슬롯 2): 창고관리를 열면
`warehouse: 거래용 화폐 1000 returned to the purse` 가 찍히고 싱글가방의
**소지금이 0 → 1000** 이 된다. 재시작 후에도 1000 이 남고, 파일이 `0 money` 로
로드되며 **환불 줄은 다시 나오지 않는다**.

> 창고는 공용이므로 잔액은 **먼저 창고관리를 연 캐릭터**에게 간다. 돈을 잃은
> 캐릭터로 열어야 그 캐릭터가 받는다.

`4/2` 핸들러 자체는 남겨 두었다. 실서버가 설정되면 환전은 원래 뜻을 되찾고
아이콘도 돌아온다 — 후크가 `offline_menus` 에만 붙기 때문이다.

### 36.5 아직 아닌 것

* **재시작 후 남는 것은 단위시험으로만 증명됐다.** 파일 왕복과 "새 세션이 이전
  세션의 물건을 본다"는 바이트로 확인했지만, 기기에서 force-stop 후 다시
  들어가 보는 것은 아직 안 했다 — 그 장면의 탭 연출이 아이템 이름 툴팁에 가려
  더 진행되지 않았다.
* **클라이언트가 거절하는 것들은 그대로 거절된다.** 장착 중인 장비(조용히),
  `kind 14`(소모품 — 물약은 창고에 안 들어간다), `kind 18`(캐시 아이템),
  `kind 12 && index > 13`, `kind <= 3 && index > 0x50`(**`(진)` 86번은
  예외 — §50**), 그리고 `(10, 17)`. 전부 **패킷 없이**
  `이용할 수 없는 아이템입니다.` 로 끝난다 — 호스트가 할 수 있는 것이 없고,
  원본도 같다. 86번만 게스트 후크가 그 비교를 건너뛴다.
* **창고 확장(캐시 id 17)** 은 살 것이 없어졌다 — 출발 용량이 이미 그리드의
  상한(80칸, 다섯 페이지)이다. `Warehouse::expand` 는 남겨 두었지만 지금은
  아무것도 늘릴 것이 없고, 그 캐시 항목은 여전히 지급 표에서 거절된다.
* **`4/8`(창고의 캐시 아이템 사용)** 은 응답이 `EquipItemInfo` 를 싣는 arm
  (0xCAE60)이라 아직 안 읽었다. 캐시 아이템은 애초에 창고에 못 들어가므로
  도달 불가고, 그대로 거절된다.
* **거래소·우편함은 그대로 닫혀 있다.** 그것들은 사람이 상대편에 있어야 하는
  기능이고, 창고와 달리 혼자 쓰는 저장소가 아니다.

## 37. 퀵슬롯과 동시입력 — 조사 (2026-08-21)

사용자 요구 둘이다. **결론이 서로 반대라서 한 절에 같이 둔다.**

* **캐시템을 퀵슬롯에 등록하고 싶다** → 원본도 안 된다. kind 18 은 그 팝업에
  구조적으로 도달할 수 없다. **그런데 우리가 지급하는 kind 를 바꾸면 된다.**
* **조이스틱과 OK/스킬을 동시에 누르고 싶다** → **원본은 됐다.** 게스트도
  손가락을 구분한다. **우리 셸이 마지막 한 칸에서 끊었다.** 다만 고쳐도 절반이고,
  나머지 절반은 원본의 버그다(§37.7).

이 절은 전부 디스어셈블과 dex 에서 나왔다. 기기 시험은 아직 안 했다.

### 37.1 가방 팝업은 아이템 kind 점프테이블 하나가 정한다

`StateInGameMenu::KeyBagMenu`(0x1091E0)의 substate 1, key -5 경로:

```
0x109c0c  ldrsb r3, [r0, #0x14]      ; r0 = ItemInfo*, +0x14 = kind
0x109c18  cmp   r3, #0x12            ; 18
0x109c1c  addls pc, pc, r3, lsl #2   ; case 0 은 pc+8 = 0x109c24 (§31 의 그 함정)
0x109c20  b     0x1099b0             ; kind > 18 → 팝업 없음
```

| kind | 클래스 | 도달하는 팝업 |
|---|---|---|
| 0~10 | `EquipItemInfo` | `IsEquipIng()` 이면 없음, 아니면 popup 2 |
| **11** | **`BattlelUseItemInfo`** | `[+0x15] == 10` 이면 popup 4, **아니면 popup 3** |
| 12·13 | 오브·재료 | popup 4 |
| 14 | — | 팝업 없음 |
| 15~17 | 조합서·스킬북 | popup 5 |
| **18** | **`CashItemInfo`** | `[+0x134]` 2 → popup 4, 3 → popup 5, **그 밖엔 없음** |

팝업의 내용물은 `StateInGameMenu::SetPopupState`(0xE67B8)가 짓는다 — `popupId - 1`
위의 25항 점프테이블이고 여기서도 **case 0 이 0xE67FC** 다.

| popupId | 메뉴 | 만드는 자리 |
|---|---|---|
| 1·8·9·11 | {OK} | 0xE6988 |
| 2 | {분해, 버리기} | 0xE6A00 |
| **3** | **{단축키등록, 버리기}** | **0xE6C34** |
| 4 | {버리기} | 0xE6B28 |
| 5 | {사용하기, 버리기} | 0xE6B20 |
| 12·13 | {착용하기, 해제하기} | 0xE6BA4 |
| 14 | {장착하기, 해제하기} | 0xE6B94 |
| 15·16 | {단축키등록, 해제하기} | 0xE6860 |
| 18 | {ct82, 단축키등록, 해제하기} | 0xE6A78 |
| 6·10·19~22·25 | {예, 아니오} | 0xE68F0 |

`단축키등록` 은 `TextMgr::GetCommonText(0x36)` 이다(`/c/csv/common_text.dat` 54행,
`버리기` 52 / `사용하기` 55 / `해제하기` 67). 인덱스는 **r1** 이다(§31).

> **가방에서 `단축키등록` 을 내는 팝업은 popup 3 하나뿐이고, 거기에 가는 kind 는
> 11 하나뿐이다.** popup 15/16/18 은 스킬·특성 메뉴 쪽이다
> (`KeySkillMenu` / `KeyPropertyMenu`). kind 18 이 갈 수 있는 곳은 4 와 5 뿐이다.

`[+0x15] == 10` 만 따로 빠지는 것은 `item_11.dat` 10행이 **제련석**이기 때문이다 —
전투 소모품이 아니다. `ItemInfo+0x15` 가 행 번호라는 근거는
`BattlelUseItemInfo::IsEquipIng`(0xA892C)이 벨트의 저장 인덱스를 `ldrsb [this,#0x15]`
와 비교한다는 것이다(0xA89C0).

### 37.2 벨트는 kind 11 을 전제로 만들어져 있다

`HERO::SetHotKeyBelt(slot, page, type, index)`(0x88B9C)는 네 인자 중 앞 둘로
주소를 만든다 — `this + 0x1B40 + slot*2 + page`, 그리고 `type` 을 `+0xC`,
`index` 를 `+0x16` 에 넣는다. 즉:

```
int8 type [5][2]  at HERO+0x1B4C     ; 주소 = slot*2 + page
int8 index[5][2]  at HERO+0x1B56
int8 page         at HERO+0x1B60     ; HERO::GetHotKeyToggle
```

**5슬롯 × 2페이지 = 10칸.** `IsEquipIng` 의 이중 루프가 그것을 확인한다(안쪽
`cmp #0x5000000`, 바깥쪽 `cmp #0x2000000`). `type == 2` 가 아이템 칸이다.

그리고 그 아이템 칸은 **kind 를 저장하지 않는다** — 인덱스 한 바이트뿐이다.
소비자들이 kind 11 을 즉치로 넣어 되찾는다. 그래서 kind 18 을 벨트에 올릴 수
있게 만들어 봐야 엉뚱한 `item_11` 행이 나온다. **데이터 플래그를 하나 뒤집어서
될 일이 아니다.**

### 37.3 진짜 엘릭서는 따로 있다 — `item_11.dat` 8행

19개 아이템 표 전부를 이름으로 조인했다. `item_18.dat`(49행, 캐시 **진열품**)의
행 중 일반 표에 같은 이름이 있는 것은 넷뿐이고, 그중 **kind 11** 은 셋이다:

| item_18 행 | 이름 | 일반 표의 쌍둥이 | 카탈로그에 있나 |
|---|---|---|---|
| 30 | 보호의 부적 | kind 11 / 14 | 아니오 |
| 31 | 마석 | kind 11 / 15 | 아니오 |
| **34** | **엘릭서** | **kind 11 / 8** | **예 — id 4, 500원, 수량 20** |
| 43 | 공허의 목걸이 | kind 10 / 17 | 예 (이미 재사상돼 있다, §32.9) |

즉 **카탈로그 30행 중 이 문제에 해당하는 것은 id 4 하나다.** 나머지는 제작
재료·대상 지정 주문서·계정 서비스·장비·랜덤 상자라 애초에 퀵슬롯에 올릴 것이 아니다.

와이어는 안 바뀐다: `NETWORK::readNetItem`(0xC85E4)이 `cmp kind,#0xa; ble` 로
갈라지므로 **kind 11 도 kind 18 과 똑같은 짧은 레코드**(10바이트 + 이름)다.
게스트의 재사상도 안 걸린다 — `ProcPacket_Shop`(0xCD38C)의 특례는
`kind == 18 && 44 <= index <= 48` 뿐이다.

popup 3 에는 `사용하기` 가 없다 — {단축키등록, 버리기} 뿐이다. 그래서 바꾸고 나면
엘릭서는 **퀵슬롯 전용**이 된다. 처음에는 그것을 대가로 적었는데, **대가가 아니다.**

> **지금 캐시 엘릭서는 먹어도 아무 일이 안 일어난다.** `StateInGameMenu::Process`
> 의 kind 18 사용 경로(0x10AEE0~)는 행 번호로 갈라지는데, 실효 코드가 있는 것은
> 11~14 · 19~21 · 24 · 25(0x10BA44 의 15항 점프테이블, **case 0 = 행 11**) 과
> 30·31 · 39~42 뿐이다. **행 34 는 어느 쪽도 아니다** — 0x10BB50 의
> `cmp index,#13; bgt` 가 0x10BEE8 로 보내고, 거기서 하는 일은
> `GetCommonText(298)`(`을(를) 사용했습니다.`)을 찍고
> `DeleteBagItemCategoryIndex(0x12, 34, 1)` 로 지우는 것뿐이다. **회복도, 효과도,
> 실패 메시지도 없다.**
>
> 즉 이 변경은 퀵슬롯을 얻으면서 잃는 것이 없다. **없던 효과가 생긴다** —
> `item_11.dat` 8행의 엘릭서는 `b134 = 90`(즉시 회복), `b136 = 250` 이다.

옛 세이브의 kind 18 엘릭서는 그대로 남는다 — 지급 표는 **새로 나가는 와이어
레코드**만 고친다(§32.9 와 같은 성질).

### 37.4 동시입력 — 원본 Java 는 두 손가락을 보냈다

`payload/classes.dex` 의 `CletEntry.onTouchHandleEvent(MotionEvent)` 는 갈래가
둘이고 `bSupportMultitouch` 가 고른다.

**멀티터치 갈래** (0x0014~):

* `packed-switch (action & 0xff)` — ACTION_POINTER_DOWN(5)/POINTER_UP(6)까지 받는다.
* 액션 인덱스를 손으로 뽑는다: `(action & 0xFF00) >> 8`. `getActionIndex()` 는
  minSdk 5 에 없었다.
* `getPointerCount() > 2` 면 **버린다**(DOWN/UP 양쪽 다).
* `getPointerId(i)` / `getX(i)` / `getY(i)` 인덱스 오버로드를 쓰고,
  **`nativeTouchHandleEvent(19|20|21, x, y, pointerId)` 의 4번째 인자로 실제
  포인터 id 를 싣는다.**
* ACTION_MOVE 는 포인터 **전체를 순회**하며(상한 2) id 별로
  `lastActionMoveEvtX/Y[id]` 와 비교해 안 움직인 손가락은 건너뛴다.
  그리고 배치마다 `System.currentTimeMillis()` 로 32 ms 스로틀
  (`Thread.sleep(32)`).

**단일터치 갈래** (0x010C~): `getX()`/`getY()` (포인터 0)만 쓰고 4번째 인자로
**항상 상수 0** 을 넘긴다.

그리고 어느 갈래를 쓰는지는 실행 중에 안 정해진다:

```
--- checkTouchType()V
    0x0000  const/4 v4, 1
    0x0001  iput-boolean v4, v6, CletEntry.bDecidedTouchType:Z
    0x0003  iput-boolean v4, v6, CletEntry.bSupportMultitouch:Z
    0x0005  return-void
```

**세 명령어다. 무조건 참이다.** 즉 원본은 **모든 기기에서 항상 멀티터치 갈래를
돌았다.** `assets/mtouchdevice.txt` 의 네 줄(SHW-M110S/M180S/M250S/NexusS)은
`<init>` 에서 필드에 담기기만 하고 **아무도 읽지 않는다** — 흔적기관이다.
CLAUDE.md 의 "화이트리스트에 없으면 터치가 꺼지는지 확인 전까지는 목록의 이름
하나를 보고한다"는 이제 근거가 없어졌다(모델명을 뭐라고 보고하든 상관없다).

### 37.5 게스트도 손가락을 구분한다 — 조이스틱에 주인이 있다

포인터 id 가 JNI 경계에서 게임 코드까지 살아서 간다:

```
Java_..._nativeTouchHandleEvent (0x14E6A4)   pointerId = [sp+0x1c] → r3
  → handleCletEvent  (0x43124)               r3 그대로
  → clsidAuthHandle  (0x42FD0)               r3 을 건드리지 않고 통과시킨다
  → onHandleEvent    (0x9C20C)               mov r5, r3
```

`onHandleEvent` 는 `type - 2` 위의 20항 점프테이블이다(**case 0 = 0x9C234**):

| type | 부르는 것 | 인자 |
|---|---|---|
| 2 | vptr+0x1c `KeyPress` | (this, key) |
| 3 | vptr+0x20 `KeyRelease` | (this, key) |
| 19 | vptr+0x30 `TouchPress` **그리고** vptr+0x38 `TouchMove` | (this, x, y, **id**) |
| 20 | vptr+0x34 `TouchRelease` | (this, x, y, **id**) |
| 21 | vptr+0x38 `TouchMove` | (this, x, y, **id**) |

`_ZTV9BaseState` + 8 을 덤프하면 그 자리들이 실제로
`TouchPress(iii)` / `TouchRelease(iii)` / `TouchMove(iii)` 다 — **세 번째 int 가
포인터 id다.**

게스트의 입력 상태는 `gv` 안의 **한 칸짜리 우편함 다섯**이다:

| | 플래그 | x | y | id |
|---|---|---|---|---|
| 키 누름 | +0xE4 | — | — | key at +0xE8 |
| 키 뗌 | +0xE5 | — | — | key at +0xEC |
| 터치 누름 | +0xF0 | +0xF4 | +0xF8 | **+0x10C** |
| 터치 뗌 | +0xF1 | +0xFC | +0x100 | **+0x110** |
| 터치 이동 | +0xF2 | +0x104 | +0x108 | **+0x114** |

`mainKeyPressed`(0x9CA18)가 틱마다 이 다섯을 **정해진 순서로 한 번씩** 비운다
(누름 → 이동 → 뗌 → 키누름 → 키뗌). 같은 종류가 한 틱에 둘 오면 뒤엣것이
앞엣것을 덮어쓴다 — `BaseState::TouchMove`(0x44DCC)는 아예 `gv+0xF2` 가 서 있으면
**기록조차 안 한다**(`streq`).

그리고 조이스틱에는 **주인 손가락**이 있다:

```
gv+0x120 / +0x122   조이스틱 원점 x/y (short)
gv+0x124            조이스틱을 잡은 포인터 id (signed byte, 없으면 -99)
gv+0x125            조이스틱 켜짐
```

```
StateGame::TouchPressed +0x250   strb #1, [gv+0x125]      ; 조이스틱 ON
                                 strb sl, [gv+0x124]      ; ← 그 손가락 id
StateGame::TouchMoved   +0x5c    if (gv+0x125 && id == (i8)gv+0x124) → 스틱을 민다
                                 else                                → 버튼 히트테스트
StateGame::TouchReleased +0xdc   if (id != (i8)gv+0x124) → 스틱을 안 놓는다
BaseState::TouchRelease  +0x88   같은 비교로 gv+0x126 래치
```

**설계가 명백하다. 한 손가락은 스틱을, 다른 손가락은 버튼을 잡으라고 만든 코드다.**

### 37.6 그런데 호스트가 마지막 한 칸에서 끊는다

```
hl5_android/src/lib.rs:1194     let _slot = self.touch_slot(t.id, x, y, release);
hl5_core/src/lib.rs:1617        &[ty, x, y, 0],
```

`touch_slot()`(`lib.rs:205`)은 이미 완성돼 있고 주석까지 정확하다 — *"The guest
stores the owning finger in a **signed byte**, so a raw winit id … cannot survive
the round trip. Slot indices make it exact by construction."* 계산해 놓고
`_slot` 으로 버린다. `Vm::touch` 는 인자 자리 자체가 없어서 리터럴 `0` 을 넣는다.

> **모든 손가락이 id 0 이 되므로 `id == gv+0x124` 가 항상 참이다.** 그래서
> OK/스킬/dash 를 **뗄 때** 조이스틱이 풀린다. 사용자가 말한
> "dash를 누르면 조이스틱이 풀려버린다"의 정확한 기전이고, 누를 때가 아니라
> **뗄 때**다 — `TouchPressed+0x1dc` 는 `gv+0x125` 가 이미 서 있으면 래치를
> 건드리지 않고 버튼 히트테스트로 지나간다.

즉 **우리는 원본이 한 번도 실행하지 않은 갈래를 무조건 재현하고 있다.**
형제 프로젝트 크로노스는 이미 winit 포인터마다 안정된 슬롯 id 를 넘기고 잃어버린
release 를 재생한다 — 참조 구현이 있다.

### 37.7 포인터 id 를 고쳐도 절반이다 — `HERO::KeyRelease` 가 원본 버그다

이것이 이 절에서 가장 중요한 발견이고, 조사 전에는 몰랐던 것이다.

터치 조이스틱과 버튼은 **전부 WIPI 키코드를 거친다.** `StateGame::TouchMoved` 가
`(x - gv+0x120, y - gv+0x122)` 를 ±10 px 데드존 + 우세축 판정으로 −1/−2/−3/−4 중
하나로 만들고 **방향이 바뀔 때만** `KeyPress` 를 낸다. 그리고
`StateGame::TouchReleased` 는 버튼마다 **`KeyRelease`(vptr+0x20)** 를 낸다:

| 자리 | 내는 키 | 자리 |
|---|---|---|
| 조이스틱 | `gv+0x11c`(현재 방향) | 0xE1924 |
| OK | **−5** | 0xE19B0 |
| 스킬 슬롯 1~5 | `'1' '3' '9' '*' '0'` | 0xE1A6C / 0xE1A54 / 0xE1A24 / 0xE1A0C / 0xE19F4 |
| 콤보 스킬 | `'7'` | 0xE1A3C |

그 `KeyRelease` 가 가는 곳:

```
StateGame::KeyReleased (0xE291C) → Battle::KeyReleased (0x4759C)
  → HERO vptr+0x10 → <CLASS>::KeyRelease
      ROGUE   (0xD8398, 4 B)  → b HERO::KeyRelease     ; 꼬리호출 썽크 (§31)
      WARRIOR (0x147444, 4 B) → b HERO::KeyRelease
      GUNNER  (0x87E8C)  → bl HERO::KeyRelease, 그 뒤 모션 보정만
      KNIGHT  (0xAAAC8)  → bl HERO::KeyRelease, 그 뒤 모션 보정만
```

**네 클래스 전부 `HERO::KeyRelease`(0x9BDCC)로 모인다.** 그 함수:

```
0x9bdd8  bl  StaticUtil::NumKeyToNaviKey   ; ← 반환값을 r0 에 받고
0x9bddc  mov r1, #0                        ; ← 바로 덮어쓴다. 버린다.
0x9bde8  bl  HERO::SetMoveKeyChk(0, 0)     ; 네 방향 플래그를
0x9bdf8  bl  HERO::SetMoveKeyChk(1, 0)     ; 전부
0x9be08  bl  HERO::SetMoveKeyChk(2, 0)     ; 무조건
0x9be18  bl  HERO::SetMoveKeyChk(3, 0)     ; 지운다
0x9be20  bl  CHAR::GetMainState
0x9be2c  beq → 0x9be60: HERO::SetStandMotion(GetDir())   ; 걷는 중이면 세운다
```

`NumKeyToNaviKey`(0x1441C4)는 멀쩡하다 — 넘패드를 방향키로 옮기는 7항
점프테이블(`'2'`→−1, `'4'`→−3, `'5'`→−5, `'6'`→−4, `'8'`→−2, 나머지는 그대로)이다.
**부르고 나서 결과를 안 쓴다.** 원래는 그 값으로 갈라서 해당 방향만 지우려던
코드였을 것이다.

> **따라서 어떤 키를 떼도 이동이 멈춘다.** OK(−5)를 떼도, 스킬(`'1'`)을 떼도
> 네 방향 플래그가 다 지워지고 걷는 중이면 `SetStandMotion` 이 걸린다. 이것이
> 사용자가 말한 "이동하다가 스킬이나 OK버튼으로 공격하면 이동키가 안먹어서
> 좀 어색하고"의 기전이고, **포인터 id 와 무관한 원본 버그다.**

거꾸로, **공격 자체는 걸으면서 하도록 허용돼 있다** — `WARRIOR::CheckSKillUse+0x1c0`
은 메인상태가 1(걷기)일 때만 계속한다. 즉 원저자는 "걸으면서 공격"을 의도했고,
`KeyRelease` 한 곳만 잘못 썼다.

**그래서 §37.6 의 한 줄과 이 절의 후크는 별개의 두 고침이고, 둘 다 해야 한다.**

### 37.8 게스트 입력의 나머지 한계 — 고치지 말 것과 못 고칠 것

* **키 상태에 비트마스크가 없다.** `BaseState::KeyPress`(0x44CBC)는 `gv+0xE4 == 0`
  일 때만 `eq` 술어로 기록한다 — 한 틱에 두 번째 키다운은 **조용히 버려진다.**
  `gv+0x11c` 가 "지금 눌린 방향" 하나, `gv+0x118` 이 "마지막으로 보낸 코드" 하나다.
* **`BaseState::TouchMove` 도 한 칸이다**(`gv+0xF2` 가 서 있으면 기록 안 함).
  두 손가락이 같은 틱에 움직이면 하나만 전달된다. 조이스틱 드래그 + 버튼 탭에는
  문제가 없지만, 두 손가락을 동시에 문지르는 조작은 **원본에서도 안 됐다.**
* **원본은 3손가락 이상을 버렸다**(`getPointerCount() > 2` → return). 우리가 그것을
  따라할 의무는 없지만, 안 따라하면 그 지점부터는 원본과 다른 동작이다.
* **dash**: 오른쪽 가장자리 버튼(`gv[0x1971]` 로 게이트)이
  `HERO::SetAttackMotion(7, dir)` + `SetDashFlag(dir,0)` + `SetDashCheckFrame(0)` 를
  부른다. 더블탭 dash 는 `<CLASS>::KeyPress` 안에 10프레임 창으로 따로 있다.
  **"dash 를 누르면 스틱이 풀린다"는 dash 코드의 문제가 아니라 §37.6 이다.**
* `StateGame::UpdateNaviAngle` 이 5° 로 양자화한 각도를 `gv+0x278` 에 따로 둔다 —
  손잡이 그림(`GetTouchJoyXY`)과 `HERO::SetAngleMoveKeyChk` 가 쓴다. 즉 방향은
  키 층에서는 4방향, 애니메이션 층에서는 연속이다.

### 37.9 인수 기준 — 바이트 동일이 정의상 불가능한 변경이 둘이다

§16.10 과 §23 의 방식을 그대로 쓴다.

**포인터 id 배선 (§37.6)**

1. **손가락 하나만 쓰는 재생에서는 slot 이 항상 0 이므로 1,241 프레임 5장면 A/B 가
   그대로 바이트 동일일 것.** 이것은 실제로 증명 가능하다 — 회귀가 아님을
   증명하는 진짜 기준이다. `hl5_core` 를 건드리므로 **HEAD 대비** 대조도 같이
   (§35.4.1 의 worktree 방법).
2. `hl5_probe` 스크립트에 `press/release/move` 의 **선택적 4번째 인수**(포인터 id)를
   더하고, 두 손가락 재생을 새로 만든다:
   `press(스틱) → press(OK) → release(OK) → move(스틱)` 뒤에도 `gv+0x125` 가 1 이고
   `gv+0x124` 가 스틱 손가락 id 일 것. `--count` 나 프레임 덤프로 본다.
3. `release_all_touches` 가 손가락마다 **자기 slot 으로** release 를 재생할 것
   (지금은 좌표만 모은다).

**`HERO::KeyRelease` 후크 (§37.7)**

4. **`Vm::hooks` 에 넣는다. `patch.rs` 에는 절대 넣지 않는다** — 그 파일의 계약은
   "같은 바이트를 내는 이식"이고 동작 변경이 섞이면 그 파일의 A/B 가 무엇을
   증명하는지 아무도 말할 수 없게 된다(§16.10 과 같은 이유).
5. 후크는 **게스트 함수만 부를 것**: `NumKeyToNaviKey(key)` 를 스스로 부르고,
   결과가 −1..−4 가 아니면 `SetMoveKeyChk` 넷을 건너뛰고 `SetStandMotion` 도
   건너뛴다. 방향키였을 때는 원본과 **완전히 같게** 행동한다 —
   그래서 방향키만 쓰는 1,241 프레임 A/B 가 계속 통과해야 한다.
6. 기기에서: 걸으면서 OK 를 떼도 캐릭터가 계속 걷고, 방향키를 떼면 선다.

**지급 kind 변경 (§37.3)**

7. `free_shop` 기본 off 이므로 1,241 프레임 A/B 는 무영향일 것.
8. `--free-shop` 재생에서 `NewBagNetItem` 의 `_NET_ITEM_+4 == 11`, `+5 == 8`,
   `+0 == 20` 일 것.
9. 기기에서 엘릭서를 사면 가방에서 **{단축키등록, 버리기}** 가 뜨고, 등록한 뒤
   그 슬롯을 누르면 **실제로 회복될 것**(지금은 회복이 아예 없다, §37.3).

### 37.10 아직 모르는 것

* **기기 시험을 하나도 안 했다.** 이 절 전체가 디스어셈블·dex·데이터 파일 판독이다.
* **적대적 검증 단계가 안 돌았다** — 조사 10갈래는 끝났지만 검증 12갈래와 종합이
  세션 한도로 실패했다. 이 절에 적은 것 중 **주소와 명령어가 붙어 있는 것은
  전부 사람이 직접 디스어셈블로 확인했고**, 확인하지 않은 것은 아래뿐이다:
  * `#0xb`(kind 11) 즉치가 벨트 소비자 **일곱 곳**에 있다는 개수 — 직접 센 것은
    `HERO::BattleUseItem` 과 `GMenu::DrawSkillBelt` 두 곳의 존재이고, 일곱이라는
    수는 조사 에이전트의 보고다.
  * 넘패드 → 슬롯 대응(`'1'/'3'/'9'/'*'/'0'` = 슬롯 0~4, `'7'` = 콤보, `'#'` = 페이지)
    은 `StateGame::TouchReleased` 가 그 키들을 낸다는 것까지만 직접 봤다.
  * `gv[0x1971]` 이 dash 버튼의 게이트라는 것.
* **`Config::speed` 와의 상호작용**을 안 봤다. 속도를 올리면 틱당 입력 한 칸
  제한(§37.8)이 더 자주 걸릴 수 있다.
* **32 ms 스로틀을 원본이 왜 넣었는지** 모른다. 우리는 안 넣었고, 안 넣어서
  생긴 문제도 아직 못 봤다.

## 38. §37 을 구현으로 옮겼다 — 그리고 §37 이 두 군데 틀렸다 (2026-08-21)

§37 의 조사 셋을 코드로 옮겼다. 적대적 검증 열 갈래(§37.10 이 못 돌렸다고 적어
둔 그것)를 먼저 돌렸고, 그 결과가 **구현을 바꿨다** — §37.7 이 함수의 꼬리를
빠뜨렸고, 이동 플래그의 소비자를 조사하지 않았다.

들어간 것:

| 요구 | 파일 | 게이트 |
|---|---|---|
| ⑧ 캐시 엘릭서를 퀵슬롯에 | `hl5_core/src/shop.rs` | `Config::free_shop` (기존) |
| ⑨-a 포인터 id 배선 | `hl5_core/src/lib.rs`, `hl5_android`, `hl5_probe` | 없음 — 회귀 복구다 |
| ⑨-b `HERO::KeyRelease` 후크 | `hl5_core/src/input.rs` (신규) | `Config::keep_walking` |

단위시험 51 → **57개** (`shop.rs` +3, `input.rs` +4 신규).

### 38.1 §37.7 이 꼬리를 빠뜨렸다 — 그대로 만들면 건슬링어가 망가진다

§37.7 은 `HERO::KeyRelease`(0x9BDCC)를 "`NumKeyToNaviKey` 를 부르고 버린 뒤
`SetMoveKeyChk` 넷과 `SetStandMotion`" 으로 적었다. **함수는 거기서 끝나지
않는다.** 0x9BE30 부터 열두 명령이 더 있고, 그것은 걷기 검사 **아래**가 아니라
**모든 경로**에서 돈다(0x9BE78 이 `b #0x9BE30`):

```
0x9be30  bl  HERO::GetPlayerClass
0x9be3c  cmp (r0<<24), #0x2000000      ; class 2
0x9be40  bne 0x9be5c                   ; 아니면 그냥 복귀
0x9be44  mov r3,#0x1f80; add r3,r3,#0x2d      ; = 0x1fad
0x9be4c  ldrb r2,[r4,r3]; cmp r2,#0; movne r2,#0; strbne r2,[r4,r3]
```

`HERO+0x1fad` 가 무엇인지는 **전수 조사로 확정했다** — 기준 레지스터로부터의
총 상수 변위가 0x1fad 인 메모리 접근을 `.text` 전체에서 찾았고(capstone
`skipdata`, **커버리지 100.00%**, 300,021 명령어), 넷뿐이다:

| 자리 | 무엇 |
|---|---|
| `GUNNER::KeyPress+0x4f4` (0x883B4) | `strb #1` — 콤보 기본모션에 들어가며 **세운다** |
| `GUNNER::EndProc+0x2c0` (0x8733C) | `ldrb`, `cmp #1` — **매 프레임 읽는다** |
| `HERO::SetStandMotion+0x100` (0x990E0) | `strb #0` — 서면 지운다 |
| `HERO::KeyRelease+0x80` (0x9BE4C) | `strb #0` — 여기서 지운다 |

**건슬링어의 콤보 진행 플래그**이고, `GetPlayerClass` 가 읽는 `[this+0x22c]` 의
값 2 가 건슬링어다(네 접근이 전부 GUNNER 함수라는 것이 그 자체로 증거다).

> **따라서 후크가 함수 몸통을 통째로 건너뛰면 그 플래그가 영구히 선다** —
> `GUNNER::EndProc` 이 첫 공격 뒤로 영원히 다른 갈래를 탄다. 고치려던 버그 하나를
> 새 버그 하나와 바꾸는 것이다. 후크는 비방향 키에서 `SetMoveKeyChk` 넷과
> `SetStandMotion` 만 건너뛰고 **이 꼬리는 원본과 같은 자리에서 같은 술어로**
> 재현한다(`ldrb != 0` 일 때만 쓴다 — 0x9BE50 의 `movne`/`strbne` 그대로).

이 한 바이트는 `Vm::hooks` 가 게스트 메모리에 직접 쓰는 유일한 곳이다.
CLAUDE.md §16.10 기준 2("후크는 게스트 함수만 부를 것")의 명시적 예외이고,
근거는 위 전수 조사다 — **0x1fad 에는 접근자가 없다.**

### 38.2 이동 플래그를 읽는 곳은 둘뿐이다 — 워리어·로그에는 죽은 코드다

§37.7 은 "네 방향 플래그를 지우니 이동이 멈춘다"로 읽히게 적혀 있다. 절반만
맞다. `HERO::SetMoveKeyChk`(0x88960)가 쓰는 `HERO+0x1d05..0x1d08` 의 독자를
전수 조사했다(같은 변위 스캔, 접근 9곳):

```
쓰기:  HERO::SetMoveKeyChk        0x88968
       HERO::SetAngleMoveKeyChk   0x8DAB4/AB8/AC0, 0x8DAF0/AF4/AFC
읽기:  HERO::IsMoveKeyChk         0x88978
       HERO::IsMoveKeyNone        0x889A0
```

그리고 `IsMoveKeyChk` 의 호출자는 **둘**, `IsMoveKeyNone` 은 **0** 이다:

```
_ZN4HERO12IsMoveKeyChkEa (0x88970) — 2 direct branch sites
    0x00087e00  GUNNER::Proc+0x228
    0x000aaa14  KNIGHT::Proc+0x210
_ZN4HERO13IsMoveKeyNoneEv (0x88980) — 0 direct branch sites
```

`GUNNER::Proc+0x214` 는 i=0..3 을 돌며 하나라도 서 있으면 0x87E6C 로, 전부
비어 있으면 0x87CD4 로 간다(근처 0x87E1C 에 `CHAR::SetMoveStop`). 인라인 독자는
없다 — 변위 스캔이 100% 커버리지에서 위 9곳 말고는 못 찾았다.

> **즉 워리어와 로그에게 `SetMoveKeyChk` 정리는 쓰기 전용이다.** 그 두 클래스가
> 겪는 버그는 `SetStandMotion` 하나이고, 그것이 `gv+0x278`(조이스틱 각도)을
> −1 로 만든다(0x990EC). 후크는 양쪽을 다 건너뛰므로 네 클래스 전부 고쳐지지만,
> **어느 클래스에서 무엇을 재는지**가 달라진다 — 아래 38.5.

### 38.3 §37.3 의 "b134 = 90, b136 = 250" 은 레코드 오프셋이 아니다

`item_11.dat` 를 아카이브에서 꺼내 읽었다: **16행**, 1,197 바이트에 정확히
맞는다. row 8 = 엘릭서, 레코드 길이 **46 바이트** — 오프셋 134·136 은 존재하지
않는다. 그 숫자는 **객체 필드**(`ItemInfo+0x134/+0x136`)이고, 레코드 꼬리
`5a 64 fa 01` 이 그리로 간다(0x5a=90, 0x64=100, **0xfa=250**, 0x01).

레코드의 끝에서 둘째 바이트가 그 250 이고, 같은 자리가 포션 4 / 미들포션 10 /
하이포션 20 / 퀵포션 40 / 미들퀵 100 / 하이퀵 160 이다 — **엘릭서가 표에서 가장
크다.** 다만 row 8 의 설명문은 `2500의 HP를 즉시 회복합니다.` 라 하류에서 배율이
붙는다. 실제 회복량은 `Formula::calc`(0x7749C) 뒤로 사라져 **확인하지 못했다** —
그래서 코드 주석에도 숫자를 안 적었다. 기기에서 250 이 아니라 큰 수가 나오는 것을
회귀로 읽지 마라.

row 10 = **제련석**, 그래서 `[ItemInfo+0x15] == 10` 특례가 있다(§37.1). 우리
index 는 8 이라 팝업 3 으로 간다.

카탈로그 30행을 `item_18` 과 조인해 확인: `a == 34` 인 행은 **id 4 하나**
(엘릭서(20), 500원, 수량 20). 보호의 부적·마석은 kind 11 쌍둥이가 있지만 판매
목록에 없다 — §37.3 이 맞았다.

한 가지 더: 팝업 3 이 `단축키등록` 을 담은 **유일한** 팝업이라고 §37.1 에 적었는데,
팝업 15·16 도 같은 `GetCommonText(0x36)` 을 담는다(0xE6860). 다만 `KeyBagMenu` 는
팝업 2·3·4·5·10 만 올리므로 **결론은 그대로다** — 정확한 문장은 "가방 메뉴가
올리는 팝업 중 유일하게".

### 38.4 포인터 id — 오프라인에서 픽셀로 증명됐다

`Vm::touch(kind, x, y)` → `Vm::touch(kind, x, y, pointer)`, JNI 4번째 인자로
전달. 호출처 7곳. `pointer.clamp(0, 0x7f)` 를 경계면에 두는 이유는 게스트가
주인을 **부호 있는 바이트**로 두고 `cmp id, byte asr #24` 로 비교하는데
(0xE18E8/EC), **−99 가 "주인 없음" 센티넬**(0xE1944 `mvn r2,#0x62`)이라서다 —
음수를 안 보내면 충돌이 정의상 불가능하다. (원점 `gv+0x120/0x122` 의 센티넬은
−100 으로 **다른 수**다, 0xE1900.)

증명은 같은 바이너리·같은 좌표·같은 프레임 수로 **스크립트의 포인터 id 만 다른**
두 재생이다(`tools_re/scripts/joystick-owner.txt` 와 `…-noowner.txt`):

```
스틱을 잡고(id 0) 밀어 둔 뒤, 둘째 손가락이 세 번 눌렀다 뗀다
  ids 0 과 1 →  tf-02:  조이스틱 링이 화면에 그대로 있다
  전부 id 0  → tf0-02:  링이 사라졌다
비교: tf-01 은 두 재생이 바이트 동일, tf-02 는 4,027/96,000 픽셀(4.19%) 차이,
      첫 차이가 (59, 139) = 조이스틱 자리
```

**카운터로는 갈리지 않는다**, 그리고 그것을 적어 두는 편이 낫다: 스틱 잡기
(0xE1CCC)와 놓기(0xE18F8)가 양쪽 다 2/2 다. 다른 것은 **시점**뿐이다 — 깨진
쪽에서는 둘째 손가락의 첫 뗌이 스틱을 놓고, 그 뒤 주인의 뗌은 할 일이 없다.
증거는 프레임이지 카운트가 아니다.

부수적으로 확인된 것 둘:

* **누르는 쪽에서는 스틱을 훔칠 수 없다** — 0xE1C58 이 `gv+0x125` 가 서 있으면
  래치를 건드리지 않고 버튼 히트테스트로 지나간다. 버그는 **뗄 때만**이다
  (§37.6 이 맞았다). 그리고 스틱 원점은 x ≥ 45, y < `GetHeight()−45` 로 클램프된다
  (0xE1C68~0xE1C88).
* **`release_all_touches` 의 순서는 무관하다.** `BaseState::TouchRelease`
  (0x44D28)가 주인의 뗌에서 `gv+0x126` 을 래치하고(0x44DBC), 래치가 서 있으면
  이후의 뗌을 **버린다**(0x44D78 경로). 그래서 어느 순서로 재생해도 우편함에
  남는 것은 주인의 뗌이다. 한 칸 우편함이 주인을 보호하도록 설계돼 있다.

### 38.5 `keep_walking` 후크 — 숫자로는 정확하고, 절반은 기기 몫이다

후크는 `Vm::hooks` 에 있고 `patch.rs` 에는 없다(§16.10). 복귀주소 필터를 **일부러
안 넣었다**: 네 클래스의 진입이 세 가지 LR 로 온다(ROGUE·WARRIOR 는 `b` 썽크라
`Battle::KeyReleased` 의 0x475FC, GUNNER·KNIGHT 는 `bl` 이라 0x87E98 / 0xAAAD4).
한 자리만 걸면 네 클래스 중 하나만 덮는다. 재귀가 불가능한 근거도 우연이 아니다 —
후크가 부르는 둘(`NumKeyToNaviKey` 0x1441C4, `GetPlayerClass` 0x88914) 안에
`bl` 이 **0개**다.

`NumKeyToNaviKey` 의 표를 완전히 풀었다(`addls pc,pc,r3,lsl#2`, case 0 = 0x1441D4,
그리고 **부호 없는** `cmp r3,#6` 이라 음수 키는 전부 폴스루):

| 키 | 결과 | | 키 | 결과 |
|---|---|---|---|---|
| `'2'` 0x32 | −1 UP | | `'6'` 0x36 | −4 RIGHT |
| `'3'` 0x33 | 그대로 | | `'7'` 0x37 | 그대로 |
| `'4'` 0x34 | −3 LEFT | | `'8'` 0x38 | −2 DOWN |
| `'5'` 0x35 | −5 SELECT | | 그 밖 | 그대로 |

터치 UI 가 내는 키(OK −5, 스킬 `'1' '3' '9' '*' '0'`, 콤보 `'7'`) 중 −1..−4 로
가는 것은 **하나도 없다** — `'3'` 이 표 범위 안인데도 폴스루와 같은 `bx lr` 로
간다. 조이스틱 자신의 뗌은 `gv+0x11c`(−1..−4, 기입자 전수 확인)로 오므로 **스틱을
놓으면 원본대로 선다.** 이 분류를 `input.rs` 의 단위시험 넷이 못박는다.

**측정 (프로브, 워리어, 같은 스크립트 `tools_re/scripts/keep-walking.txt`):**

| | 후크 off | 후크 on |
|---|---|---|
| `HERO::SetMoveKeyChk` 진입 | 28 | **4** |
| `0x9BE74` (`KeyRelease` 안의 `SetStandMotion`) | 0 | 0 |

차이 24 = **정확히 4 × 6** — 비방향 키 릴리스 여섯 개가 억제됐다. 그리고 켠 쪽에
남은 4 는 `keyup left` 하나가 낸 것이다: **방향키는 원본 그대로 돈다.**

`0x9BE74` 가 양쪽 0 인 것이 남은 구멍이다. 걷기 검사(`GetMainState() == 1`,
`SetWalkMotion` 이 그 1 을 세운다 — 0x98F84)를 만족하는 순간에 비방향 키를 떼는
재생을 **만들지 못했다.** 시도한 것과 관측:

* `keydown left` 하나로는 `SetWalkMotion` 이 **1회**만 불린다(조이스틱도 같다 —
  방향이 바뀔 때만 KeyPress 를 내므로). 프로브에서 지속 이동을 만들려면 `move` 를
  계속 보내야 하고, 그렇게 하면 실제로 걷는다(프레임 67.5% 차이로 확인).
* 그런데 그 상태에서 OK 를 눌렀다 떼도 `0x9BE74` 는 0 이었다 — 릴리스 시점의
  메인상태가 1 이 아니다(공격 모션 2, 또는 이미 서 있음). 메인상태 값은 실측이다:
  `SetWalkMotion`→1 (0x98F84), `SetStandMotion`→0 (0x98FF8), `SetAttackMotion`→2
  (0x988A8).

즉 **후크가 억제한다는 것은 숫자로 증명됐고, 사용자가 보는 절반(`SetStandMotion`)
은 오프라인에서 걸리지 않았다.** 기기에서 봐야 한다(§38.7).

### 38.6 인수 기준 — 통과한 것

```
bash tools/ab_905.sh head      # HEAD, patched vs --no-patch → 1,241 동일
bash tools/ab_905.sh mine      # 변경 후, 같은 비교        → 1,241 동일
python tools_re/ab_compare.py out/ab-head-new/<장면> out/ab-mine-new/<장면>
```

| 장면 | HEAD 대비 |
|---|---|
| newgame 119 / ingame 543 / shop 134 / online 109 / npcmenu 336 | **1,241 전부 바이트 동일** |

`hl5_core` 를 건드렸으므로 patched-vs-`--no-patch` 만으로는 부족하고(그 비교는
양쪽에 같은 변경이 들어간다), **HEAD 필름과 직접 대조**한 것이 위 표다. 셋 다
기본 off 이거나 단일 손가락에서 id 0 을 내므로 이것이 기대값이다.

그 밖에: `cargo test -p hl5_core` **57개**, `--selftest` **29개**,
`cargo check -p hl5_android --target aarch64-linux-android` 통과.

**`--keep-walking` 을 `ab_905.sh` 에 넣지 마라.** `ingame.txt` 에 게임플레이
`key select`(−5) 릴리스가 여럿이고 각각 `SetMoveKeyChk` 넷을 건너뛰므로, 켜면
그 비교가 무엇을 증명하는지 말할 수 없게 된다. 후크의 증거는 §38.5 의 카운터다.

### 38.7 기기에서 봐야 하는 것 — 그리고 눈으로 판정할 수 없는 것

* **엘릭서는 새로 사야 한다.** 지급 표는 새로 나가는 와이어 레코드만 바꾼다.
  가방에 **한 칸, 스택 20** 으로 들어와야 한다(`NewBagNetItem` 의 스택 병합이
  `(kind|index<<8)` 로 맞추고 상한이 99다). 스무 칸이면 `+0x14/+0x15` 가 (11,8)이
  아닌 것이다. 눌러서 **{단축키등록, 버리기}** 가 나오면 성공이고, {사용하기,
  버리기} 면 아직 kind 18 이다. 회복량은 **250 이 아니라 큰 수를 기대하라**(38.3).
* **동시입력은 `adb` 로 시험할 수 없다** — `input tap` 은 손가락 하나를 눌렀다
  바로 떼고 `input swipe` 도 단일 포인터다. 사람 손이어야 한다.
* **"이동이 멈춘다"로는 고침 둘을 구분할 수 없다.** 포인터 id 회귀(§37.6)와
  `KeyRelease` 원본 버그(§37.7)가 같은 증상으로 보인다. 스틱을 잡고 OK 를 뗀 뒤
  **`gv+0x125` 가 1 인지**(스틱을 아직 쥐고 있는가)와 **`gv+0x278` 이 −1 이 아닌지**
  (각도가 살아 있는가)를 따로 봐라. `gv` 는 `tools_re/guest_objects.txt` 에 있다.
* **건슬링어로 한 번은 해야 한다.** 38.1 의 꼬리를 빼먹었는지 잡을 수 있는 유일한
  방법이고, 그 클래스만 잡을 수 있다: 반복 공격 뒤 OK 를 떼고 사격이 멈추는가.
* **후크가 오작동하면 `-KeepWalking off`** — 또는 debuggable 벤치 빌드에서
  `<files>/hl5_keep_walking.txt`. 이 셸의 동작 변경 중 기기에서 한 번도 돌지 않은
  것은 이것뿐이라, 다른 레버와 같은 탈출구를 붙여 뒀다.
* **범위 밖으로 남긴 것**: GUNNER·KNIGHT 의 클래스 래퍼(0x87E8C / 0xAAAC8)가
  `bl HERO::KeyRelease` **뒤에** 모션을 강등한다(`GetMotion()==0xf → SetMotion(0xe)`,
  `0x1e → 0x1d`). 대시 *도중* 취소는 거기 있고 후크 둘이 더 필요하다 — **다른
  버그이므로 다른 단계, 다른 A/B 로 한다.** 기기에서 "대시 중 버튼을 누르면
  대시가 죽는다"가 여전히 보이는 것은 **예상값**이다. 대시가 *끝난 뒤* 스틱을
  쥐고 있는데 서는 것은 §38.8 이다.

### 38.8 탭 공격의 서는 자리는 `KeyRelease` 가 아니다 (2026-08-30)

§38.5 가 오프라인에서 `0x9BE74` 를 한 번도 못 건드린 이유가, 사용자가 기기에서
본 증상의 전부다. 공격·스킬·대시를 **탭**하면 릴리스 순서는 이렇게 된다:

1. `KeyPress(OK)` → `SetAttackMotion` → 메인상태 **2**
2. 같은 틱이거나 직후 `KeyRelease(OK)` → `GetMainState() != 1` 이라
   `SetStandMotion` 분기는 **원본도 후크도 안 탄다**
3. 모션이 끝나면 공격 종료 경로가 `HERO::SetStandMotion` 을 부른다
4. 그 함수가 `gv+0x278` 을 −1 로 지운다 (0x990EC)
5. 조이스틱은 아직 잡혀 있다 (`gv+0x125 == 1`, `gv+0x11c` ∈ −1..−4) —
   포인터 id 배선은 이 증상과 무관하다
6. `TouchMoved` 는 방향이 **바뀔 때만** `KeyPress` 를 내므로, 같은 방향을
   유지한 채로는 걷기가 다시 안 걸린다. 사용자가 말한 "다른 방향으로 한번
   전환했다가 다시 왼쪽으로" 가 정확히 이 문이다

그래서 `keep_walking` 은 후크가 **둘**이다. `HERO::KeyRelease` 는 §38.1 그대로
(플래그 + 건슬링어 꼬리). `HERO::SetStandMotion` 은 `gv+0x125 == 1` 이고
`gv+0x11c` 가 −1..−4 일 때만 `SetWalkMotion(그 방향)` 으로 바꾸고, 스틱이
없거나 데드존(−999)이면 원본대로 선다. 메인상태가 이미 1 이면 서기만 건너뛴다
— KeyRelease 자신의 호출이 그 경우다.

대시 *종료 후* 걷기 재개는 이 후크가 덮는다. 대시 *도중* 버튼으로 모션이
강등되는 것은 38.7 의 래퍼이고 아직 안 건드린다.

## 39. 유물함이 물건을 준다 — 그리고 지급되던 장비는 전부 망가져 있었다 (2026-08-21)

사용자 보고: **캐시상점에서 유물함을 사도 가방에 안 들어온다.** 맞다 — §32.9 가
"대상 테이블을 유도할 수 없다"로 넷 다 일부러 거절해 두었다(`shop.rs` 의
`"random box with no derivable target"`). 그것을 열면서 **훨씬 큰 것이 나왔다:
kind ≤ 10 으로 지급하던 아이템은 스탯이 0 이고 아무 클래스도 못 입는 상태로
가방에 들어가고 있었다.** 공허의 목걸이가 그랬다.

### 39.1 게임이 내용물을 말해 준다 — `tips.dat` tip 24

유물함 넷(`item_18.dat` 15~18행, 카탈로그 id 18/19/20/21, 1,500 / 2,000 / 2,500 /
3,000원)은 **서로 구분되지 않는다**: 아이콘 필드가 넷 다 `df 00`(223), 설명문도
넷 다 `랜덤 아이템을 획득합니다.` 로 같고, 다른 것은 이름과 가격뿐이다. 그래서
"유도할 수 없다"는 §32.9 의 판단은 그 자리에서는 옳았다.

아카이브를 통째로 뒤지면 답이 있다. `/c/csv/tips.dat` 24번:

```
유물함에서는 제한 레벨이 낮아진 아이템을 얻을 수 있습니다.
```

**제한 레벨은 장비에만 있다.** 그리고 그 기전은 바이너리에 이름까지 붙어 있다 —
`EquipItemInfo::GetRelieveLevelLimit`(0xA835C)가 세 옵션 슬롯 중 **종류가 0x6c**
인 것의 값을 더하고, `GetLevelLimit`(0xA83B0)이 기본 제한(`+0x15d`)에서 그것을
뺀다. 즉 유물함은 **레벨완화 옵션이 붙은 장비**를 팔았다.

그 밖에 "유물"이라는 시스템은 **없다**: 그 단어는 아카이브에서 이 넷과 퀘스트
아이템 하나(`item_14` 37행 오래된 유물), 퀘스트·맵 대사, 그리고 이 팁에만 나오고
UI 텍스트표(`common_text`/`menu_text`/`ingame_text`)에는 0곳이다.

### 39.2 지급되던 장비가 망가져 있었다 — 와이어가 표를 덮어쓴다

`BagItem::NewBagNetItem` 의 kind ≤ 10 갈래(0xA574C)는 두 단계다:

```
0xA5764  bl EquipItemInfo::EquipItemInfo
0xA5790  bl ItemTable::GetItemTableInfo(obj, item+4, item+5)   ; 표에서 채운다
0xA57D8~ ItemInfo+0x150..+0x164 를 _NET_ITEM_ 에서 덮어쓴다     ; 그리고 지운다
```

**전부 0 인 꼬리는 "표 값을 쓰라"가 아니라 "0 을 쓰라"다.** 덮어쓰이는 것:

| ItemInfo | 무엇 | 0 이면 |
|---|---|---|
| +0x150 / +0x152 / +0x156 / +0x158 / +0x15a | 스탯 다섯 | 공격력도 방어력도 없다 |
| **+0x155** | **착용 계층** | **아무 클래스도 못 입는다** |
| +0x15d | 기본 제한 레벨 | (0 이면 실효 1 — 39.4) |
| +0x15f~+0x164 | 옵션 종류·값 셋 | 옵션 없음 |

착용 게이트는 `EquipItemInfo::IsEquipPossible`(0xA81AC)이고 슬롯 8항 점프테이블
이다. 투구·갑옷·장갑·장화·장신구는 공통 검사 0xA8214 로 모인다:

```
class 1·2 (ROGUE, GUNNER)   → +0x155 == 5        (0xA82A0)
class 0·3 (WARRIOR, KNIGHT) → +0x155 == 5 또는 6  (0xA8224)
```

무기(kind ≤ 4)와 방패(kind 9)는 0xA8248 로 가서 `+0x155` 를 **히어로 자신의
클래스**와 비교한다.

> **따라서 §32.9 가 공허의 목걸이를 `item_10` 17행으로 재사상한 목적 —
> "kind 18 로는 착용이 불가능하니 착용 가능한 자리로 옮긴다" — 은 그날부터
> 달성된 적이 없다.** 목걸이는 스탯 0, 계층 0 으로 가방에 들어갔고 어느
> 클래스도 입을 수 없었다. 이번 변경이 그것도 고친다.

### 39.3 표의 그 21바이트를 그대로 실어 보낸다 — 다만 memcpy 는 아니다

`ItemTable::LoadItemTable` 은 커서 하나로 레코드를 읽는다:

```
+0x154 ← sb+4      +0x155 ← sb+5      (0xA3EEC / 0xA3F00)
+0x15c ← sb+0xc    +0x15d ← sb+0xd    (0xA3F6C / 0xA3F80)
그 뒤 3회 루프: [fp+0xf] → 옵션 종류, [fp+0x10] → 옵션 값, fp += 2  (0xA3F9C~)
```

**옵션이 레코드에서도 종류·값 교대**로 놓여 있고, 와이어도 같은 순서다
(`_NET_ITEM_+0x47..+0x4c`). 그래서 대응이 1:1 이다:

```
record[sb + j]  ↔  _NET_ITEM_ + 0x38 + j        j = 0..0x14   (21바이트)
```

`+0x15d` 가 `sb+0xd` 이면서 동시에 `record[len-8]` 이므로 **`sb = record[len-21]`**
이다. 이 두 오프셋은 서로 독립적으로 확인됐다 — 레벨 필드는 **끝−8** 이고,
사용자가 준 커뮤니티 자료의 레벨 14개(수르트 34, 무스펠 57, 그람 82,
기사의징표 1 …)를 전부 재현한다. 그러면 착용 계층은 **끝−16** 이고, 그 값이
설계된 데이터처럼 읽힌다:

| 표 | 끝−16 |
|---|---|
| `item_00`/`01`/`02`/`03` | 전부 0 / 1 / 2 / 3 — **무기의 표가 곧 클래스다** |
| `item_09` (방패) | 81행 전부 **3 = 나이트 전용** |
| `item_05`~`08` (방어구) | **5 가 41행, 6 이 40행**, 7 이 몇 행 |
| `item_10` (장신구) | 5 가 17행, 7 이 1행(신비조사회링) |

그리고 이것이 같은 자료와 교차검증된다: 자료가 "워리어 세트"라고 적은
푸른비룡·지크문드가 **6**, "공용"이라고 적은 아체야키가 **5**, 방패는 나이트.

> **그러나 통째 복사는 틀린다.** 파일의 u16 은 리틀엔디안
> (`StaticUtil::ByteToInt16`, 0x1437EC: `p[i] | p[i+1]<<8`), 와이어는 빅엔디안
> (`NETWORK::readNet16`, 0xC7C2C: `buf[5] | buf[4]<<8`). 그대로 memcpy 하면
> **스탯 다섯이 전부 바이트가 뒤집힌다.** `net_item_granted` 는 21바이트를 옮기고
> `EQUIP_U16_AT` 의 다섯 자리만 스왑한다. 단위시험
> `a_granted_equipment_record_carries_the_rows_stats_byte_swapped` 가 그 21바이트를
> 한 바이트씩 못박는다.

### 39.4 제한 레벨만 통과시키지 않는다 — 그리고 그것이 팁의 내용이다

블록에서 딱 한 바이트, `+0x15d`(기본 제한 레벨)만 0 으로 보낸다. 그러면
`GetRelieveLevelLimit` 이 옵션과 무관하게 `subge r0, r2, #1` 로 **−1** 을 돌려주고
`GetLevelLimit` = `0 − (−1)` = **1** 이 된다. 어느 레벨이든 착용 가능하다.

원본은 0x6c 옵션으로 같은 자리에 도달했다. 완화량이 얼마였는지는 복원할 수 없으므로
**그 숫자를 지어내는 대신 기본값을 0 으로 보낸다** — 도착지는 같고 발명이 없다.

### 39.5 무엇을 주기로 했나 — 그리고 어디까지가 결정인가

`GrantTable::records` 는 **순수 함수**다(부팅 때 만든 표에서 답하고 살아 있는
`Vm` 이 없다). 그래서 히어로의 클래스를 물어볼 수 없고, `+0x155` 를 클래스와
비교하는 **무기(kind ≤ 4)와 방패(kind 9)는 애초에 후보가 아니다.** 남는 것이
공통 검사로 가는 **투구·갑옷·장갑·장화(kind 5~8)** 이고, 그 넷은 **행번호로 세트가
정렬**돼 있다(색인 i 가 네 표에서 같은 세트). 장신구(kind 10)는 18행뿐이고 전부
레벨 1 짜리 기초 악세라 뺐다.

등급대는 가격 순이다:

| 상자 | 가격 | 행 | 조각 수 | 아이템 레벨 |
|---|---|---|---|---|
| 작은 유물함 | 1,500원 | 20~30 | 44 | 18~47 |
| 유물함 | 2,000원 | 40~50 | 44 | 44~68 |
| 큰 유물함 | 2,500원 | 60~70 | 44 | 62~98 |
| 오래된 유물함 | 3,000원 | 74~80 | 28 | 70~98 |

**행번호가 등급이라는 것은 추측이 아니다** — 사용자가 준 실서비스 시절 경매장
시세표가 세트를 정확히 행번호 순으로 매긴다(가시나무 23행 50만골드 →
지크문드 80행 2000만골드). 그 목록 46개 이름 중 42개가 이 표들의 행으로 풀린다.

> **이 절에서 결정인 것은 밴드의 경계뿐이다.** 팁이 "장비"를 확정하고, 시세표가
> "표의 어느 쪽이 좋은가"를 확정하고, 네 경계는 우리가 골랐다. 그렇게 적어 둔다.

계층 7 은 **아무도 못 입는다**(`MakeSocket` 0xA123C 도 거절한다). `item_05` 81~89,
`item_06` 81~92, `item_10` 16 이 그것이고 **어느 밴드에도 없다** — 걸러 내는 코드가
있고, 밴드가 나중에 움직여도 그것이 유지되게 한다
(`an_unwearable_row_never_enters_a_relic_pool`).

### 39.6 지급 표를 부팅 로그에 찍는다

거절만 찍고 **무엇을 주는지는 안 찍고 있었다.** 목걸이가 오래 망가진 채로 있었던
것이 그 비대칭 때문이다. `GrantTable::summary()` 가 이제 한 줄씩 낸다:

```
free-shop: row 18 -> kinds {5, 6, 7, 8} rows 20..=30, 44 piece(s), groups {5, 6}, item level 18..=47 (sent as 1)
free-shop: row 46 -> kind 10/17 x1, group 5
free-shop: row 4  -> kind 11/8 x20
```

출하 아카이브에서 **거절은 이제 0개**다(카탈로그 30행에 계층 0 인 행이 없다).

### 39.7 인수 기준

* `cargo test -p hl5_core` **63개** (`shop.rs` 에 유물함 넷 + 스탯 왕복 + 계층 7 배제).
* 1,241 프레임 5장면 A/B — `free_shop` 이 기본 off 라 무영향, HEAD 대비 확인.
* `--selftest` 29개, `net_item` 은 **파이썬과 바이트 동일 그대로**(`localserver_ab.rs`
  5개 통과) — 지급용 `net_item_granted` 를 따로 두었기 때문이다. 모듈 계약상
  지급은 "더해도 되는 유일한 것"이므로 이 분리가 그 계약을 지킨다.

### 39.8 기기에서 볼 것

1. 유물함 넷을 각각 사라. 가방에 **투구/갑옷/장갑/장화 중 하나**가 들어와야 한다.
2. **툴팁에 공격력·방어력이 있어야 한다.** 0 이면 39.3 의 엔디안이 틀린 것이다.
3. **입어 봐라.** 계층 5 는 아무 클래스나, 계층 6 은 워리어·나이트만 — 로그로
   6짜리를 입으려 하면 거절되는 것이 **정상**이다(게임의 원래 규칙).
4. **제한 레벨이 1 로 보여야 한다.** 아이템 이름은 지크문드인데 레벨이 1 이면
   맞다 — 그것이 유물함이 파는 것이다.
5. **공허의 목걸이를 다시 사서 입어 봐라.** 이번에 처음으로 착용된다.
   옛 세이브에 있는 것은 스탯 0 인 채로 남는다 — 지급 표는 새로 나가는 와이어
   레코드만 고친다.
6. 가방에 빈칸이 있는 세이브로 하라(§38.7 의 96칸 주의).

## 40. 유물함이 무기를 굴리고, "+랜덤 옵션"이 문구에서 사실이 됐다 (2026-08-21)

사용자 보고 둘. **유물함에서 무기는 안 나오나** — §39.5 가 일부러 뺐다. 그리고
**나온 아이템 이름에 `+랜덤 옵션` 이 붙어 있는데 문구일 뿐 아무것도 랜덤이
아니다** — 문구를 지우든지, 그 자리에 실제로 능력치를 올려 주는 랜덤 옵션을
넣든지 해야 한다는 것. 이 절의 작업은 **payload 가 없는 환경**에서 했다 —
디스어셈블 불가, VFS 열람 불가. 그래서 어디까지가 기록에서 증명되고 어디부터가
결정인지가 §39 보다도 중요하다.

### 40.1 문구는 지울 수 없다 — 이름은 게스트의 표에서 온다

`NewBagNetItem` 의 장비 갈래는 `ItemTable::GetItemTableInfo`(0xA5790)로 표에서
객체를 채우고, 와이어가 덮어쓰는 것은 `ItemInfo+0x150..+0x164` 뿐이다(§39.2).
이름은 그 블록 밖이다. 즉 `+랜덤 옵션` 이 붙은 행 이름은 호스트가 어떤 와이어
레코드를 보내든 그대로 화면에 나온다. 남는 길은 하나 — **문구를 사실로
만든다.** 실제로 랜덤인 값을 실어 보낸다.

### 40.2 옵션 슬롯은 이 환경에서 건드릴 수 없다 — `subge` 의 ±1

원본의 기전은 옵션 3쌍(`+0x15f..+0x164`)이고 해독된 종류는 0x6c(레벨완화)
하나다(§39.1). 그런데 `GetRelieveLevelLimit`(0xA835C, 84바이트)에 대한 두
기록이 **±1 을 남긴다**: §39.1 은 "0x6c 값을 더하고 기본에서 뺀다", §39.4 는
"`subge r0, r2, #1` 로 옵션과 무관하게 −1". 양쪽 다 지금 출하된 조합
(base 0, 옵션 없음 → 실효 1)은 설명하지만, **(base=B, 옵션 v) 가 B−v 인지
B−v+1 인지, (base=B, 옵션 없음) 이 B 인지 B+1 인지는 저 84바이트를 읽어야
갈린다.** 잘못 고르면 제한레벨이 0·음수·2 로 나간다. 그래서 옵션 슬롯과
`+0x15d` 는 §39 그대로(전부 0, 실효 1) 두었고, 확정은 payload 세션의 숙제다
(40.6).

덧붙여 심볼표에서 이번에 처음 눈에 띈 것: 게스트에는
`ItemTable::MakeItemOption(ItemInfo*)`(0xA10E8, 140 B),
`SetItemOption(ItemInfo*, signed char)`(0xA0FF8),
`LoadItemOptionTable()`(0xA0BA4) 이 있다 — **게임 자신의 랜덤 옵션 롤러와 옵션
표 로더**다. 한 번도 조사된 적이 없다(ANALYSIS.md 언급 0회). 이 셋을 읽으면
발명 없이 원본 방식 그대로 옵션을 굴릴 수 있다.

### 40.3 랜덤은 스탯 다섯 워드에 실었다

kind ≤ 10 에서 스탯 다섯(`+0x150/+0x152/+0x156/+0x158/+0x15a`)은 **와이어가
표를 덮어쓰는 권위**이고(§39.2), 가방 툴팁이 그 값을 그대로 보여준다는 것은
§39.8 의 엔디안 검사가 존재하는 이유 그 자체다. 여기가 안전한 자리다 —
의미를 모르는 바이트는 건드리지 않고, **행이 이미 갖고 있는 0 아닌 스탯만
스케일한다.**

`shop.rs::boost_stats`: 0 이 아닌 스탯마다 독립적으로 **+10~30%**(최소 +1,
상한 0x7fff — 게스트 산술이 halfword 를 부호 있게 볼 가능성 차단), 0 은 0
그대로. 엔트로피는 `GrantTable::cursor` → splitmix64 — 오브 상자가 세운 규칙
그대로 **결정적**이라 재생이 재현된다. **이 절의 결정들**: 범위 10~30%,
스탯별 독립 롤, 최소 +1, 상한 0x7fff(이미 상한 이상인 워드는 깎일까 봐
건드리지 않는다), 그리고 옵션 슬롯 대신 스탯에 싣기로 한 것 자체(근거는
40.2~40.3 의 사실들이지만 선택은 선택이다). 2011년 운영 서버의 수치는 복원
불가능하다.

### 40.4 무기는 클래스를 알아야 준다 — 이제 안다

§39.5 가 무기(kind ≤ 4)·방패(kind 9)를 뺀 이유는 `IsEquipPossible` 이 그 둘을
`+0x155 == 히어로 클래스`(0xA8248)로 갈랐고 `GrantTable::records` 가 순수
함수라 클래스를 물을 수 없어서였다. 해소는 이렇게 했다:

* **어디서 묻나**: `connect` HLE 팔(`files.rs`). 게스트가 소켓을 여는 그
  순간은 호스트가 `Vm` 전체와 아직 삽입 전의 `Session` 을 동시에 쥐는 유일한
  시점이다 — `Session` 은 `Files.fds` 안에 살아서 그 뒤로는 구조적으로 `Vm`
  을 볼 수 없다(차용). 캐시샵은 방문마다 소켓을 새로 열고 다시 로그인하므로
  (§16 — `netConnect` 는 모드마다 새 소켓) **connect 시각 = 구매 시각**이고,
  한 소켓이 열려 있는 동안 클래스는 바뀔 수 없다.
* **무엇을 읽나**: `shop.rs::hero_class` — `gv+0x1474`(`warehouse.rs` 의
  `GV_HERO`, `ProcPacket_Inventory` 의 입고 팔이 쓰는 바로 그 필드, §36)에서
  HERO 포인터를 읽고 `HERO::GetPlayerClass`(0x88914, `[this+0x22c]`, §38.1)를
  `Vm::call` 로 부른다. HLE 안에서의 게스트 재진입은 qsort 전례(§'셀프테스트'),
  게스트 전역을 심볼로 읽는 것은 `try_skip_network` 전례다. 못 읽으면(부팅
  로그인 소켓처럼 HERO 미적재, 또는 0..3 밖) `None` → §39 그대로 방어구만.
  grants 없는 세션은 아예 묻지 않는다 — `--local-server` 재생에 없던 게스트
  재진입을 만들지 않는다.
* **무엇을 주나**: `item_00..03`(표=클래스, §39.3 전수조사) + `item_09`(81행
  전부 3=나이트). **선정은 행 밴드가 아니라 방어구 밴드의 레벨 스팬이다** —
  행순=티어의 증거(경매 시세표, §39.5)는 방어구 이름들이었고 무기에는 그런
  증거가 없다. 대신 레벨 바이트(`record[len-8]`)는 §39.3 이 14개 값으로
  검증한 규칙이고 19개 표가 파서(커서 하나)를 공유하므로 무기 표에서도 같은
  자리다 — 그 14개 중 수르트·그람은 이름으로는 무기로 읽히지만 §39.3 은
  표까지는 적지 않았다. 근거는 표 확인이 아니라 **형식 공유**다. 81..85(공허
  무기)는 `RELIC_ROW_MAX = 80` 으로 배제 — 자기 진열대가 따로 있는 캐시샵
  물건이다. 스팬은 착용 가능한 방어구 **풀**에서 계산한다 — 그룹 7 행이
  밴드에 들어와도 스팬을 늘리지 못한다(`roll_relic`).
  **이 항목에서 결정인 것은 레벨 스팬 규칙과 80 울타리다** — §39.5 의 밴드
  경계와 같은 지위이고, 실데이터에서 무엇을 무는지는 40.6-4 의 부팅 로그
  항목이 확인한다.
* **다른 클래스의 무기는 뽑히지 않는다**(`class_may_draw`). 아는 클래스와 다른
  무기를 주는 것은 §39.2 목걸이 꼴(가방에 있는데 영영 못 입는 아이템)의
  재현이라 풀에서 거른다. 방어구의 그룹 6 은 §39 대로 남는다 — 로그가 뽑으면
  원본 규칙대로 못 입는 것이고, 그것은 게임의 규칙이지 버그가 아니다.

와이어는 §39.3 의 21바이트 블록 그대로다(무기 행의 블록에는 자기 클래스가
`+0x155` 로 실려 간다). `ProcPacket_Shop` 의 특례는 `kind == 18 && 44..48`
뿐이라(§25.2) kind 0..3 레코드는 재사상 없이 곧장 `NewBagNetItem` 으로 간다.

### 40.5 인수 기준

* `cargo test -p hl5_core` **62 + 5 = 67개**(+4: 클래스 게이트, 레벨 스팬과
  공허행 배제, 스탯 롤 범위, 결정성). `localserver_ab` 5개 **그대로 통과** —
  `hero_class` 는 grants 있는 세션만 읽으므로 기본 세션의 바이트는 정의상
  불변이다.
* 1,241 프레임 A/B 와 `--selftest` 는 **이 환경에서 돌릴 수 없다**(payload
  없음). `free_shop` 기본 off 라 코드 경로 자체가 없지만, HEAD 대비 확인은
  payload 있는 세션이 해야 한다.
* `cargo check -p hl5_probe` 통과. `hl5_android` 의 aarch64 체크는 이 환경에
  그 타깃의 std 가 없어 불가(코드 변경은 `hl5_core` 안에서만).

### 40.6 기기에서 볼 것, 그리고 payload 세션의 숙제

기기(§39.8 에 더해):

1. **유물함을 반복 구매하라. 자기 클래스의 무기가 섞여 나와야 한다** —
   워리어=대검, 로그=단검, 건슬링어=권총, 나이트=장창+방패. **다른 클래스의
   무기가 나오면 회귀다.**
2. **같은 이름 두 개를 뽑아 스탯을 비교하라.** 대체로 달라야 하고(스탯별
   21개 값의 독립 롤이라 가끔은 겹칠 수 있다), 표의 기본값보다 10~30% 높아야
   한다. **표에서 0 이 아니던 스탯이** 0 으로 오면 §39.3 의 엔디안 계열
   회귀다(0 이던 스탯은 0 이 설계다).
3. 부팅 로그의 `free-shop:` 줄이 밴드마다 `N class-gated, M named +랜덤 옵션`
   을 찍는다 — 기기에서 뽑히는 것들과 대조하라.
4. 옛 세이브의 아이템은 안 변한다(§39.8 5번과 같은 이유).

payload 세션:

1. **`GetRelieveLevelLimit` 84바이트를 읽어 ±1 을 확정하라**(40.2). 확정되면
   기본 레벨을 통과시키고 0x6c 옵션으로 실효 1 을 만드는 원본 방식으로 바꿀 수
   있다 — 지금은 base 0 이 그 자리를 대신한다.
2. **`LoadItemOptionTable` 이 여는 파일과 형식, `MakeItemOption` 의 분포**를
   읽어라(40.2). 그러면 스탯 스케일 대신 원본 옵션을 굴릴 수 있다.
3. **스탯 다섯 워드의 의미**: 툴팁 렌더러(`CommonUi::DrawItemDiscription`
   0x7B350, 4,816 B / `DrawItemInfo` 0x80960)가 다섯 워드를 어떤 이름으로
   그리는지. 40.3 의 "전부 양의 전투 스탯" 가정이 여기서 확정되거나 깨진다.
4. 무기 표의 레벨 스팬 선정(40.4)이 실데이터에서 어떤 행들을 고르는지 부팅
   로그로 확인 — 스팬이 이상한 행(퀘스트·더미)을 물면 그때 손본다.

## 41. `probability.dat` 를 전부 열었다 — 소켓·제련·오브·드랍 확률이 한 파일에 있었다 (2026-08-22)

사용자 요구 둘. **유물함으로 얻은 아이템에 소켓확장이 안 된다 — 되게 해라.**
그리고 **오브 성공확률·제련 성공확률·전설 드랍률을 올리려는데 현재 값을 알려
달라.** 앞의 것은 코드가 들어갔고, 뒤의 것은 이 절이 답이다.

이 절은 §40 과 달리 **payload 가 있는 환경**에서 했다. 그래서 전부 디스어셈블과
파일 바이트에서 나오고, 결정은 하나(§41.7 의 소켓 하한)뿐이다.

### 41.1 `LoadProbablityTable` 은 51행을 열한 블록으로 쪼개고 첫 블록은 전치한다

`/c/csv/probability.dat`, 614 바이트. 프레이밍은 다른 표와 같다 —
`[u16 count=51]` + 51 × (`[u16 reclen=10]` + 5 × u16 LE) = 2 + 612 = **614, 정확히**.

`ItemTable::LoadProbablityTable`(0xA0C98, 864 B)이 그 51행을 **용도별 열한 블록**
으로 흩어 놓는다. 목적지가 빈틈 없이 이어지는 것이 이 해독의 자기검증이다:

| 블록 | 목적지 | 행 | 값/행 | 소비자 |
|---|---|---|---|---|
| A | `this+0x78` | 0..5 (6) | 5 | `MakeSocket` — **전치** |
| B | `this+0xB4` | 6..10 (5) | 3 | `ApplyOrbCombine` (0xA1F84) |
| C | `this+0xD2` | 11..20 (10) | 5 | `ApplyItemRefine` **일반** (0xA2BA0) |
| D | `this+0x136` | 21..30 (10) | 5 | `ApplyItemRefine` **고급** (0xA29CC+6) |
| E | `this+0x19A` | 31..35 (5) | 3 | (소비자 미확정) |
| F | `this+0x1B8` | 36..40 (5) | 5 | `ApplyItemDecompose` (0xA63B8) |
| G | `this+0x1EA` | 41..46 (6) | **1** | `MakeSocketItemCompose` (0xA119C) |
| H~K | `this+0x1F6/0x1FC/0x202/0x208` | 47/48/49/50 | 3 | I 는 `ApplyOrbCombine` (0xA20C8) |

블록 A 만 전치다. 루프가 `strh r0, [r6], #0xc` 로 **12바이트씩** 나아가면서
목적지를 `this + 0x78 + 2*행` 에서 시작하므로(0xA0CDC~0xA0D04),
원소 `k`·행 `r` 이 `this + 0x78 + 12*k + 2*r` 에 앉는다. `MakeSocket` 이
`this + 0x78 + 12*grade` 에서 halfword 여섯을 **연속으로** 읽으므로(0xA1284),
읽히는 것은 **행 0..5 의 원소 `grade`** 다. 즉 파일의 열이 곧 등급이다.

> **`reclen` 을 데이터로 읽지 마라.** 각 행의 첫 halfword 는 10(=reclen)이고
> 로더는 그것을 읽어 **버린다**(0xA0C14 의 반환값이 죽는다). 순진하게 여섯을
> 데이터로 읽으면 모든 임계값이 한 칸 밀리고 첫 임계값이 `10` 이 되어
> "0소켓 1%" 같은 그럴듯한 헛것이 나온다. 이 조사가 한 번 정확히 그렇게 틀렸다.

닫힘 확인: 6+5+10+10+5+5+6+1+1+1+1 = **51행**, 마지막 행이 파일의 614바이트
끝에서 끝난다. 그리고 열한 블록의 목적지가 `0x78 → 0x20E` 로 빈틈 없이 붙는다.

### 41.2 소켓 개수 — 표가 둘이고 전설은 한쪽에서 아예 거절된다

`ItemInfo+0x15c` 는 소켓 **개수가 아니라 등급**이다. `ItemTable::MakeItemOption`
(0xA10E8)이 그 바이트로 랜덤옵션을 고르는 것이 증거다:

| `+0x15c` | `MakeItemOption` | `MakeSocket` |
|---|---|---|
| 0 | 옵션 없음 | 열 0 |
| 1 | 옵션 1개, band 0 | 열 1 |
| 2 | 옵션 2개, band 0+1 | 열 2 |
| 3 | 옵션 1개, band 2 | 열 3 |
| **4** | **옵션 없음** | **거절** (0xA1258 `beq`) |
| 5 이상 | 옵션 1개, band 2 | 열 4 (4 이상 클램프) |

**그리고 4 는 전설이다.** 아카이브가 스스로 말한다 — 등급 사다리는
`노멀 / 레어 / 에픽 / 영웅 / 전설 / 세트` 이고(도움말이 "전설 아이템: 붉은색"),
"최고레벨 전설 양손도끼인 무닌/**그람**/**쇼픈**/**오드**/**알케인**" 이라고 적은
그 이름들이 `item_00.dat` 에서 전부 `+0x15c == 4` 다. 5..18 은 세트이고
**세트 하나가 네 방어구 표에 한 행씩**(14세트 × 4조각 = 56행) 들어 있다.

`Rand(0,999)` 로 굴리고 임계값 여섯을 걸어 **처음으로 밑도는** 자리가 개수다
(0xA1298 `blt`); 여섯을 다 지나면 0 이다(0xA12A8).

| 등급 | 0소켓 | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|---|
| 노멀·레어·에픽 (0·1·2) | 50% | 20% | 15% | 10% | 4% | 1% |
| 영웅 (3), 세트 (5 이상) | 70% | 20% | 10% | 0 | 0 | 0 |
| **전설 (4)** | — | — | — | — | — | — (**굴림 자체가 없다**) |

`MakeSocket` 의 호출자는 **아이템이 생기는 순간**뿐이다 — `NewItemEx`,
`MapItem::NewDropItem`, `ApplyNormalMix`, `ApplySpecialMix`. 그리고 별개의
`MakeSocketItemCompose`(0xA1174)가 **조합**에서만 불리는데(`ApplyItemCompose`
0xA61FC 한 곳), 그쪽은 **등급 게이트가 아예 없다**(유일한 관문이 `kind > 9`,
0xA1188) 그리고 블록 G 를 쓴다:

```
250  500  700  850  950  1000   →  0:25%  1:25%  2:20%  3:15%  4:10%  5:5%
```

`tips.dat` 가 그 둘을 그대로 요약한다 —
`소켓 장비는 조합이나 몬스터를 통해 얻을 수 있습니다.`
**즉 이 게임에 "소켓확장" 기능은 없다.** 소켓은 아이템이 만들어질 때 정해진다.

### 41.3 그래서 유물함 아이템에 소켓이 없었다 — 그리고 그것은 호스트 탓이었다

유물함 아이템은 만들어지지도 조합되지도 않는다. 와이어로 와서
`BagItem::NewBagNetItem` 이 가방에 넣는다. 그 `kind <= 10` 갈래는 와이어를
**`ItemInfo+0x168` 까지 그대로 복사한다**(0xA589C) 그리고 다섯 슬롯
`+0x169..+0x16D` 까지(0xA58A4~0xA58C4). `writeNetItem`(0xC8854)이 순서를 말해
준다 — `+0x165`(제련도) `+0x166`(제련 보너스) `+0x167`(깨짐) `+0x168`(소켓 수)
`+0x169..+0x16D`(슬롯 다섯) = **`_NET_ITEM_+0x4D` 의 아홉 바이트.**

**호스트는 거기에 0을 아홉 개 보내고 있었다.** 그리고 소켓 수 0 은
`EquipItemInfo::IsSocketFull`(0xA8300)에서 `ble`(0xA8310)로 **"꽉 찼다"** 가 되고,
`IsOrbCombinableItem`(0x9F8D0)도 같은 `ble` 로 **거절**한다. 오브를 영원히 못
박는다.

> `localserver.rs` 의 주석이 "`GetItemTableInfo` 가 `+0x168` 을 스스로 0으로
> 만든다(0xA27C8)" 라고 적어 그 아홉 바이트를 무해한 것으로 취급했다.
> **그 문장은 사실이지만 결론이 틀렸다** — `GetItemTableInfo` 는 0xA5790, 즉
> 와이어 복사(0xA57D8~) **앞**에서 돈다. 와이어가 권위다. 이번에 고쳤다.

### 41.4 제련 성공확률 — `Rand(0,999)`, 인덱스는 현재 제련도

`ApplyItemRefine`(0xA292C). `Rand(0,999)`(0xA29A0), 인덱스는 `item+0x165`,
행은 고급이면 `this+0x136+10*제련도`(0xA29CC `add lr,#0x130` + 0xA29D8 `add r3,#6`),
일반이면 `this+0xD2+10*제련도`(0xA2BA0). 갈림은 다섯째 인자
(`[sp+0x54]`, 0xA29A4 `cmp sl,#0`).

임계값에는 보정이 붙는다 — `mla lr, sb, r8, lr`(0xA29EC): `sb` 는 `[sp+0x20]` 의
결과별 부호 바이트 다섯, `r8` 은 넷째 인자(보너스). 아래 표는 **보너스 0** 이다.

결과는 5항 점프테이블(0xA2B60, **case 0 은 `pc+8`**)로 갈린다:

| case | 자리 | 하는 일 |
|---|---|---|
| 0 | 0xA2C98 | 제련도 +1, `+0x166` **+2** → 대성공 |
| 1 | 0xA2C6C | 제련도 +1, `+0x166` **+1** → 성공 |
| 2 | 0xA2A14 | 그대로 → 실패 |
| 3 | 0xA2C58 | `[item+0x167] = 1` → **깨짐** |
| 4 | 0xA2BD0 | 해제 + `ClearEquipItem` → **파괴** |

**성공확률 = case 0 + case 1.**

| 제련 | 일반 성공 | 실패 | 깨짐 | 파괴 | 고급 성공 | 실패 |
|---|---|---|---|---|---|---|
| +0→+1 | **88.0%** | 12.0% | 0 | 0 | **99.0%** | 1.0% |
| +1→+2 | **77.0%** | 23.0% | 0 | 0 | **99.0%** | 1.0% |
| +2→+3 | **66.0%** | 34.0% | 0 | 0 | **99.0%** | 1.0% |
| +3→+4 | **66.0%** | 28.5% | 5.0% | 0.5% | **88.0%** | 12.0% |
| +4→+5 | **55.0%** | 34.0% | 10.0% | 1.0% | **88.0%** | 12.0% |
| +5→+6 | **55.0%** | 23.0% | 20.0% | 2.0% | **77.0%** | 23.0% |
| +6→+7 | **44.0%** | 23.0% | 30.0% | 3.0% | **66.0%** | 34.0% |
| +7→+8 | **33.0%** | 23.0% | 40.0% | 4.0% | **55.0%** | 45.0% |
| +8→+9 | **22.0%** | 23.0% | 50.0% | 5.0% | **44.0%** | 56.0% |
| +9→+10 | **11.0%** | 23.0% | **60.0%** | **6.0%** | **33.0%** | 67.0% |

§31.6 이 실행으로 관측한 "저급은 9레벨에서 60% 깨짐 / 6% 파괴" 가 마지막 줄에
정확히 나온다 — 독립 재현이다. 그리고 §31.6 의 "고급 행은 임계값 2~4가 전부
1000 이라 파괴 분기가 도달 불가" 도 맞다(블록 D 의 열 셋이 전부 1000).
`+0x165 > 9` 면 결과가 무조건 case 2 로 고정되므로(0xA2A0C `cmp #0x9000000`)
**제련 상한은 +10** 이다.

### 41.5 오브결합 성공확률 — 인덱스는 이미 박힌 같은 오브 수

`ApplyOrbCombine`(0xA1E30). `Rand(0,999)`(0xA1F60), 행은 `this+0xB4+6*n`
(0xA1F84), 그리고 `n` 은 **그 아이템에 이미 박혀 있는 같은 오브의 개수**다 —
0xA1F34 의 루프가 `[item+0x169+k]` 를 넣을 오브와 비교해 세는 것이 `n` 이다.

성공 임계값에는 배수가 곱해진다(`mul r2, ip, r2`, 0xA1F9C). 그 배수는
0xA1EF4 의 `cmp r6, #9` / `moveq r2,#2` / `movne r2,#1` — **소모한 `kind 18`
행이 9(달성의부적)일 때만 2, 그 밖에는 1.** §31.6 의 "성공 배수를 2배" 가
이 세 명령이다.

| 이미 박힌 같은 오브 | 성공 | 달성의부적(×2) |
|---|---|---|
| 0개 (첫 오브) | **40.0%** | 80.0% |
| 1개 | **20.0%** | 40.0% |
| 2개 | **5.0%** | 10.0% |
| 3개 | **1.0%** | 2.0% |
| 4개 | **0.5%** | 1.0% |

### 41.6 전설 드랍률 — `probability.dat` 가 아니라 몬스터별 데이터다

`Monster::SetDropItem`(0xBC910)은 `probability.dat` 를 **한 번도 읽지 않는다.**
굴림은 `Rand(0, 65535)`(0xBCB14, `0x10000-1`)이고, 걸리는 것은 몬스터 자신의
**7단 사다리** `Monster+0x254/0x258/0x25C/0x260/0x264/0x268/0x26C` 다.
`Monster::setEnemyData`(0xC1D7C~0xC1DDC)가 그 일곱을 `ByteToInt32` 로 채운다.

출처는 `/c/csv/enemy_<N>.dat`(0x158D38 의 `/c/csv/enemy_%d.dat`, `N` =
`HERO+0x277` = 난이도, 실재하는 것은 0·1·2). 각 23,190 바이트 / **166 몬스터**,
프레이밍은 `[u16 count][u16 reclen][u8 namelen][name][필드…]` 이고 세 파일 다
마지막 바이트까지 소비한다. 사다리는 **이름 뒤 39바이트** 지점의 u32 일곱이다 —
`r6` 이 이름 길이만큼 움직이기 때문에 고정 오프셋이 아니고, 그래서 처음에
헛값을 읽었다. 확정은 둘이 맞물려서 된다: 디스어셈블이 `r6+0x26`
(`r6 = 레코드시작+4+namelen`)이라고 말하고, **166행 전부에서 단조증가이고
65536 이하인 오프셋이 39 하나뿐**이다(전수 탐색).

`roll < T0` 이면 아무것도 안 떨어지고, 그 뒤 여섯 구간이 등급 여섯에 대응한다:

| | 없음 | b1 | b2 | b3 | b4 | **b5** | b6 |
|---|---|---|---|---|---|---|---|
| 지하 고블린 (일반, 난이도 0) | 20.0% | 16.0% | 2.0% | 1.0% | 0 | **0** | 0 |
| 테오도어 (보스, 난이도 0) | — | 0 | 0 | 50.0% | 10.0% | **1.0%** | 1.0% |
| 테오도어 (난이도 1) | — | 0 | 0 | 57.5% | 11.5% | **1.149%** | 1.151% |
| 테오도어 (난이도 2) | — | 0 | 0 | 65.0% | 13.0% | **1.300%** | 1.300% |

166 중 **150 몬스터**가 사다리를 갖고(박스·풀·광석은 전부 0), 그중 **54 마리만**
b4~b6 에 폭이 있다. 즉 **상위 등급은 이름 있는 몬스터에게만 떨어지고, 가장
희귀한 두 구간은 마리당 약 1%** 다.

> **미확정 하나**: 구간 여섯이 등급 여섯에 1:1 로 대응한다는 것은 폭의 모양
> (희귀할수록 좁다)과 등급이 정확히 여섯이라는 사실에서 온 **추정**이다.
> 핸들러는 0xBCCA8 / 0xBCCF8 / 0xBCDA0 / 0xBCD60 넷뿐이고 서로 다른 것은
> `0xff`(빈 슬롯) 아닌 필드를 몇 개 굴리는지와 `0x51EB851F`(÷100) 퍼센트
> 계산의 개수다 — 품질 사다리인 것은 분명하지만 **"b5 = 전설"을 바이트로
> 증명하지는 못했다.** 확률을 올릴 때는 b4~b6 을 함께 넓히는 것이 안전하다.

### 41.7 무엇을 고쳤나 — 아홉 바이트 중 여섯

`net_item_granted` 의 꼬리 아홉 바이트에 소켓을 싣는다. 굴림은
**게스트 자신의 조합 표**(블록 G)이고, `MakeSocket` 쪽을 안 쓴 이유는 그것이
전설을 거절하기 때문이다 — 그 표를 쓰면 유물함이 주는 가장 좋은 물건이 정확히
소켓 0개가 되고, 그게 사용자가 신고한 증상이다.

* `shop.rs`: `SOCKET_CDF`(블록 G 그대로), `roll_sockets(kind, salt)` —
  `kind > 9` 는 0(게스트도 그렇다), 엔트로피는 §40.3 과 같은 `mix(cursor)`.
  `GrantRecord::sockets` 신설.
* `localserver.rs`: 꼬리 `[3]` = 개수, `[4..9]` = **앞 개수개는 `0x00`(소켓 있음,
  비어 있음), 나머지는 `0xFF`(없음)** — `MakeSocket` 이 남기는 모양 그대로
  (`memset(item+0x169, -1, 5)` 뒤 앞 개수개를 0으로, 0xA12B0~0xA12FC).
  **개수가 0이면 아홉 바이트를 그대로 0으로 둔다** — 배포 서버가 보내는 바이트이고
  `localserver_ab.rs` 가 파이썬과 바이트 동일로 못박는 대상이며, 그 상태에서는
  `IsSocketFull` 이 `ble` 에서 먼저 돌아서므로 읽는 사람도 없다.

**이 절의 결정은 하나다: `SOCKET_FLOOR = 1`.** 조합 표의 25% 짜리 0소켓 버킷을
1소켓으로 접었다. 소켓은 나중에 추가할 수 없으므로(41.2) 그 버킷은 네 번에 한
번씩 원래 신고를 재현한다. 나머지 곡선은 게스트 것 그대로:
**1개 50% / 2개 20% / 3개 15% / 4개 10% / 5개 5%.**

### 41.8 인수 기준

* `cargo test -p hl5_core` **66 + 5개** (소켓 넷 신설: 하한·kind 게이트·
  분포 20만회 ±1pp·와이어 아홉 바이트 전수).
* `cargo run -q -p hl5_probe -- --selftest` **29개**.
* `localserver_ab.rs` 5개 — `net_item` 은 파이썬과 **바이트 동일 그대로**.
* `bash tools/ab_905.sh` 1,241 프레임 (`free_shop` 기본 off 라 코드 경로 없음).
* `cargo check -p hl5_android --target aarch64-linux-android` 는 이 호스트에서
  `windows-sys` 의 dlltool 때문에 안 된다(CLAUDE.md 의 기존 제약, 이 변경과 무관).

### 41.9 그리고 `option.dat` 도 열렸다 — §40.2 의 숙제 절반

`/c/csv/option.dat`, 252 바이트. `ItemTable::LoadItemOptionTable`(0xA0BA4)이
`[u16 count=25]` + 25 × (`[u16 reclen=8]` + `[u8 종류][u8 값][3 × u16]`) 로 읽고
= 2 + 250 = **252, 정확히**. 목적지는 `this+0x74` 이고 레코드는 8바이트다.

세 u16 은 **밴드 셋의 누적분포**이고 셋 다 정확히 1000 에서 끝난다.
`MakeItemOption` 이 등급으로 밴드를 고르고(41.2) `SetItemOption`(0xA0FF8)이
그 밴드를 걸어 종류·값을 뽑는다.

**그리고 §39.4 가 "복원할 수 없다"고 적어 둔 레벨완화량이 여기 있다** —
종류 `0x6c` 행의 값 바이트가 **10** 이다. 밴드별 출현율은 0.1% / 1.0% / 5.0%.

> 로더의 루프 상한은 `0x1E2`(=2+48×10)로 **하드코딩 48행**이고 할당도
> 48×8=384 바이트인데 파일에는 25행뿐이다. 즉 25..47 행은 252바이트 버퍼
> **바깥**을 읽는다. 도달 불가라 무해하다 — 누적분포가 24행에서 정확히 1000 에
> 닿으므로 걷기가 그 앞에서 반드시 멈춘다. 원본의 행동이므로 고치지 않는다.

### 41.10 다음이 할 것

1. **기기 시험**: 유물함을 사서 소켓이 보이는지, 그 소켓에 오브가 박히는지.
   옛 세이브의 유물함 아이템은 **안 고쳐진다** — 지급 표는 새로 나가는 와이어
   레코드만 바꾼다(§39.8·§40 과 같은 주의).
2. **확률 올리기**: 가장 깨끗한 것은 `/c/csv/probability.dat` 를 **호스트가 갈아
   끼워 서빙**하는 것이다 — 게스트 후크 0개, 패치 0개. `StaticUtil::LoadRes` 가
   호스트 FS 를 먼저 보므로(`MC_fsFileAttribute` → `CUSTOM_knlGetResourceID`)
   파일 하나로 끝난다. 드랍률만은 `enemy_<N>.dat` 라 파일이 다르다.
3. **블록 E**(행 31..35, `this+0x19A`)의 소비자를 못 찾았다. 표 모양이
   `[1000,1000,1000] / [500,1000,1000] / [500,800,1000] / [0,500,1000] /
   [0,0,1000]` 로 매우 특징적이라 찾으면 바로 알 수 있다.
4. **등급 ↔ 드랍 구간 대응**(41.6 의 미확정)을 툴팁 색에서 확정하기.
   `BFont::GetColorBy(char)`(0x43AEC)가 `^` 를 붉은색으로 바꾸는 자리이고,
   아이템 이름에 이스케이프를 붙이는 쪽을 찾으면 등급→색→구간이 닫힌다.

## 42. 소켓확장이 안 되던 진짜 이유, 그리고 확률을 파일로 올린다 (2026-08-22)

§41 은 **오진이었다.** "이 게임에 소켓확장 기능은 없다"고 적었는데, 있다 —
`item_18.dat` **12행 소켓확장**, `장비 아이템의 소켓을 1개 늘릴 수 있습니다;
(아이템 소켓은 5개 제한입니다)`. `tips.dat` 의 `소켓 장비는 조합이나 몬스터를
통해 얻을 수 있습니다.` 는 소켓이 **처음 생기는** 경로를 말한 것이지 늘리는
경로를 부정한 것이 아니었다. 사용자가 정확히 그것을 지적했다.

### 42.1 소켓확장 목록의 술어는 단 하나 — `0xFF` 칸이 있는가

경로 전체:

```
StateInGameMenu::Process 0x10AD3C   kind 18 이고 행 ∈ {11,12,13}
  → OpenBagFilter 0xED838           0xED848 cmp r2,#0xc  → 0xED8E0
  → BagItem::SetItemFilter(0x1F)    0xED8F8 mov r1,#0x1f
  → Process case 12  0x10BFE4       ldrb r2,[r7,#0x168] / add r2,#1
                                    strb #0,[r3,#0x169] / strb r2,[r7,#0x168]
```

`OpenBagFilter` 는 **아이템의 index**(`ItemInfo+0x15`)로 갈린다 — 11 → 필터 30
(역행의 기원), **12 → 필터 31 (소켓확장)**, 13 → 필터 32 (복원의 서).

필터 31 의 몸통(`SetItemFilter` 의 33항 점프테이블, 슬롯 31 = 0xA1718 →
**0xA17F8**)은 셋만 본다:

```
0xA17F8  cmp  r2, #9              ; kind <= 9 (장비)
0xA1800  ldrb r3, [r3, #0x155]    ; 착용 계층 != 7
0xA180C  ldrb r3, [r0, #0x169] / cmp #0xff / beq 0xA173C   ← 채택
0xA1818~ +0x16a .. +0x16d 를 같은 식으로 넷 더
0xA1848  (다섯 칸 전부 0xFF 아님 → 목록에서 뺀다)
```

**등급 검사도 개수 검사도 없다.** 즉 `MakeSocket`(0xA1224)이 거절하는 전설
(등급 4)도 **소켓확장은 된다** — 확장 경로는 `MakeSocket` 을 아예 안 지난다.
조건은 하나뿐이다: **다섯 칸 중 하나가 `0xFF` 여야 한다.**

`0xFF` 와 `0x00` 은 바꿔 쓸 수 없다. 규약을 정의하는 것은 `MakeSocket` 자신이다
— 0xA12BC `memset(item+0x169, -1, 5)` 로 다섯 칸을 **전부 0xFF** 로 칠한 뒤
0xA12EC 에서 앞의 `count` 칸만 `0x00`(소켓 있음, 비었음)으로 바꾼다. 그래서
**소켓 0개인 정상 아이템은 개수 0 + 다섯 칸 전부 0xFF** 이고, 그것이 확장
목록에 뜨는 이유다.

### 42.2 호스트가 게스트가 만들지 않는 상태를 보내고 있었다

`net_item_granted` 가 꼬리 아홉 바이트에 0 을 아홉 개 보냈다. 그 아홉은
`_NET_ITEM_+0x4D`, 그리고 `NewBagNetItem` 이 전부 복사한다 — `+0x168`(개수)은
0xA589C, 다섯 칸은 0xA58A4~0xA58C4. 결과가 **개수 0 + 다섯 칸 전부 0x00**,
게스트가 스스로는 절대 만들지 않는 상태다. `0xFF` 가 하나도 없으니 0xA1848 로
떨어져 목록에서 조용히 빠진다.

> **§41.3 의 진단(개수 0 → `IsSocketFull` 이 꽉 참으로 본다)은 맞았지만
> 그것은 오브결합이 막히는 이유였고, 사용자가 본 것은 확장 목록이었다.**
> §41.7 이 넣은 "최소 1개 굴림"은 증상만 가렸다. 진짜 고침은 **다섯 칸을
> `MakeSocket` 이 남기는 모양대로 보내는 것**이다.

**고친 것** (`localserver.rs`): 꼬리를 칠하는 조건이 `n > 0` 이 아니라
**`equip.is_some()`** 이다. 장비 레코드면 언제나 앞 `n` 칸 `0x00` / 나머지
`0xFF`, 장비가 아니면 아홉 바이트 0 그대로 — 그래야 `net_item` 이
`hl5_server/protocol.py` 와 **바이트 동일**로 남고 `localserver_ab.rs` 다섯 개가
계속 그것을 못박는다.

**그리고 소켓 굴림을 게스트 표로 되돌렸다** (`shop.rs`). §41.7 은 조합 표
(블록 G, 25/25/20/15/10/5%)에 하한 1 을 얹었는데, 유물함 장비는 조합물이 아니라
**생성물**이다. 지금은 `MakeSocket` 자신의 표(블록 A, 등급별)를 쓴다 —
등급 0·1·2 는 50/20/15/10/4/1%, 등급 3·세트는 70/20/10%, **등급 4 전설은 0개**
(게스트가 그렇게 정한다). 하한은 없앴다: 0개여도 확장이 되므로 발명할 이유가
사라졌다.

> **공허 무기는 이 버그와 무관했다.** `ProcPacket_Shop` 이 `kind 18 / index
> 44..48` 을 0xCD384 에서 가로채 `NewItemToBag`(0xA72D0) → `NewItem`(0xA4A5C)
> → `GetItemTableInfo` 로 게스트가 직접 만든다(`NewBagNetItem` 을 건너뛴다,
> 0xCD3EC). 그 경로는 다섯 칸을 0xFF 로 남기므로 처음부터 확장이 됐다.

### 42.3 등급 4 = 전설은 이제 **증명**됐다

§41.2 는 이름 대조(그람·쇼픈·오드·알케인)로 추정했다. 바이너리가 직접 말한다:

* `CommonUi::DrawItemDiscription`(0x7B684)의 `+0x15c` 점프테이블, **case 4**
  (0x7B6BC)가 `GetCommonText(218)` = `^전설|` 을 부른다.
* `ItemInfo::DrawName`(0xA8E34)의 같은 모양 테이블, case 4(0xA8ED8)가
  색 `0x00DF0025` — 도움말이 약속한 붉은색이다.

사다리 전체: 0 일반 / 1 레어 / 2 에픽 / 3 영웅 / **4 전설** / 5..18 세트(값−5 가
`set_option.dat` 행). **출하 데이터에 등급 1 행은 0개다** — 레어는 드랍 밴드가
씌우는 등급으로만 존재한다.

### 42.4 확률 레버는 후크가 아니라 **파일 그림자**다

`StaticUtil::LoadRes`(0x144E80)가 `/c/...` 를 호스트 FS 에서 **먼저** 찾는다.
관문이 **둘**이다:

```
0x144ED0 bl MC_fsFileAttribute    ; → getFullFilePath → stat
0x144EE8 bne 0x144F40             ; st_size != 0 → 호스트 FS 팔
0x144F50 bl strcmp                ; gv+0x48 vs gv+0x5a — 버전 두 개
0x144F58 bne 0x144EEC             ; 다르면 VFS 로 되돌아간다   ← 둘째 관문
0x144F5C mov r0,r6 / MC_knlCalloc / 0x144F74 StaticUtil::FileRead
```

**경로가 함정이다.** `MC_fsFileAttribute`(0x14C0B8)는 먼저
`MIDASKernelManager::getFullFilePath`(0x151E40)를 부르고, 그것이 mode 1
(`LoadRes` 가 0x144ECC 에서 넘기는 값)에서 `root + '/' + path` 를 만든다.
root 는 `nativeInitFileStorage` 가 받은
`/data/data/co.kr.eamobile.HeroesLore5/files` 다. 그래서 그림자 파일은
`<storage>/c/csv/...` 가 **아니라**
`<storage>/data/data/co.kr.eamobile.HeroesLore5/files/c/csv/...` 에 놓아야 한다.
그 리터럴을 `hl5_core/src/lib.rs` 의 `INTERNAL_STORAGE_ROOT` 로 뽑아 부팅
경로와 그림자 경로가 갈라질 수 없게 했다.

**엉뚱한 곳에 쓰면 증상이 없다** — 조용히 원래 확률로 돈다. 그래서 판정은
로그가 아니라 카운터다:

```
cargo run -q -p hl5_probe -- --boot --boost-rates --frames 60 \
    --storage out/rt --count 0x144f5c --count 0x144eec
```

| 파일 위치 | 0x144F5C (호스트 읽기) | 0x144EEC (VFS) |
|---|---|---|
| 없음 | **0** | 135 |
| `<storage>/c/csv/…` | **0** | 135 |
| `<storage>/data/data/…/files/c/csv/…` | **1** | 134 |

0x144F5C 는 버전 strcmp **뒤**라, 1 이 나온다는 것은 그림자가 두 관문을 다
통과해 실제로 읽혔다는 뜻이다.

> **미해결**: 실서버를 물리면 `NETWORK::ProcPacket_Patch`(0xCC9AC / 0xCCD10)가
> `gv+0x5a` 를 와이어에서 덮어써 둘째 관문이 막힐 수 있다. 오프라인에서는
> `CommonUi::InitLocalSaveData`(0x77F1C / 0x77F78)가 두 필드에 같은 리터럴
> `HERO5.ALL.06.000` 을 써서 항상 통과한다. `-Server` 빌드는 미시험이다.

### 42.5 오브결합 50/45/40/35/30% — 그리고 파괴대가 같이 줄어든다

`probability.dat` 레코드 6..10 의 **원소 0** 이 성공 임계값이고 인덱스는
**이미 박힌 같은 오브의 개수**다(0xA1F34 의 계수 루프). 400/200/50/10/5 →
**500/450/400/350/300**.

원소 1 은 "두 번째 성공"이 아니라 **벌칙대의 끝**이다. `[T0, T1)` 은 0xA21DC 로
가고 거기서 **세 번째 `Rand(0,999)`**(0xA21E8)가 500 에서 반으로 가른다
(0xA21EC `cmp r0,#0x1f4`) — 아래면 **아이템 파괴**, 위면 **박혀 있던 오브 전멸**.
`[T1, 1000)` 만이 오브 하나만 잃는 무해한 실패다.

그래서 성공을 올리면 파괴·전멸이 같이 줄어든다. 같은 오브 4개째에서 벌칙대가
**80% → 50.5%** 가 된다. `T0 > T1` 이 되는 행(레코드 7)은 `T1` 을 `max(T0,T1)`
으로 올린다 — 순차 걷기라 어차피 빈 밴드지만, 표가 코드와 같은 말을 하게.

**달성의부적(×2, 0xA1F9C)은 그대로 산다.** 첫 행은 500×2 = 1000 이라 100%
성공이 되고, `ldrsh` 로 읽히므로 부호 있는 halfword 안에 넉넉히 든다.

### 42.6 전설 드랍 5% — 밴드 5 만 넓히고 이웃은 안 건드린다

> ★ **이 절의 "고침"은 2026-09-07 에 철회됐다 (사용자 지시). 지금 밴드 5 는
> 출하 폭 그대로이고 두 갈래가 같은 값을 쓴다.** 현장 제보("바닐라가 main 보다
> 덜 나온다")의 원인이 갈래가 아니라 **한쪽 설치에만 이 그림자가 있었던 것**
> 이었다 — 출하 폭은 6~852/65536(대개 9 ≈ 0.014%/굴림, 헬 보스 852 ≈ 1.3%)인데
> 그림자는 3277 이라 흔한 행에서 **360배**다. `rates::patch_enemy` 는 지워졌고,
> `rates::restore_shipped_drop_bands` 가 `boost_rates` 와 무관하게 **매 부팅**
> `enemy_<N>.dat` 그림자를 지운다 — 안 쓰는 것만으로는 `LoadRes` 가 호스트 FS 를
> 계속 먼저 보므로 이미 깔린 기기가 되돌아오지 않는다. **아래 밴드 표와 오프셋
> 39 는 그대로 참이다**; 철회된 것은 `T5` 를 미는 문단이고, 기록으로 남긴다.
> 정본은 [`BRANCHES.md`](BRANCHES.md).

`Monster::SetDropItem`(0xBC910)은 `Rand(0,65535)`(0xBCB14) 하나를 몬스터의
7단 사다리(`Monster+0x254..+0x26C`)에 건다. 밴드 여덟, 그리고 **셋은 등급을
강제하고 셋은 표의 등급을 살린다**:

| 밴드 | 조건 | 나오는 것 |
|---|---|---|
| 0 | `r < T0` | kind 13 제작재료 |
| 1 | `[T0,T1)` | 등급 **0 강제** (0xBCCD4) |
| 2 | `[T1,T2)` | 등급 **1 강제** = 레어 (0xBCD58) |
| 3 | `[T2,T3)` | 등급 **2 강제** = 에픽 (0xBCE1C) |
| 4 | `[T3,T4)` | 표 등급 (대개 영웅) |
| **5** | **`[T4,T5)`** | **표 등급 — 전설이 나오는 유일한 밴드** |
| 6 | `[T5,T6)` | 표 등급 (대개 세트) |
| 7 | `r ≥ T6` | 아무것도 없음 |

값은 `/c/csv/enemy_<N>.dat`(N = 난이도 0·1·2, 각 166 몬스터 23,190 B)의
**이름 뒤 39바이트** u32 일곱이다. 그 오프셋은 디스어셈블(`r6+0x26`,
`r6 = 레코드시작+4+namelen`)과 전수 탐색(166행 전부에서 단조·≤65536 인 오프셋이
39 하나뿐) 양쪽에서 나온다.

**고침**: `T5` 를 올려 밴드 5 를 **3277/65536 = 5.000%** 로 만들고 `T6` 을 같은
delta 만큼 함께 민다. 그러면 밴드 4(영웅)는 양끝이 안 움직여 폭 그대로, 밴드
6(세트)도 양끝이 같이 움직여 폭 그대로 — **줄어드는 것은 `T6` 위의 "아무것도
안 나옴"뿐이다.** 전설은 빈 굴림에서 나오지 다른 등급에서 빼앗지 않는다.

**이미 전설을 떨구는 몬스터만** 손댄다(파일당 **54행**, 166 중). `T4 == T5` 인
행은 그대로 둔다 — 잡몹에 5% 전설을 주는 것은 확률 상향이 아니라 드랍표 발명
이다. 실측: 세 파일 다 크기 23,190 그대로, 54행 변경, 전부 밴드5 = 정확히
3277, 사다리 단조·≤65536 유지, 밴드 6 폭 바이트 단위로 보존.

> **한 마리당 확률은 이것과 다르다.** `SetDropItem` 은 몬스터의 `enemy[0x275]`
> 번(1~4회) 독립 굴림을 돈다. 밴드 5 가 5%/굴림이면 4굴림 보스는
> `1-0.95⁴ ≈ 18.5%/마리` 다. 그리고 그 그룹의 droptable 네 칸 중 몇 개가 실제로
> 등급 4 를 가리키는지가 다시 곱해진다.

### 42.7 인수 기준

* `cargo test -p hl5_core` **79 + 5개** (`rates.rs` 11개 신규: 오브 6, 드랍 5).
* `bash tools/ab_905.sh rates2` 1,241 프레임 (`boost_rates` 기본 off).
* 그림자가 실제로 읽히는 것은 `--count 0x144f5c` 로 0 → 1.
* `--selftest` 29개, `localserver_ab.rs` 5개(파이썬과 바이트 동일 유지).

### 42.8 아직 안 한 것

> **⚠ 이 절의 두 문장은 틀렸다. §43.1 과 §43.2 가 고쳤다 (2026-08-22).**
> (1) 성장의 서의 +50% 는 죽어 있지 않다 — 0xC08F4 는 건너뛰기가 아니라 64비트
> 비교의 차가운 절반이고 보너스로 되돌아온다. (2) 바닷물 프레임 드랍의 원인은
> `Map::MapEffect` 가 아니다 — S9 실측으로 게스트 인터프리터가 코어의 92~94% 를
> 쓰고 present 는 6% 다. 아래는 기록으로 남긴다.

* **슈퍼 성장의 서**(경험치 300%) — 착수 못 함. `item_18` 25행 성장의 서가
  본이고 기제는 `HERO::{Get,Set}ExpGoodTime`(0x89D54 / 0x8D7CC) +
  `ProcAutoExpGoodTime`(0x8D5A4)이다. 새 행을 어디에 둘지(빈 행 4·5·6·7·41·42가
  후보), 150% 상수가 어디 있는지, 카탈로그가 파일인지 와이어인지가 숙제.
* **바닷물 프레임 드랍** — 착수 못 함. 후보는 `DrawImageContortClip565`(물결
  왜곡), `BoxBlur565/666`, `Map::RestoreMapEffect`(0xAC994). 방법론은 §20/§32.6
  그대로: `--profile` 로 이름을 얻고 → 호스트 이식 → 1,241 프레임 바이트 동일.
* **기기 시험 전부.** 소켓확장 목록에 유물 장비가 뜨는 것을 사람 눈으로 본 적이
  없다. 그리고 **옛 세이브의 유물 장비는 안 고쳐진다** — 다섯 칸이 0x00 인 채로
  남아 계속 목록에서 빠진다. 다시 뽑아야 한다.

## 43. §42.8 이 두 군데 틀렸다, 그리고 S9 이 성능 지도를 다시 그렸다 (2026-08-22)

이 절은 정정 둘과 구현 셋이다. 정정이 먼저인 이유는 §42.8 의 두 문장이 다음
작업의 전제였고, **둘 다 사실이 아니어서** 그 위에 세운 계획이 통째로 틀렸기
때문이다.

### 43.1 원본 성장의 서의 +50% 는 살아 있다 — 사용자의 플레이 시험이 맞았다

§42.8: "경험치 보너스는 0xC0724 `add r3, r3, sl, asr #1` 인데 관문이 0xC0708
`cmp r1,#0 / ble` — 상위 워드만 본다. 3시간은 상위 워드가 0 이라 항상
건너뛴다."

**틀렸다.** 0xC08F4 는 건너뛰기 지점이 아니라 GCC 가 떼어낸 64비트 비교의
차가운 절반이고, 조건이 맞으면 **보너스로 되돌아온다**:

```
0xC0704  bl  HERO::GetExpGoodTime   ; r0 = lo, r1 = hi
0xC0708  cmp r1, #0
0xC070C  ble #0xC08F4               ; 여기서 끝나 보인다
...
0xC08F4  bne #0xC0900               ; hi < 0 -> 정말로 만료
0xC08F8  cmp r0, #0
0xC08FC  bne #0xC0710               ; hi == 0 && lo != 0 -> 보너스 적용
0xC0900  ldr r0, [r2, r3]           ; (만료 경로: HERO 를 다시 잡고)
0xC0910  b   #0xC072C               ; 보너스를 건너 정령 경로로
```

건너뛰는 것은 `hi == 0 && lo == 0` 과 `hi < 0` 뿐이다. `hi > 0` 이면 0xC070C 를
그대로 통과한다. 정령 경로(0xC0764 / 0xC0914)도 같은 모양이고 같이 살아 있다.

> **이 함정은 CLAUDE.md 에 이미 적혀 있었다** — GCC 는 64비트 비교를 차가운
> 블록으로 떼어내고 되돌아온다. §42.8 이 자기 문서의 경고에 그대로 빠졌다.
> 판정법은 하나다: **`ble <먼 곳>` 을 보면 `<먼 곳>` 을 반드시 읽어라.**

따라서 "원본 성장의 서도 150% 로 고친다"는 결정은 **구현할 것이 없다.**

정령 보너스는 정확히 같지는 않다. `0xC072C` 앞에 `GetSpiritInfoPtr` 의
상태 바이트 검사(`ldrsb r3,[r0] / cmp r3,#0 / ble`)가 하나 더 있어서, 정령이
없으면 그 경로 자체에 도달하지 않는다. "구조가 동일하다"까지가 과장이고
"살아 있다"는 맞다.

### 43.2 프레임 드랍은 맵 효과가 아니다 — S9 실측

§42.8 은 원인을 `Map::MapEffect` 의 프레임버퍼 전체 변환으로 지목했다. 사용자가
S9(SM-G960N, 1080×2220 → 캔버스 555×270, 빅 클러스터 2.704 GHz)을 연결해 줘서
실제로 재 봤다. 벤치 APK 를 나란히 깔고(`kr.heroeslore.five.bench`), 사용자의
Lv5 워리어 세이브를 불러 제자리에서:

| S9 게임플레이 (제자리, 몬스터 셋) | 값 |
|---|---|
| `repaints/s` | **20.5~22.0** (설계 천장 27.8) |
| 게스트 CPU | **9,157~9,432 ms / 10 s = 92~94%** |
| present | 582~707 ms = **6%** |
| `late` | 168~225 ms |
| 게스트 명령어 | 31.1~34.4 M/s |
| 틱당 게스트 비용 | **45.6 ms** (예산 36 ms) |

**present 가 6% 이므로 렌더 파이프라인이 아니라 인터프리터가 천장이다.**
그리고 이것은 *맑은* 상태의 수치다. 지속 부하에서:

| 상태 | 빅 클러스터 | 게임플레이 |
|---|---|---|
| 갓 띄움 | 2.50~2.65 GHz | 25.1 fps |
| 1분쯤 | `scaling_max_freq` 가 **1.794 GHz** 로 캡 | 20.7 fps |
| 3분쯤 | 1.261 GHz | **12.3 fps** |

배터리 35.9 °C, `low_power` = 0. 강제 종료하고 2분 식히면 2.65 GHz 로 돌아온다.
**즉 사용자가 "해안가에서 떨어진다"고 한 것의 절반은 열 강하다** — 고부하 장면이
열을 올리고, 클럭이 내려가고, 그래서 더 느려진다.

`android_main` 은 이미 `Cpus_allowed_list 4-7` 이다(프라임 핀이 S9 에서도
동작한다). 스케줄 쪽에 남은 것은 없다.

**주의: 순간 클럭 읽기는 노이즈다.** 게스트 스레드는 틱 사이에 잠들기 때문에
`scaling_cur_freq` 한 번 읽기는 1.69~2.50 GHz 사이를 튄다. 신뢰할 수 있는 것은
10초 집계의 `guest ins/s` 다 — 그것이 34 M 에서 20.5 M 로 내려간 것이 실제
지속 강하였다.

**캔버스는 지렛대가 아니다**: 같은 ingame 재생을 400×240 과 555×270 으로 돌리면
넓은 쪽이 게스트 명령어를 **덜** 쓴다(1.515 G vs 1.609 G). 큰 블리터가 이미
호스트에 있어 픽셀이 늘어도 호스트 시간만 늘기 때문이다(두 재생은 스크립트 탭이
다른 자리에 떨어져 장면 내용도 갈라지므로, 이 비교에서 그 이상은 읽어내지 마라).

### 43.3 §42.8 의 후보 목록도 두 군데 과장이었다

* **`BoxBlur` 는 프레임당 비용이 아니다.** `Graphic::BoxBlur`(0x557A8)의 호출
  16곳이 전부 `StateInGameMenu`/`StateInExpertMenu` 의 생성자·소멸자다. 메뉴를
  열고 닫을 때 한 번씩이고, 프로파일에서 1.5% 다. (크로노스에서 메뉴 열 때
  1.2초를 먹은 그 함수가 맞지만, 여기서는 프레임 예산의 문제가 아니다.)
* **`ChangeBlendingPal`(0x58D38)·`ChangeHSB`(0x5F000)는 팔레트를 돈다.** 루프
  상한이 `GbmImage::GetPaletteSize` 이지 `GetWidth*GetHeight` 가 아니다.
  `Map::MapEffect` 안에서 진짜 전체 캔버스를 훑는 것은 **`GrayFrameBuff`(0x54FA8)
  둘뿐**이다.

`GrayFrameBuff` 는 해독해 두었다(360바이트): `level = (255 * -(i8)arg2)/100`,
0 이면 즉시 반환, `Graphic::GetRbgType() != 1` 이면 반환, 그 뒤 `w*h` 픽셀을
`gray5 = (3R5 + 4G5 + B5) >> 3` 로 바꾸고 `level != 255` 면 5비트/6비트 가중
혼합을 한다. `FrameBufferInfo` 는 프레임버퍼 디스크립터에서 스트라이드·bpp·
너비·높이·베이스를 그대로 복사하므로 호스트 이식이 게스트 호출 없이 된다.

### 43.4 실제 프로파일 — 상위 셋이 73%

`--profile`, `tools_re/scripts/ingame.txt`, 출하 캔버스(`--screen 1080x2220`),
2,718 프레임:

```
33.7%  _midas_funcFntDrawString            미이식
21.8%  Graphic::DrawImageMirrorClip565     미이식
17.5%  Graphic::DrawImageBlendingClip565   부분 이식 — 13,836회 사양
 3.1%  Graphic::ChangeBlendingPal
 2.8%  Graphic::DrawImageRotateEx5Clip565
 1.8%  desRound
 1.5%  Graphic::BoxBlur565
```

**S9 이 천장에 닿는 조건은 틱당 명령어 −21%** (45.6 → 36 ms). 뒤의 둘만
이식해도 39% 다. S25 는 이미 천장의 98% 라 이 이식으로 프레임을 벌지 않는다 —
값을 하는 곳은 S9 같은 저사양 기기와 `Config::speed > 1.0` 이다.

`--profile-in` 이 준 뜨거운 자리:

| 함수 | 뜨거운 오프셋 | 그 함수의 비중 |
|---|---|---|
| `FntDrawString` | `+0x1140`~`+0x1280` | ~74% |
| | `+0x04C0`~`+0x0540` | ~8.7% |
| `DrawImageMirrorClip565` | `+0x0400`~`+0x04C0` | 63% |
| | `+0x0640`~`+0x06C0` | ~18.6% |
| `DrawImageBlendingClip565` | `+0x14C0`~`+0x16C0` | ~52% |
| | `+0x1100`~`+0x11C0` | ~18% |

`FntDrawString` 이 왜 그렇게 큰가: 뜨거운 루프(0x3B8A4~0x3B980)는 글리프
**비트마다** 한 바퀴를 돌고, 루프 변수가 전부 PIC 전역(`_midas_r0` 비트 인덱스,
`_midas_r1` 바이트 오프셋, `_midas_drawx/drawy/chX/chWidth/temp`)이라 반복마다
GOT 왕복이 열 몇 번이다. 그리고 켜진 픽셀마다 `bl _midas_funcFntDrawPixel` —
**그 함수는 이미 호스트 패치**라서 글리프 픽셀마다 패치 디스패치를 한 번씩
낸다. 실측(`--no-patch --count`): 전체 패치 호출 3,626,353 중
**3,467,716(95.6%)이 FntDrawPixel**, 프레임당 1,276 픽셀.

루프 구조: 2바이트/행, 바이트0 의 비트 b → `drawx+8`, 바이트1 의 비트 b →
`drawx`, 비트마다 `drawx += 1`, 행 끝에 `drawx = chX` / `drawy += 1` /
`_midas_r1 += 2`, 종료는 `(*_mxfnt_prt)[0x30] > _midas_r1 + 2`.
`patch.rs` 의 `FntGlobals` 가 나머지 열셋(clip·opacity·rgb·lcd)을 이미
해결해 두므로, 남은 것은 이 일곱 개의 루프 상태 전역뿐이다.

### 43.5 함수 중간에서 재개하는 후크 — 새 기법

`Vm::run` 은 후크가 `true` 를 돌려주면 `PC := LR` 을 한다. 그래서 **후크가 `LR`
을 직접 써서 재개 지점을 고를 수 있다.** 조건 셋:

* 그 함수가 진짜 복귀주소를 프롤로그에서 스택에 밀어 두었을 것,
* 재개 지점 이후가 `LR` 을 읽기 전에 쓸 것,
* **`vm.call` *뒤에* `LR` 을 쓸 것** — `Vm::call2` 가 CPU 전체를 복원한다.

`cashitems.rs` 가 이것으로 `StateInGameMenu::Process` 한가운데로 들어간다.
`FntDrawString` 의 글리프 루프도 같은 방법으로 이식할 수 있지만, **그것은
"같은 바이트" 이식이므로 `patch.rs` 에 넣어야 하고, 그러려면 `PATCHES` 를
심볼+오프셋 표로 넓혀야 한다** — 동작 변경용 `Vm::hooks` 에 이식을 섞지 마라.
그 구분이 무너지면 두 맵의 A/B 가 각각 무엇을 증명하는지 아무도 말할 수 없다.

### 43.6 슈퍼 성장의 서 — 그리고 확장 가능한 캐시템 계층

게스트의 캐시템 사용 분기는 15칸 점프테이블이고 `item_18` 행 11..25 만 덮는다.
정렬은 짐작이 아니라 자기검증으로 확정했다: 행 25 는 `SetExpGoodTime` 몸통에
떨어져야 하고, `0x10BA4C + (25-11)*4 = 0x10BA84` 의 워드가 `b #0x10BC78` 이며
그 안 0x10BC98 이 `bl HERO::SetExpGoodTime` 이다.

진입은 한 곳뿐이다(0x10AF54 `bhi #0x10BA34`). 그 자리에서 `ip` = `ItemInfo*`,
`r3` = 행 인덱스(아직 원본, 다음 명령이 `lsl #0x18`)이고, 인덱스 30·31 과
39..42 는 이미 걸러져 있다. **행 25 를 넘는 인덱스는 기본 분기로 가서 "소모만
하고 아무 일 없음"이 된다** — 후크가 그것을 가져간다.

재개 지점은 `Process+0x18B8`(0x10BEE8): `GetCommonText(298)` →
`MC_knlSprintk` → `GetBagItemPtr` → `DeleteBagItemCategoryIndex(bag, 18,
(i8)[ip+0x15], 1)`. **그래서 호스트는 가방을 건드리지 않는다** — 게스트가 스스로
소모하고 "…을(를) 사용했습니다." 를 띄운다. `ip` 가 그 시점에도 `ItemInfo*` 인
근거는 유일한 선행자가 0x10AF34 에서 그것을 실었고 사이에 쓰는 곳이 없다는
것이다.

배수는 `SetDieMotion+0x68`(0xC06EC, `bl Formula::calc` 바로 다음)에서 `r0` 을
곱하고 **`false` 를 돌려준다**. 여기서 `true` 를 돌려주면 `PC := LR` 이 되어
`SetDieMotion` 이 +0x68 에서 반환되고 `QuestCheck`·사냥 카운터·`setMonState(4)`
·`SetMotion(5)`·`SetDecreaseDieFrame` 이 전부 건너뛰어져 **몬스터가 죽지
않는다.** 그 자리가 안전한 이유는 `r7`(=0x1474)·`r8`(=gv)이 호출을 넘어 살아
있고 게스트 자신이 바로 다음 명령에서 그것을 역참조하기 때문이다.

**배수는 3 이 아니라 2 다.** 게스트가 그 위에 `floor(base/2)` 를 더하므로:

| 상태 | 호스트 배수 | 합계 |
|---|---|---|
| 버프 없음 | 1 | 1.0배 |
| 성장의 서 | 1 | **1.5배** (출하 동작 그대로) |
| 슈퍼 성장의 서 | 2 | **3.0배** |

판별자는 지속시간이다. `SetExpGoodTime` 은 인자를 `+0x330`(총)과 `+0x328`(남음)
**양쪽에** 쓰므로, 슈퍼는 `10_800_001` 을 넘겨 원본의 `10_800_000` 과 갈린다.
게스트가 그 필드를 세이브에 넣으므로 호스트는 상태를 따로 두지 않는다. 1 ms 는
사람에게 안 보인다(활성 메시지는 시·분·초, 인게임 타이머는 분 단위 올림).
경험치는 `HERO+0x230` 의 부호 있는 32비트이고 게스트가 그 위에 절반을 더하므로
곱은 `0x3FFF_FFFF` 로 물린다 — 넘치면 경험치가 음수가 되어 레벨업이 깨진다.

**데이터 레이아웃은 로더에서 읽고 다시 파싱해 검증했다** — 두 파일 모두 걸음이
정확히 EOF 에 떨어진다(`item_18.dat` 4022/4022, 카탈로그 2530/2530):

```
item_18.dat   [u16 count] + [u16 reclen][u16 param][u8 nameLen][name]
                            [u32 money][u8 descLen][desc][u8 useClass][u8 last]
카탈로그      [u16 count] + [u16 reclen][u8 id][u16 param][u8 nameLen][name]
                            [u8 a][u8 qty][u8 descLen][desc][u16 price ×3]
```

`useClass` 3 이 가방에 {사용하기, 버리기} 를 준다(2 는 버리기만, 그 밖에는 팝업
자체가 없다 — §32.9 의 오브 토큰이 그것이었다). 새 항목은 `item_18` 행 49,
카탈로그 id 52. **id 는 56 을 넘기면 안 된다** — `GetDiscount` 가
`gv + id*4 + 0x1D04` 를 범위검사 없이 색인하고 `gv` 는 7,656 바이트다. 행은
49 이상이어야 한다 — 그 아래는 어딘가에 하드코딩된 인덱스가 있다.

**부팅 순서에 버그가 있었고 같이 고쳤다.** `shop::ShopHost::new` 가
`GrantTable` 을 만들 때 `item_18.dat` 를 **아카이브에서** 파싱하고 `a` 가 행 수를
넘는 카탈로그 행을 거절한다. 그림자를 그 뒤에 쓰면 새 항목이 **상점에는 뜨고
지급은 거절되어** 돈만 먹는다. 지금은 확장 바이트를 지급표와 그림자 양쪽에
같은 것으로 준다.

**그리고 EUC-KR 함정에 한 번 걸렸다.** 첫 초안은 이름·설명을 Rust 문자열
리터럴로 썼는데 그것은 UTF-8 이고 게스트는 깨진 글자를 그린다. 지금은 바이트
이스케이프이고, **"UTF-8 로 파싱되면 실패"** 하는 단위시험이 붙어 있다.

검증: `--count 0x144f5c` 가 **2** — 두 그림자 파일을 게스트가 실제로 읽었다는
뜻이고, §42.4 가 말한 대로 이 판정은 로그가 아니라 이 카운터로만 된다.

### 43.7 세이브를 logcat 으로 빼낸다

릴리스 APK 는 debuggable 이 아니라 `run-as` 가 거절되고, `/sdcard/Android/data/`
는 최신 안드로이드·삼성에서 막힐 수 있는데 그 실패가 "파일이 없다"와 구분되지
않는다. 그래서 앱이 부팅 때 세이브를 base64 로 logcat 에 찍는다
(`savedump.rs`, `-DumpSave`, `<files>/hl5_dump_save.txt`), 호스트가
`tools/savedump_decode.py` 로 되돌린다. 15,396 바이트 `kickass` + `LOCAL.sav`
실물로 **바이트 동일 왕복 확인**. 헤더의 청크 수와 실제 도착 수를 대조하므로
링버퍼가 중간을 흘린 캡처는 조용히 짧은 파일이 되지 않는다.

되돌린 파일은 `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/` 에
놓아야 한다 — 그림자와 같은 함정이다.

### 43.7b 오프라인에서 실제로 증명한 것 — 그리고 `ingame.txt` 로는 못 한다

`ingame.txt` 는 `Monster::SetDieMotion` 에 **150번** 들어가고 경험치 지급부에는
**0번** 닿는다. 그 함수가 이렇게 열리기 때문이다:

```
0xC0688  ldr r3, [r0, #0xf0]
0xC0694  cmp r3, #0
0xC06A0  ble #0xC06AC        ; <= 0 이면 지급부로
0xC06A4  add sp,#0xc / pop   ; > 0 이면 즉시 반환
```

150번이 전부 조기 반환을 탄다. 그래서 그 장면의 `--count 0xC06EC` = 0 은
호스트와 무관한 값이고, **이것을 "후크가 안 걸린다"로 읽으면 안 된다.**

새 스크립트 `tools_re/scripts/expkill.txt` 가 실제로 싸워서 한 마리를 죽인다
(`--count 0xC06EC` = **1**). **`ab_905.sh` 의 장면 목록에 넣지 마라** —
`keep-walking.txt` 와 같은 계열로, 픽셀이 아니라 카운터를 움직이는 스크립트다.
버프는 프로브의 새 스크립트 명령 `exp-buff <ms>` 로 켠다 — 게스트 자신의
`HERO::SetExpGoodTime` 을 부르므로 아이템 경로와 구분되지 않는다.

| 측정 | 결과 |
|---|---|
| 지급부 도달 (`0xC06EC`) | **1** (버프 무관) |
| 게스트 +50% 몸통 (`0xC0710`), 버프 **없음** | **0** |
| 게스트 +50% 몸통 (`0xC0710`), 버프 **있음** | **1** |
| 버프 없이 후크 on/off 프레임 비교 | **566/566 동일** (후크가 무해하다) |
| 버프 있고 후크 on/off 프레임 비교 | **569 중 118 프레임 다름** |
| 그 차이의 합집합 경계상자 | **(152,233)-(203,236)** — 경험치 바 그 자체 |

**세 번째 줄이 §43.1 을 디스어셈블이 아니라 측정으로 증명한다**: 버프가 켜지면
게스트의 +50% 가 실제로 실행된다. §42.8 이 "항상 건너뛴다"고 한 그 자리다.
그리고 마지막 두 줄이 호스트 ×2 가 실제로 결과를 바꾼다는 것과, 바꾸는 것이
경험치 바 하나뿐이라는 것을 같이 말한다 — §25.4 의 캐시샵 가격 후크와 같은
기준이다.

**아직 오프라인으로 못 한 것**: 아이템을 실제로 사서 가방에서 써 보는 왕복.
캐시샵에서 마지막 행까지 스크롤하고 가방을 여는 UI 조작이 길고 깨지기 쉬워서
`exp-buff` 로 우회했다. 그 절반은 기기에서 사람이 판정해야 한다.

### 43.8 인수 기준

* **1,241 프레임 5장면, HEAD(01ccc25) 대비 바이트 동일** — 출하 캔버스
  (`PROBE_FLAGS="--screen 2340x1080"`), 두 트리에서 `ab_905.sh <tag> new` 를
  돌리고 `ab_compare.py` 로 대조: newgame 119 / ingame 543 / shop 134 /
  online 109 / npcmenu 336, **전부 identical, 0 differing**. 새 기능은 전부
  `Config::default` 에서 off 이므로 코드 경로 자체가 없다.
* 단위시험 84 → **96**, `--selftest` 29 통과, 안드로이드 빌드 성공.
* `--count 0x144f5c` = 2 (그림자 두 개를 읽었다).
* 생성된 두 파일을 다시 파싱해 EOF 정확 일치, 행 49 / id 52 / `useClass` 3 확인.

### 43.9 아직 안 한 것

* **기기 시험 전부.** 슈퍼 성장의 서를 사서 쓰고 경험치가 3배로 들어오는 것을
  사람이 본 적이 없다. 판정법: 버프를 켜고 같은 몬스터를 잡아 `HERO+0x230`
  증가분을 비교하거나 레벨업 속도를 재라. 그리고 **가방에 {사용하기, 버리기}
  가 뜨는지**가 `useClass = 3` 이 도달했다는 증거다. 새로 사야 한다 — 지급 표는
  새로 나가는 와이어 레코드만 고친다.
* **블리터 이식 셋** (§43.4 의 순서). S9 의 −21% 가 목표다.
* **서명 설정이 막혀 있다** — `~/.gradle/gradle.properties` 에 `hl5.*` 가 없고,
  `<keystore>` 의 사본은 없는 경로(`<keystore>`)를
  가리킨다(키스토어 실물은 `<keystore>`, 별칭 `hero5`).
  지금 `-Release` 는 조용히 서명 없는 APK 를 내고 사용자의 세이브 위에 설치되지
  않는다. **배포 전에 이것부터 고쳐야 한다.**
* **조합 성공률과 이동속도 숫자는 사용자 결정 대기.** 출하값은 각각 75% 와
  7/8/8/6 px/틱이고, 바꾸는 방법은 CLAUDE.md 의 해당 절에 있다.

## 45. 유물함 투구가 머리를 지운 이유, 그리고 해상도 A/B/C (2026-08-25)

> **번호 주의**: ANALYSIS.md 에 §44 는 없다. 요구 ⑯⑰⑱(이동속도 고정·버리기·
> 랜덤옵션)의 서술은 CLAUDE.md 에만 들어갔고 여기로 옮겨지지 않았다. 이 절이
> §45 인 것은 그 사실을 덮지 않기 위해서다.

사용자 보고 둘로 시작한다.

1. **"유물함에서 나온 투구를 끼면 머리가 사라진다."** 스크린샷 첨부.
2. **"게임 내부 환경설정에 해상도 변경을 넣을 수 있나. A 전체화면 / B 가로 축소 /
   C 더 축소, 가운데 정렬."** 축소량은 뒤에 −10% / −20% 로 정해졌고, 줄은
   **환경설정 목록의 열 번째 행**이어야 한다 — "데이터관리 밑에, 투구보기·
   터치방식과 같은 디자인으로".

둘 다 됐다. 첫째는 우리 버그였고 원인은 예상보다 넓었다.

### 45.1 `ItemInfo+0x150` 은 스탯이 아니다 — 스프라이트 파일 번호다

`HERO::SetLoadImgPath`(0x8DE58)가 착용 장비에서 스프라이트 경로를 만든다.
슬롯마다 같은 모양이 여섯 번 반복된다:

```
  0x8dee0  mov r1,#2 / bl EquipItem::GetEquipItem       ; 갑옷 슬롯
  0x8df00  mov r3,#0x150 / ldrsh r2,[r0,r3]             ; <- +0x150
  0x8df18  bl MC_knlSprintk(_szSprFile+0x100, "/c/sp/img1/%03d.mgr", r2)
  0x8df30  mov r3,#0x150 / add r3,r3,#2 / ldrh r3,[r0,r3]
  0x8df40  strh r3,[_nSprPalFile, #2]                   ; <- +0x152 는 팔레트
```

`_szSprFile` 은 0xa00 바이트(0x100 × 10 슬롯), `_nSprPalFile` 은 halfword 열이고
함수 머리에서 각각 0 과 −1 로 memset 된다. 슬롯 배치:

| `_szSprFile` 슬롯 | EquipItem 슬롯 | 경로 | 부위 |
|---|---|---|---|
| 0 | 1 | `/c/sp/img0/%03d.mgr` | **투구(=머리)** |
| 1 | 2 | `/c/sp/img1/%03d.mgr` | 갑옷 |
| 2 | 3 | `/c/sp/img2/%03d.mgr` | 장갑 |
| 3 | 4 | `/c/sp/img3/%03d.mgr` | 장화 |
| 4 | 0 | `/c/sp/img4/%03d.mgr` | 무기 |
| 5 | — | `/c/sp/img5/00{0,1,2,3}.mgr` | 클래스 몸체 (점프테이블, `HERO[0x22c]`) |
| 6 | 5 | `/c/sp/img6/%03d.mgr` | 방패 (나이트 갈래에서만) |

**독자가 하나 더 있다**: `StateInGameMenu::KeySystemMenu+0x9d0`(0x1047EC)이
같은 포맷 문자열로 같은 짓을 한다(캐릭터 시트 미리보기). 즉 `+0x150` 을
"플레이어에게 보이는 수"로 읽는 코드는 `.text` 어디에도 없다.

**진짜 스탯은 둘뿐이다.** `Formula::getValFunc` 가 `ItemBase*` 에서 읽는 것은
`+0x158`(0x7602C)과 `+0x156`(0x76034, `mov r3,#0x154 / add r3,r3,#2`)뿐이다.
`+0x15a` 는 `RefineItem::ApplyItemDecompose+0x404` 가 `ldrsb` 로 되읽는다.

### 45.2 아카이브가 그것을 확정한다

장비표 아홉 개의 `+0x150` 값 범위가 **자기 스프라이트 디렉터리의 마지막 파일과
정확히 일치한다**. VFS 를 djb2 로 전수 조회한 결과:

| 표 | 행 | `+0x150` | 디렉터리 | 실재 파일 |
|---|---|---|---|---|
| `item_05` 투구 | 90 | 108..123 | `img0` | 0..127 (127개, 003 없음) |
| `item_06` 갑옷 | 93 | 2..56 | `img1` | 0..56 |
| `item_07` 장갑 | 81 | 2..48 | `img2` | 0..48 |
| `item_08` 장화 | 81 | 2..48 | `img3` | 0..48 |
| `item_00..03` 무기 | 86×4 | 1..64 | `img4` | 0..67 |
| `item_09` 방패 | 81 | 1..16 | `img6` | 0..16 |
| `item_10` 장신구 | 18 | **전부 0** | (레이어 없음) | — |

우연으로는 아홉 번 연속 맞을 수 없다.

### 45.3 그래서 `boost_stats` 가 머리를 지웠다

`hl5_core/src/shop.rs` 의 `boost_stats` 는 블록 오프셋 `[0, 2, 6, 8, 0x0a]`
= `ItemInfo+0x150/0x152/0x156/0x158/0x15a` 다섯을 **+10~30% 로 굴렸다**.
§39.2 와 그 함수의 주석이 다섯을 전부 "스탯 다섯 워드"로 부른 것이 틀렸다.

실제 표와 실제 아카이브로 옛 공식을 그대로 재현해 세어 보면:

```
장비 전 표 1,740회 굴림
  아카이브에 없는 파일을 요구 (레이어가 아예 안 그려진다)   285  16.4%
  파일은 있지만 남의 장비 그림                            1,455  83.6%
  원래 그림 그대로                                            0   0.0%

투구만 (item_05) 539회 중 435회 = 81% 가 없는 /c/sp/img0/NNN.mgr 을 요구
```

**유물함 장비는 100% 가 잘못된 모습이었고, 투구는 81% 가 머리 없음이었다.**
없는 파일이 왜 "아무것도 안 그림"이 되는가: `LoadRes` 가 NULL →
`GbmMerger::OpenGbmMerger` 가 0 → `SPRITE_IMAGE::LoadSprImg` 가 그 슬롯을
비운 채 두고 → `SPRITE::DrawSprite` 가 NULL 레이어를 건너뛴다. 클램프도
대체 그림도 없다.

> **원본 행동이 아니다.** 게스트에는 투구를 감추는 설계가 하나 있는데
> (`gv+0x8a` = 환경설정의 `투구` 줄), 그 길은 `/c/sp/img0/000.mgr` 로 **떨어지지**
> 대신 아무것도 안 그리지 않는다.

### 45.4 그리고 그 굴림은 재시작을 넘지 못했다

세이브를 읽어 보면 더 나온다. `HERO::SaveBagData`(0x8F4F4)가 아이템마다 쓰는
것은 `kind`(+0x14), `index`(+0x15), int64 셋(+0x138/+0x140/+0x148),
`+0x15c..+0x16D` 다. **`+0x150..+0x15a` 는 저장하지 않는다.**
`HERO::LoadBagData`(0x8EB4C)는 그 자리를
`ItemTable::GetItemTableInfo`(0xA2728) → `EquipItemInfo::CopyData`(0xA8718)로
**표에서 다시 채운다**.

따라서:

* **증상은 세션 한정이다.** 앱을 껐다 켜면 표 값이 돌아와 투구가 정상으로
  보인다. 이미 산 투구를 고치려고 세이브를 건드릴 필요가 없다.
* **그리고 스탯 굴림도 마찬가지로 사라졌다.** 사서 붙은 +10~30% 는 다음 실행에서
  표값으로 되돌아왔다. 장비가 조용히 약해지는 것은 애초에 안 굴린 것보다 나쁘다.

`GetItemTableInfo` 의 점프테이블은 kind 0..10 → `EquipItemInfo::CopyData`,
11 → `BattlelUseItemInfo`, 12 → `OrbItemInfo`, …, 18 → `CashItemInfo` 다
(**case 0 이 pc+8** — 0xA2738 의 `b` 는 폴스루 기본값이지 case 0 이 아니다).

### 45.5 고친 것 — `boost_stats` 를 지웠다

좁게 고칠 수도 있었다(스탯 둘만 굴린다). 그러지 않은 이유는 §45.4 다: 그 둘도
저장되지 않으므로 굴려 봐야 재시작에 증발한다. 그리고 그 함수의 원래 주석이
스스로 이유를 적어 두었다 — "옵션 슬롯을 아직 해독하지 못해서 스탯에 얹는다".
§41.9 가 `option.dat` 를 열었고 `fill_options` 가 슬롯을 굴리고 있으므로 그
전제는 없어졌다.

**유물함 아이템이 지금 갖는 무작위성은 전부 저장되는 것들이다** — 옵션 슬롯
(`+0x15f..+0x164`), 소켓(`+0x168..+0x16D`), 제한레벨 완화(`+0x15d` = 0),
그리고 뽑는 행 자체. 없어진 것은 "보이지도 남지도 않으면서 머리를 지우던" 부분
하나다.

`EQUIP_U16_AT` 은 다섯 그대로 남았다 — 그 목록의 용도는 **와이어 엔디안 스왑**
이고, `NETWORK::writeNetItem` 이 다섯을 전부 `ldrsh` 로 읽으므로 다섯이 맞다.
주석에 각 워드의 뜻을 적었다.

### 45.6 해상도 A/B/C — 지렛대는 캔버스다

체감 문제는 "큰 폰에서 화면이 너무 넓다"이고, 레버는 `widened_canvas` 다.
표시 단계에서 그림을 줄이는 방법은 **거절했다**: §33 이 확보한 정수 배율
(2340/585 = 1080/270 = 4)이 3.8 로 깨지고, 그 반 픽셀이 최근접 확대가 놓을 수
없는 유일한 것이다.

대신 **가로 캔버스만 좁힌다.** 높이는 그대로라 배율은 정확히 4 로 남고, 남는
좌우는 검게 두고 가운데 정렬한다.

```
widened_canvas_for(2340, 1080, A) = 585x270   x4.0   2340px
                              B  = 526x270   x4.0   2104px  (좌우 118px)
                              C  = 468x270   x4.0   1872px  (좌우 234px)
```

세 폭 모두 배율이 정확히 4 로 남는다 — 세로를 안 건드렸기 때문이고, 그것이 이
방식을 고른 유일한 이유다. 프로브에서 세 폭 다 프롤로그·타이틀·메뉴가 정상으로
그려진다(`out/shot/canvas-{a,b,c}.png`).

호스트 쪽 레터박스는 한 군데가 소유한다 — `App::dest_x` 가 `(x0, w)` 를 주고
EGL 은 `glViewport(x0, 0, w, sh)`, CPU 경로는 그 폭으로만 표를 만들고 나머지를
0 으로 채우며, `App::to_guest` 가 같은 비율을 역으로 쓴다. **A 모드는
`(0, sw)` 를 돌려주므로 옛 코드와 산술이 같다** — 등가가 아니라 동일하다.

대가는 하나: `nativeInitDisplay` 는 프로세스당 한 번이라 **바꾸면 다시 켜야
적용된다.** 줄이 그것을 말한다.

### 45.7 환경설정의 **열 번째 행** — 게스트 목록을 안 늘리고 늘린 방법

처음에는 패널 아래에 띠를 하나 그렸다. 동작은 했고 **모양이 틀렸다** — 목록에서
떨어져 있고, 자기 혼자 다른 디자인이고, 목록을 위로 올리면 오토타겟 밑에 떠
있었다. 사용자 지적이 정확했다.

목록의 실제 모양:

```
  index  0 배경음   1 효과음   2 게임속도  3 진동   4 화면효과
         5 오토타겟 6 투구보기 7 터치방식  8 데이터 관리
  보이는 칸 6개, 커서 행 this+0x0e, 커서 인덱스 this+0x0f
  창은 first = idx - row .. first + 5
```

**9 는 변수가 아니다.** `.rodata` 의 9바이트 타입표 둘(`DrawOption` 은 0x15A610,
`KeyOption` 은 0x15A6EC), `KeyOption` 의 랩 산술, `CommonUi::DrawScroll` 에 넘기는
리터럴 6/9, 그리고 `DrawOption` 이 스택에 만드는 8개짜리 라벨 id 배열에 박혀 있다.
**남는 줄도 없다** — 값은 `gv[0x84..0x8c]` 이고 전부 살아 있으며, 비어 보이던
타입 3 짜리 줄은 실은 터치방식(값은 `gv[0x8f]`, `StateGame::{UpdateNaviAngle,
DrawTouchUi, TouchPressed}` 가 읽는다)이다.

그래서 **인덱스 9 를 호스트가 갖는다.** 게스트의 표를 하나도 늘리지 않고 후크
다섯으로 끝난다:

| 어디 | 무엇 |
|---|---|
| `DrawOption+0x234` — 행 루프 머리 | 인덱스 9 면 게스트 자신의 프리미티브로 행을 그리고 루프 꼬리(`+0x3f4`)로 복귀 |
| `CommonUi::DrawScroll` (`DrawOption` 호출만) | 스크롤바에게 아홉이 아니라 열이라고 말한다 |
| `KeyOption` 진입 | 경계 둘(8→9 down, 0→9 up)과, 커서가 9 일 때의 모든 키 |
| `TouchOption+0x2ac` — 아래 화살표 클램프 | 창의 first 가 4 까지 가게 한다 |
| `TouchOption+0x30c` — 위 화살표 랩 | 8 이 아니라 9 로 감는다 |

**`TouchOption` 은 그 둘 말고는 손댈 것이 없다.** 행 히트테스트가
`idx = first + slot`(0x11BEB8)을 그냥 계산하고 표를 안 읽으며, 활성화는 전부
vptr+0x1c 로 `BaseState::KeyPress` 를 **합성**한다(LEFT −3, RIGHT −4,
SELECT −5, BACK −16). 그 키들은 다시 `KeyOption` 으로 들어오고, 거기 후크가 이미
인덱스 9 를 답한다.

행 하나가 무엇으로 이루어져 있는지는 `DrawOption` 에서 그대로 읽었다:

```
  y            halfH - 0x3a + 0x17*slot            (0x11DABC, 0x11DCA0)
  라벨 판       DrawHalfBox(halfW-0x4c, y-3, 0x8a, 0x11, …)   (0x11DB44)
                커서 행이면 마지막 셋이 1,1,0xff, 아니면 4,0,0xff (0x11DEA8)
  라벨 글자     DrawText(halfW-0x3f, y, …, 흰색 / 0x808080)    (0x11DB98)
  값 상자       FillRoundRect(halfW+0x0e, y-1, 0x1f, 0x0d, …)  (0x11DBF0)
  값 글자       DrawText(halfW+0x1e, y, …, align 1, 흰색)      (0x11DC80)
  화살표        DrawImageMirror(halfW+0x0c, y, CommonUi+0x1b84, frame 8)  (0x11DF28)
                DrawImage      (halfW+0x2f, y, 같은 이미지,   frame 9)   (0x11DF74)
```

루프 꼬리(0x11DC94)가 `add r6,#1 / cmp r6,#6 / add r7,#1 / add r5,#0x17 /
add sb,#2 / bne 머리` 라, 거기로 복귀하면 한 바퀴가 게스트 자신의 것과 구별되지
않는다. `PC := LR` 을 쓰는 §43.5 의 기법이다.

**`this` 는 r0 이 아니라 스택에서 읽는다** — 후크가 대신하는 그 명령이 바로
`ldr r2,[sp,#0x28]` 이고, 그 시점의 r0..r3 은 이미 쓰레기다. (앞선 시도에서
r0 으로 읽어 `GetSubState(0xF)` 폴트를 냈고, 그렇게 찾았다.)

### 45.8 인수 기준

* **1,241 프레임 5장면 A/B 통과** — `screen_option` 과 `boost_stats` 제거는
  `Config::default` 에서 off / 경로 없음이라 프레임을 안 건드린다.
* **단위시험 96 → 119**, `--selftest` 29 통과,
  `cargo check -p hl5_android --target aarch64-linux-android` 통과.
* 새 회귀 시험 `relic_draws_carry_the_table_rows_appearance_verbatim` —
  유물함이 뽑은 행의 21바이트가 옵션 칸을 뺀 전부에서 표와 **바이트 동일**.
* 새 캔버스 시험 둘 — 세 모드에서 높이·배율 불변, 폭만 이동, `WIPI_W` 하한.
* **S25 실기기 확인 (versionCode 36, 2026-08-25).** 부팅 로그가 후크 넷을 전부
  걸고, 환경설정 목록이 아래로 스크롤되면 `해상도` 가 **데이터 관리 바로 아래**
  열 번째 행으로, 오토타겟·투구보기·터치방식과 같은 판·같은 값 상자로 나온다
  (`out/dev/s25-row.png`). 행을 탭하면 주황 하이라이트와 `< B >` 화살표가 붙고
  (`s25-row-b.png`), 값이 `hl5_resolution.txt` 에 즉시 쓰인다. 다시 켜면
  `logical 526x270 (해상도 B, 90% of 585)` 로 뜨고 좌우에 118 px 씩 검은 띠가
  생기며 그림 크기는 그대로다(`s25-modeb2.png`).
* **터치 역변환도 그 상태에서 확인됐다** — 모드 B 의 레터박스(`x0 = 118`,
  배율 4.0)로 계산한 좌표 여섯(메뉴 칸, 스크롤 화살표 넷, 행, 값 화살표)이
  전부 정확히 눌렸다. 프로브에는 창이 없어 오프라인으로는 못 재는 조각이다.
* **레터박스 산술은 `hl5_core::screen::letterbox` 한 곳이 소유하고 시험이 붙어
  있다** — 프로브에는 창이 없어 캔버스→창→손가락 왕복이 오프라인에서 안 도는
  유일한 조각이라, 그 왕복을 단위시험으로 대신 돌린다(세 모드 × 여섯 열).
  A 모드는 `(0, sw)` 를 돌려주므로 옛 식과 **동일**하다.
* **오프라인 실사 확인**: `tools_re/scripts/option.txt` 가 환경설정을 열고
  `down` 아홉 번으로 열 번째 행에 내려가 `right`/`left` 로 값을 돌린다. 행이
  44번 그려지고 세 번의 변경이 전부 `hl5_resolution.txt` 에 쓰이며, 스크린샷
  (`out/shot/opt-row9.png`)에 **데이터 관리 바로 아래**, 투구보기·터치방식과
  같은 판·같은 값 상자·같은 `< >` 화살표로 나온다.
* **실제 아카이브 대조** (`tests/_tmp_relic_bytes.rs`, payload 필요):
  `item_05` 20행(표 `+0x150`=113, `+0x152`=146, 스탯 21/21, `+0x15a`=27,
  옵션 `[36,3, 32,7, 255,0]`)이 지급될 때 와이어 블록이
  `0071 0092 01 06 0015 0015 001b 03 00 00 24 03 20 07 20 04` 로 나간다 —
  **다섯 워드가 전부 표 그대로**이고, 바뀐 것은 제한레벨 바이트(0)와
  예약돼 있던 세 번째 옵션칸(`ff 00` → `20 04`)뿐이다.

### 45.9 아직 안 한 것

* **유물함 투구를 새로 사서 머리가 붙는지** — 이것 하나만 남았다. 캐릭터로 실제
  플레이해야 하고, 옛 투구는 앱 재시작으로 스스로 낫는다(§45.4).
  **재시작해도 머리가 없다면 §45.4 의 모델이 틀린 것이므로 다시 봐야 한다.**
  (참고: 이 기기의 환경설정에서 `투구보기` 가 **ON** 이다 — 그래서 증상이
  보였다. OFF 면 `/c/sp/img0/000.mgr` 로 떨어져 맨머리가 그려진다.)
* **옛 세이브의 투구는 앱 재시작으로 스스로 낫는다**(§45.4). 사용자에게 먼저
  말해야 하는 것이고, 만약 재시작해도 머리가 없다면 이 모델이 틀린 것이므로
  다시 봐야 한다.
* **`gv[0x89]`(오토타겟)의 독자를 못 찾았다.** 스캐너가 `bl` 경계에서 gv 추적을
  버리므로 놓쳤을 수 있다. 이 작업과는 무관하지만 적어 둔다.

## 46. 해상도를 바꾸면 재시작을 묻는다 — 게스트가 이미 갖고 있던 대화 (2026-08-26)

사용자 요구:

> 해상도를 변경하면 재시작해야 해상도 적용된다는 팝업을 띄워주려고 하는데
> 가능해? 재시작할지도 물어보고 확인 누르면 게임이 꺼져야 해. 아니오 누르면
> 팝업만 닫히면 돼. 해상도는 변경된 거 그대로 놔두고.

§45.6 이 남긴 구멍이 정확히 이것이다. `nativeInitDisplay` 는 프로세스당 한 번이라
행을 돌려도 **다음 실행부터** 바뀌는데, 그 사실을 말하는 곳이 아무 데도 없었다.
아무 일도 안 일어나는 설정은 고장난 설정으로 읽힌다.

**그리고 이 대화는 새로 만들 것이 아니었다 — 게임이 이미 갖고 있다.**

### 46.1 게임종료 메뉴가 쓰는 팝업이 요구 그대로다

주 메뉴 아홉 칸 중 마지막(게임종료)이 `KeyMainMenu+0x154`(0x123430):

```
0x123430  ldr  r3, [pc, #0x13c]        ; _pTextMgr
0x123434  mov  r1, #0xcb               ; common_text 203 = "종료하시겠습니까?"
0x123440  bl   #0x143170               ; TextMgr::GetCommonText
0x123444  mov  ip, #9
0x123454  mov  r1, #8                  ; <-- 팝업 id
0x123458  stm  sp, {r5, ip}            ; [sp]=0, [sp+4]=9
0x12345c  bl   #0x11b42c               ; StateMainMenu::SetPopupState(8, msg, 0, 0, 9)
```

`StateMainMenu::SetPopupState(signed char id, char* msg, int, int, int)`
(0x11B42C)는 머리에서 `strb r1,[this+0x14]`(id)와 `strb 0,[this+0x15]`(결과)를
찍고 `id - 1` 로 0x3b 항 점프테이블에 든다. 테이블 시작은 **0x11B470**(폴스루
기본분기 0x11B46C 의 *다음* 칸 — CLAUDE.md 의 그 함정), 그래서 id 8 은 7번 칸
0x11B48C → **0x11B5F8** 로 간다:

```
GetCommonText(0x32) -> sprintk -> [sp+0x20]        ; "예"
GetCommonText(0x33) -> sprintk -> [sp+0x50]        ; "아니오"   (48바이트 간격)
r1 = 2            ; 팝업 타입
r3 = msg
[sp+8]  = sp+0x20 ; char (*)[48] 버튼 배열
[sp+0xc]= 2       ; 버튼 수
[sp+0x10]= this+0x15   ; 결과를 쓸 자리
ldr pc,[vptr,#0x70]    ; BaseState::CallPopupState
```

그리고 답은 `StateMainMenu::Process`(0x124898)가 읽는다. 그 함수는 머리에서
`GetPopupState()`(= `ldrsb [this+0x14]`)로 같은 모양의 점프테이블에 들고,
id 8 은 **0x124F30**:

```
0x124F30  ldrb r6,[r5,#0x15]
0x124F34  cmp  r6,#0
0x124F38  bne  #0x124EAC      ; 1(아니오) -> SetPopupState(0,…) 로 팝업만 지우고 반환
0x124F3C  b    #0x124DC4
          0x124DC4  bl SetPopupState(this,0,0,0,0,9)   ; 팝업을 지우고
          0x124DE4  bl MC_knlExit(0)                   ; 게임을 닫는다
```

**결과 0(첫 버튼 예) = 종료, 1(아니오) = 팝업만 닫기.** 요구의 두 갈래가 이미
바이너리 안에 있고, 우리가 넣을 것은 가운데 문장 하나뿐이다.

종료의 나머지 반은 이미 배선돼 있다 —
`MC_knlExit`(0x14E4DC) → `AndroidService::exitApp`(0x14F6B4) →
`CletEntry.exitApp()` → `cletentry.rs` 의 `exit_requested` →
`hl5_android` 의 `quit` → `event_loop.exit()`. 게임종료 메뉴가 쓰는 그 길이다.

### 46.2 결과 바이트가 0 으로 시작하는데 왜 즉시 안 꺼지는가

`SetPopupState` 가 `this+0x15` 를 **0 으로 초기화**하고, 0 이 곧 "예"다. 그러면
팝업이 떠 있는 동안 `Process` 가 한 번만 돌아도 게임이 꺼져야 한다. 안 꺼진다.

`BaseState::CallPopupState`(0x45560)가 이유다:

```
0x04559c  bl   #0xabd1c   ; operator new(0xc40 + 8)
0x0455d4  bl   #0x141cd4  ; StatePopup::StatePopup(a,i,Pc,PS0_,i,PA48_c,Pa)
0x0455d8  str  r4, [r5, #0x27c]     ; *(gv+0x27c) = 새 팝업
0x0455dc  str  sb, [r4, #8]         ; 팝업+8 = 이전 상태
```

**현재 상태 포인터 자체가 팝업으로 바뀐다.** 메인 루프는 `*(gv+0x27c)` 의
`Process`/`Paint`/`KeyPress` 를 부르므로 팝업이 떠 있는 동안 `StateMainMenu::Process`
는 **한 번도 안 돈다**. 버튼이 눌려 팝업이 자기를 걷어내고 `gv+0x27c` 를 되돌린
뒤에야 그 arm 이 결과를 본다. 프로브가 이것을 그대로 재현한다 — 팝업을 띄우고
30프레임을 돌려도 `--count MC_knlExit` 는 0 이다.

(같은 함수 머리의 `cmp r1,#5 / moveq r3,#1 / strbeq r3,[r5,#0x15] / beq 0x11B5F8`
가 그 규칙의 확인이다. id 5 는 **캐릭터 슬롯 삭제**의 예/아니오이고 —
`Process` 의 그 arm 0x124F40 이 결과 0 에서 `SlotInfo::RemoveSlotData` +
`HERO::RemoveHeroData` 를 부른다 — 그래서 이 팝업만 결과를 **1 로** 미리
채운다. 0 이 언제나 첫 버튼이고 첫 버튼이 되돌릴 수 없는 쪽이라, "고르지 않은
채 닫힘"의 안전한 기본값은 1 이다.)

### 46.3 호스트가 쓴 것 — `screen::ask_restart`, 그리고 결과 후크 0개

```rust
fn ask_restart(vm: &mut Vm, this: u32) -> Result<(), String> {
    if !vm.screen.needs_restart() { return Ok(()); }
    let msg = text(vm, vm.screen.chosen.restart_message())…;
    vm.call(vm.symbol(SET_POPUP_STATE)?, &[this, 8, msg, 0, 0, 9])?;
    vm.screen.asked += 1;
}
```

`Screen::choose` 가 파일을 쓴 **뒤에** 이것을 부른다. 순서가 계약이다 —
**값은 묻기 전에 디스크에 있으므로, 답은 캔버스를 언제 다시 짓느냐만 정하고
설정을 되돌리지 않는다.** "아니오 누르면 팝업만 닫히고 B 유지"가 우연이 아니라
구조가 된다.

뒤의 세 정수는 게임종료 호출부가 넘기는 값 그대로다. 그 셋을 읽는 것은
**id 4** 의 arm(0x11B690, 3버튼, `vptr+0x7c`)뿐이고 — 첫 정수를
`GetCommonText(그 값 + 0xb2)` 의 인덱스로 쓴다 — 두 버튼 arm 은
`CallPopupState` 를 부르기 전에 셋을 전부 덮어쓰므로 값이 의미를 갖지는 않는다.
**이 호출을 *그* 호출로 만들기 위한 것**이다.

> **여기서 한 번 틀릴 뻔했다.** 3버튼 arm 을 "id 5" 로 읽었는데 실제로는
> **id 4** 다 — 점프테이블 칸 3 이 id 4 이고(`addls pc,pc,r1,lsl#2` 의 case 0 이
> `pc+8`, 즉 폴스루 분기의 *다음* 칸), id 5 는 애초에 테이블에 닿지도 않는다
> (머리의 `beq` 가 먼저 가로챈다). CLAUDE.md 가 적어 둔 그 함정이고, 이 저장소
> 에서 그 함정이 문서로 새어 나올 뻔한 것은 이번이 처음이 아니다.

문구는 모드마다 하나씩 상수 셋(EUC-KR, `;` 가 게스트의 줄바꿈):

```
해상도를 B 로 바꿨습니다.;다시 시작해야 적용됩니다.;지금 종료하시겠습니까?
```

세 토막으로 나눈 이유는 §36.4.1 이 폰에서 배운 것과 같다 — 게스트는 폭으로
줄을 접고, 한 낱말 긴 줄은 음절 가운데서 끊긴다. 각 토막은 25·25·22바이트로
게임 자신의 3줄 팝업 중 가장 긴 줄(`common_text` 0x130 의
`^아이템 구매 중 일부 시간이`, 27바이트) 안이다. 마지막 줄은 게임 자신의
`종료하시겠습니까?` 에 `지금` 을 붙인 것이다. 글자를 박아 두는 이유는
**팝업이 그 값을 보여 주는 행을 덮기 때문**이고, 모드마다 상수인 이유는
`text()` 가 슬라이스 주소로 인터닝하므로 만들어 쓰는 문자열은 누를 때마다
게스트 힙을 한 덩이씩 흘리기 때문이다.

### 46.4 안 묻는 경우가 하나 있다

`needs_restart()` = `chosen != applied`. `applied` 는 **이 프로세스의 캔버스가
지어진 모드**다. A 로 켠 판에서 A → B → C → A 로 돌아오면 마지막 걸음은
아무것도 안 묻는다 — 화면에 있는 그림이 이미 행이 말하는 그것이라 재시작이
적용할 것이 없고, 그런데도 물으면 사용자가 그 말을 믿고 행동한다.

### 46.5 백키도 아니오다 (실측)

디스어셈블로 정하지 않고 재생으로 쟀다: 팝업이 뜬 상태에서 `key back` →
`MC_knlExit` 0회, 팝업만 닫히고 행은 B 그대로(`out/shot/res-back-key.png`).
그래서 실수로 앱이 꺼지는 기본 동작이 없다.

### 46.6 이 팝업은 주 메뉴에만 뜬다 — 세이브가 위험할 수 없는 이유

`예` 가 진짜로 앱을 닫으므로 "던전 한가운데서 꺼지면?"이 당연한 걱정이다.
그런 일은 구조적으로 없다. 환경설정 목록은 `StateMainMenu` 에만 있다 —
`DrawOption`(0x11D8A0)의 호출자는 `StateMainMenu::Paint+0x1e4` **한 곳**,
`KeyOption`(0x122228)의 호출자는 `StateMainMenu::KeyPressed+0x104` **한 곳**
(`tools_re/xref.py`). 인게임 메뉴는 `StateInGameMenu` 라 이 행이 아예 없다.
즉 팝업이 뜰 수 있는 화면은 게임종료 메뉴가 나란히 있는 그 화면뿐이다.

### 46.7 인수 기준

의도한 동작 변경이라 §16.10 이후의 다른 것들과 같이 바이트 동일은 정의상
불가능하다. 대신:

* **1,241 프레임 5장면 A/B 통과** (`bash tools/ab_905.sh restartprompt`:
  newgame 119 / ingame 543 / shop 134 / online 109 / npcmenu 336, 전부
  `identical`). `screen_option` 은 `Config::default` 에서 off 이고 `ab_905.sh`
  는 그것을 안 켜므로 이 코드 경로가 아예 없다 — `hl5_core` 의 변경은 전부
  `screen.rs` 안이고, 그중 게이트 밖에 있는 것은 `Screen` 의 `asked: u64` 필드와
  그 초기화, 그리고 아무도 안 부르는 `needs_restart()` 뿐이다. §45.8 이 같은
  게이트에 대해 쓴 근거와 같은 것이라 HEAD 대비 재대조는 하지 않았다.
* **단위시험 119 → 122**, `--selftest` 29 통과,
  `cargo check -p hl5_android --target aarch64-linux-android` 통과.
* 새 시험 셋:
  `every_mode_names_itself_in_its_restart_message`(세 문구가 서로 다르고 각자
  자기 글자만 담는다 — 복사붙여넣기를 잡는다),
  `the_restart_message_is_three_runs_the_popup_can_hold`(EUC-KR 을 걸어 선행
  바이트마다 후행 바이트가 있는지까지 본다 — 반 음절로 잘린 편집이 폰이 아니라
  여기서 죽는다), `only_a_mode_other_than_the_running_one_needs_a_restart`
  (아홉 쌍 전부).
* **새 재생 `tools_re/scripts/resolution-restart.txt`** — 네 번 바꾸고 두 답을
  다 누른다. 숫자 둘이 인수다:

  ```
  resolution row  115 draw(s), 4 tap(s), 3 restart prompt(s); now B (90% width), restart to apply
  guest exit      asked for (MC_knlExit -> CletEntry.exitApp)
  counted           1  MC_knlExit
  ```

  **4 번 바꿨는데 3 번만 물었다** — 셋째 걸음이 C → A, 즉 §46.4 다.
  **`MC_knlExit` 가 정확히 1** — 예를 누른 그 한 번. 그리고
  `out/res-storage/hl5_resolution.txt` 는 `B`.
* 그림 넷: `res-prompt-b.png`(세 줄이 안 끊기고 들어가며 예가 기본 하이라이트),
  `res-prompt-no-highlighted.png`(RIGHT 이 아니오로), `res-after-no.png`(목록이
  돌아오고 행은 여전히 B), `res-back-to-applied.png`(C → A 는 팝업 없이 A).
* **캔버스 둘에서 같은 결과** — 출하 585×270(`--screen 2340x1080`)과 맨
  400×240 양쪽에서 숫자 넷(4 tap / 3 prompt / `MC_knlExit` 1 / 파일 `B`)이
  같고, 좁은 쪽에서도 세 줄이 접히지 않는다. 팝업 폭이 `GetHalfWidth` 를
  따라가므로 좁은 캔버스가 더 위험한 쪽이고, 그래서 그쪽을 따로 쟀다.

* **S25 실기기 확인 (versionCode 37, 2026-08-26).** 설치 직후 사용자가 세 모드를
  왕복했고 로그가 그것을 통째로 찍었다:

  | 프로세스 | 부팅 캔버스 | 쓴 값 | 그 뒤 |
  |---|---|---|---|
  | 1780 | 585×270 (A) | B `00:44:09` | `guest asked to exit` `00:44:12` |
  | 2452 | **526×270 (B)** | C `00:44:26` | 종료 `00:44:27` |
  | 2781 | **468×270 (C)** | A, B, A `00:44:40~43` | 종료 `00:44:44` |
  | 3300 | **585×270 (A)** | — | 그대로 플레이 |

  세 가지가 한꺼번에 증명된다. (1) **매 부팅의 캔버스가 직전 세션이 쓴 값 그대로**
  다 — 재시작이 실제로 적용한다. (2) **값을 쓴 1~3초 뒤에 종료**가 온다. pid 2452
  는 `00:44:26.435` → `00:44:27.577`, **1.14초**다. 환경설정에서 빠져나와 3×3
  격자의 오른쪽 아래 칸으로 가서 확인까지 누를 수 있는 시간이 아니므로, 그 종료는
  게임종료 메뉴가 아니라 **이 팝업의 예**다. (3) pid 2781 이 **한 세션에서 세 번**
  썼다(A → B → A) — 앞의 둘은 아니오로 닫고 목록에서 값을 계속 돌렸다는 뜻이고,
  그것이 요구의 "아니오 누르면 팝업만 닫히고 값 유지"다.

**함정 하나, 이번에 걸렸다: `option.txt` 가 조용히 무의미해졌다.** 값을 바꾸면
팝업이 그 뒤의 키와 탭을 전부 가져가므로, 그 스크립트의 `right`/`left`/스크롤
화살표 시험이 통째로 팝업에 흡수됐다(탭 수가 4 → 1 로 떨어진 것으로만 보인다).
변경마다 `key back` 을 넣어 고쳤다. **`key select` 를 쓰면 안 된다 — 첫 버튼이
예라서 재생이 게임을 끈다.**

### 46.8 아직 안 한 것

* ~~기기 시험~~ — **끝났다** (§46.7). 넷 다 S25 로그에 있다.
* **백키가 아니오라는 것만 기기에서 못 봤다.** 오프라인 재생으로는 확정이고
  (`res-back-key.png`, `MC_knlExit` 0회), 기기 로그는 백키와 아니오를 구분하지
  않는다 — 둘 다 아무 줄도 남기지 않는 것이 정상이다.
* **문구를 더 늘리려면 폭을 다시 재라.** 지금 셋은 25·25·22바이트이고 400×240
  에서도 안 접히지만, 그 여유가 크지는 않다. 한 줄이 접히면 고침은 `;` 를 하나
  더 넣는 것이다.

## 47. 헬모드 숨겨진 대장장이 둘째 쌍이 노말 전설을 만든다 (2026-08-30)

듣는자 루트에서 보스 고유드랍(사신의 숨결·알렉스의 검 계열)으로 만드는
전설무기가 헬인데도 노말 결과(미노/녹스/시엘라/세라딘, 레벨 43)가 나온다.
호스트 후크가 아니다. 출하 `smith_2.dat` 84–87행의 결과 index 가 44 다.

### 47.1 결과는 꼬리 +11 이고, 로더가 그대로 쓴다

`HERO::LoadMixSmithTableInfo` (0x8B958)가 13바이트 꼬리를 이렇게 쓴다:

| 꼬리 | 목적지 | 명령 |
|---|---|---|
| +10 | `MixSmithTableInfo+0x126` 결과 kind | `strb` 0x8BB4C |
| +11 | `+0x127` 결과 index | `strb` 0x8BB60 |
| +12 | `+0x128` 성공률 | `strb` 0x8BB7C |

`ApplyNormalMix` (0xA7DF8)는 kind 를 `+0x126` 에서 읽어 `GetItemTableInfo` 에
넘긴다. 이름 문자열은 그 경로에 없다. 레시피 이름이 알케인이어도 index 가
44 면 `item_00[44]` = 미노:창공의검 이 나온다.

### 47.2 난이도별 두 쌍

고유무기 재료(알렉스의 검 / 루퍼스의 단검 / 88mm포신 / 에블린의 창)를 쓰는
레시피는 파일마다 여덟 줄, 두 쌍이다.

| 파일 | 행 76–79 (첫째 쌍) | index | 행 84–87 (둘째 쌍) | index |
|---|---|---|---|---|
| `smith_0` | 루스 / 발키리 / 엘디르 / 모르셀 | **43** ✓ | 미노 / 녹스 / 시엘라 / 세라딘 | **44** ✓ |
| `smith_1` | 후긴 / 발드르 / 걀라르호른 / 레이븐 | **71** ✓ | 무닌 / 게리 / 플로라 / 미라펜 | **72** ✓ |
| `smith_2` | 이시르 / 프레키 / 바이드 / 에테르 | **79** ✓ | 알케인 / 펜리르 / 데스크림존 / 궁니르 | **44** ✗ (80이어야 함) |

헬 둘째 쌍은 이름·재료(사신의 숨결, 상급 룬스톤)만 헬이고 결과 index 는
노말 둘째 쌍을 그대로 둔 것이다. `item_00[80]` = 알케인:혼돈, 레벨 97.

### 47.3 듣는자와의 관계

코드가 듣는자라서 `smith_0` 을 여는 것은 아니다. 헬이면 `HERO+0x277 = 2` 로
`smith_2` 를 연다. 듣는자 루트가 주는 고유드랍이 둘째 쌍 재료이고, 고대신
쪽으로 가는 첫째 쌍(이시르…)은 같은 파일에서도 맞다. `item_15` 도감에는 이
여덟 레시피가 없다 — NPC 일반조합만.

루트별 보상 파일 `N_rewards_M.dat` 의 첫째 인덱스는 루트(0/1), 둘째는
난이도다. `2_rewards_*` 는 없다.

### 47.4 고침

`rates::fix_hell_legendary_results`. 꼬리 모양이 `04` (재료 셋) + kind 0..3
+ index 44 + 셋째 재료 kind 13 / index ∈ {0x4E, 0x51, 0x54, 0x55} 인 행만
+11 을 80 으로 쓴다. 이름은 안 본다. `smith_2` 에만 적용한다 — 같은 술어가
`smith_0` 의 올바른 44 에도 맞기 때문이다.

확률 레버와 무관하다. `Config::boost_rates` 가 꺼져 있어도
`install_hell_legendaries` 가 `smith_2` 그림자를 쓴다. 켜져 있으면
`install` 이 성공률을 올린 뒤에 같은 리맵을 한 파일에 넣는다.

## 48. 대시 연타 투명·공격 후 이동 불능 — `SetWalkMotion` 의 인자가 틀렸다 (2026-08-31)

사용자 보고 둘: 이동 중 대시를 연타하면 캐릭터가 투명해지고 느려지며 행동은
안 되는데 피격은 맞는다. 그리고 공격이 끝난 뒤 잠깐 이동이 안 된다.
둘 다 `keep_walking` 의 `SetStandMotion` 후크다. §38.8 이 그 후크를 넣었고,
기기에서 이 두 증상이 나온 것은 그 후크가 자리와 인자를 둘 다 틀린 채로
돌았기 때문이다.

### 48.1 방향은 두 공간이다

조이스틱이 `gv+0x11c` 에 쓰는 것은 WIPI 키 **−1..−4** 다
(`TouchMoved` 0xE1648 / 0xE1734 / 0xE1774 / 0xE16A4). `SetWalkMotion` 과
`CHAR::SetDir` 과 `SPRITE::GetLastFrame` 이 받는 것은 스프라이트 방향
**0..3** 이다. `CHAR::GetInvertDir` 가 그 넷만 뒤집고 (0↔1, 2↔3),
`WARRIOR::KeyPress+0x3f0` 은 `SetAngleMoveKeyChk` → `GetAngleToDir` 로
0..3 을 만든 뒤에야 `SetWalkMotion` 을 부른다.

`GetAngleToDir` / `WorkMoveHero` 의 카드널:

| 각도 | facing | 내비 키 |
|---|---|---|
| 270° | 0 | UP −1 |
| 90° | 1 | DOWN −2 |
| 0° | 2 | RIGHT −4 |
| 180° | 3 | LEFT −3 |

후크가 `SetWalkMotion(gv+0x11c)` 를 하면 facing = −1 이 된다.
`GetLastFrame(motion, −1)` 은 프레임이 없고, `SPRITE::DrawSprite` 가
그 레이어를 건너뛴다 — **캐릭터가 투명하다.** 히트박스는 `CHAR` 좌표에
남아 있으니 피격은 맞는다. 대시를 연타할수록 `EndProc` 이 그 후크를
반복해서, 대시가 끝난 프레임마다 잘못된 facing 이 다시 심어진다.

### 48.2 `SetStandMotion` 은 정리 함수다

`SetStandMotion` (0x98FE0, 284 B) 은 서기만 심는 게 아니다.
`SetNextActSkillIdx(−1)`, 콤보 카운트 둘, `SetIsComboDefault/Skill(0)`,
`SetFeintBackStepState(0)`, `SetIsShadowStepTarget(0)`,
`InitColorChar` / `InitColorBack`, 건슬링어 `HERO+0x1fad` 를 지우고
`gv+0x278` 을 −1 로 만든다. `SetWalkMotion` 은 그중 색과 메인상태만
건드린다.

후크가 원본을 통째로 건너뛰면 대시·스킬 extra-frame 색과 스킬 인덱스가
남는다. `CheckExtraFrame` 가 매 틱 `ApplyColorChar` / `ApplyColorBack` 을
다시 칠하고, `ProcHeroSkill` 이 남은 인덱스를 먹는다. 그게 "느려지고
아무 행동도 안 된다"의 나머지 반이다.

그리고 `WorkMoveHero` (0x8D020) 의 첫 명령이 `cmn r3, #1; beq return`
이다. 각도가 −1 이면 **픽셀을 한 칸도 안 옮긴다.** 공격 종료의
`SetStandMotion` 이 그 −1 을 쓰고, `UpdateNaviAngle` 도 스틱이 안 움직이면
같은 −1 을 쓴다 (0xDE910). 후크가 걷기 모션만 심고 각도를 안 되살리면
"공격 후 잠깐 이동이 안 된다." 스틱을 한 번 비틀면 `TouchMoved` 가
새 각도와 `KeyPress` 를 내니 풀린다.

### 48.3 후크가 받아서는 안 되는 자리

`SetStandMotion` 호출처 34곳. 공격·대시·스킬이 *끝난* 자리는 클래스
`EndProc` 여덟과 `HERO::EndProc` 하나뿐이다. 나머지를 걷기로 바꾸면
원본이 기대하는 정리가 사라진다:

* `HERO::AddEffectStun` (0x99260) — 선 뒤에 `SetDecreaseStunFrame`
* `TargetEffect::ProcDemageCalc`, `Monster::MonAttackProc` — 피격 리액션
* `HERO::Revival`, `DecreaseActiveBlockFrame`
* `EventProc` 의 스킵·씬·종료, `NewGame`, `ChangeHeroClass`, `StatePopup`
* `CheckSKillUse` / `ProcHeroSkill` / `EndProcException` — 스킬 거절·예외

대시 *도중* 그 자리들이 서는 것은 원본 동작이다. §38.7 의 GUNNER·KNIGHT
`KeyRelease` 모션 강등 (0xf→0xe, 0x1e→0x1d) 은 그대로 범위 밖이다.

### 48.4 고침

`hl5_core/src/input.rs` 의 `walk_if_stick_held`:

1. `lr == RETURN_MAGIC` 이면 사양 — 후크가 원본을 부를 때의 재진입.
2. 스틱이 없거나 데드존이면 사양 — 원본대로 선다.
3. `lr` 가 위 아홉 `EndProc` `bl` 의 복귀주소가 아니면 사양.
4. 메인상태가 이미 1 이면 서기만 건너뛴다.
5. 그 밖(공격·대시·스킬 종료)은 **원본 `SetStandMotion` 을 먼저 돌리고**,
   저장해 둔 `gv+0x278` 을 되돌리거나 카드널을 쓰며,
   `SetWalkMotion(GetAngleToDir(각도) 또는 navi→0..3)` 을 부른다.

`Config::keep_walking` / `hl5_keep_walking.txt` 게이트는 그대로다.

## 49. 헬 보스 세트 드랍의 셋째 줄이 `+랜덤옵션` 문구만이다 (2026-08-31)

유물함(§44)과 같은 구멍, 다른 문. 출하 `item_05..08` 세트 행(23..26, 43..46,
75..80)은 전부 슬롯 2 종류가 `0xFF` 다. `ItemTable::NewItemEx`(0xA4CAC)는
`MakeSocket` 뒤에 `MakeItemOption`(0xA10E8)을 불러 그 칸을 채운다.
`MapItem::NewDropItem`(0xA7664)는 `GetItemTableInfo` → (부호 있는 오버라이드
`>= 0` 일 때만 등급/옵션 덮어쓰기) → `MakeSocket` 이고 **`MakeItemOption` 은
호출자 0곳**이다. 추가 드랍은 오버라이드를 전부 `0xFF` 로 넘겨 표 값을
살리므로, `CommonUi::DrawItemDiscription` 의 `ldrsb` / `bgt`(0x7B83C)가
`common_text` 287 = `+랜덤옵션` 을 그린다.

오토픽업은 `NewDropItem` 안에서 `NewBagItem` 으로 복사한 뒤 맵 아이템을
지우므로, 반환 뒤에 채우면 가방 쪽이 빈 칸으로 남는다. 후크는
`MakeSocket` 진입이고 복귀주소가 `NewDropItem+0x48c`(0xA7AF0)일 때만,
등급 `>= 5` 이고 슬롯 2 종류 `<= 0` 일 때 원본 `MakeSocket` 을 돌린 뒤
게스트 `MakeItemOption` 을 부른다. `NewItemEx`·조합은 복귀주소가 달라
사양한다. 원본 레어/에픽/영웅 드랍(등급 0..4)도 그대로다.

이미 가방에 있는 빈 칸 세트는 지급 표와 같이 고치지 않는다 — 다시 떨어뜨려야
한다.

## 50. `(진)` 은 창고에 들어간다 — 게스트의 `index > 80` 만 연다 (2026-08-31)

`(진)` 은 `item_00..03` 86번이다. 유물함 펜스(`RELIC_ROW_MAX` = 80)와 공허
1~5급(81..85) 뒤에 붙인 자리라, 대장장이·캐시샵·유물함은 원래부터 안 본다.
창고 올리기는 다른 문이다.

`StateNetMenu::KeyStorage` (0x12D3CC) 가방 입고 경로:

```
0x12DDF4  ldrb  r3, [sl, #0x14]     ; kind
0x12DDF8  cmp   r3, #3
0x12DDFC  bhi   0x12DE0C            ; kind > 3 → 무기 펜스 아님
0x12DE00  ldrsb r3, [sl, #0x15]     ; index
0x12DE04  cmp   r3, #0x50
0x12DE08  bgt   0x12DBC8            ; 거절 팝업, 패킷 없음
```

86 > 80 이라 `(진)` 은 여기서 죽는다. 출하 헬 종결(80번)은 `bgt` 가 아니라서
원래부터 창고에 들어간다. 호스트 창고는 도착한 72바이트를 그대로 보관하므로
고칠 것은 이 비교 하나다.

후크는 `KeyStorage+0xa34`(0x12DE00). `sl` 의 kind ≤ 3 이고 index 가 86 일
때만 `LR` 을 `+0xa40`(0x12DE0C, `(10, 17)` 검사)로 쓰고 `true` 를 돌려
거기를 재개한다. 공허 81..85, 장착품, kind 14/18, `(10, 17)` 은 사양해서
원본 `bgt` 가 그대로 거절한다. 터치는 커서를 옮길 뿐이고 SELECT·같은 칸
두 번째 탭은 `KeyPressed` 0x139848 한 곳으로 모인다.

드랍 전용·그룹 63·유물함 펜스는 그대로다. `legendaries.rs`.

## 51. 정체모를 괴수가 죽으면 스토리가 바로 시작돼 드랍을 못 줍는다 (2026-09-01)

사용자 보고: **다른 보스에는 루팅타임이 있는데 정체모를 괴수는 죽는 순간 화면이
넘어가면서 아이템을 주울 수가 없다.**

### 51.1 표에는 칸이 있다 — 그것도 전설 무기 그룹이다

`/c/csv/enemy_<N>.dat` 를 훑으면 이 이름의 행이 **둘**이다. 레코드 오프셋은
이름 뒤 39 = 사다리 일곱(§42.6), **70 = 드랍그룹**(`Monster+0x273`),
**72 = 굴림 수**(`Monster+0x275`). 뒤의 둘은 `legendaries` 가 드랍표에서
유도해 둔 헬 보스 그룹 59..62 · 4굴림과 정확히 맞아떨어져 확인됐다.

| | 행 7 `'정체모를 괴수{` | 행 8 `정체모를 괴수` |
|---|---|---|
| 노말 / 하드 / 헬 그룹 | 44 / 49 / 55 | 44 / 42 / 56 |
| 굴림 | **4** | **0** |
| 사다리 | 정상 | **전부 0** |

그룹 44·49·55 는 네 칸이 전부 **전설 무기**다 — 페이튼:워액스 계열(제한 34) /
그리드:거신 계열(57) / 그람:궁극의힘 계열(82). 굴림 하나당 노말 기준 재료 1.0%,
**밴드 3 이 50.0%**, 전설 12.0%(밴드 4·5·6 이 전부 등급 4다), 없음 37.0%.

**밴드 1·2·3 은 드랍그룹을 안 본다 (2026-09-01, 실제 드랍이 가르쳐 줬다).**
밴드 3 의 arm(0xBCDA0)은 `Rand(0,9)` 로 **장비 종류를 뽑고**(0..3 무기 / 4
스태프 / 5..8 방어구 / 9 방패), 아이템은 그 몬스터 자신의 기준 index
`Monster+0x272` 를 `+0x274` 퍼센트(`+0x64` = 100 을 더해 100분율로 곱한다)로
흔들어 정하며, `mov r2,#2` 로 등급만 에픽으로 덮어쓴다. 즉 **그룹의 전설을
강등하는 것이 아니라 그 레벨대의 아무 장비를 에픽으로 주는 것**이다. 밴드 1·2
도 같은 모양이고 `r2` 만 0·1 이다. 드랍그룹을 쓰는 것은 밴드 4·5·6 뿐이다.

정체모를 괴수의 `+0x272` 는 **13**, `+0x274` 는 8 이고, `item_*.dat` 의
index 13 이 정확히 **산토리아** 한 벌(대검/대거/슈터/장창/햇/코트/핸드/슈즈/
실드, 전부 등급 노멀·제한 Lv32~36)이다. 노말에서 실제로 나온 `에픽 산토리아핸드`
+ `에픽 산토리아대검` 이 그 두 줄의 직접 확인이다 — 산토리아는 `droptable.dat`
252칸 어디에도 없다.

**같은 이름 두 행 중 하나가 죽어 있는 것은 이 표의 흔한 모양**이다 — 아일린
35/36, 그리고 `[박스{`·`[광물{`·`[풀{` 138..142 대 161..165 가 같은 꼴이다.
어느 쪽을 맵이 스폰하는지는 바이너리로 결정되지 않는다(스폰은 `/c/map/*.scn`).

### 51.2 왜 못 줍는가 — 이유가 둘이고 둘 다 가능하다

* **바닥이 지워진다.** `MapItem::DeleteAll`(0xA74F0)의 호출자는
  `EventProc::Scene_Init+0x478`(0x72820) 하나뿐이고, `Scene_Init` 의 호출자는
  `StateGame::LoadProc` 둘뿐이다. **맵을 부르는 장면은 바닥의 모든 아이템을
  지운다.**
* **드랍 자체가 안 일어날 수도 있다.** `Monster::SetDropItem` 의 진입점은
  `Monster::EndProc+0x12c`(0xC21BC) **한 곳**이고, 그 앞에 `GetMotion()==5` ·
  `GetDecreaseDieFrame()==0` · `HP<=0` 셋이 있다. 죽는 모션이 끝나기 전에 맵이
  갈리면 그 자리에 영영 도달하지 못한다.

`Monster+0xf0` 은 이벤트 플래그가 아니라 **HP** 다(`BATTLER::IncreaseHP` ·
`HERO::CalcStatusComputation` 이 같은 자리에 쓴다). `EndProc` 의 `bgt` 는 죽음을
한 번 더 확인하는 것뿐이다.

### 51.3 고침은 게스트 자신의 오토픽업이다 — 새 코드가 아니라 분기 선택

`MapItem::NewDropItem` 은 `MakeSocket` 직후 `HERO::GetAutoPickupItem`(0x89D48 =
`HERO+0x310`/`+0x314` 의 int64 버프 타이머)을 부르고, **양수면 0xA77F0 으로 가
`GetBagItemPtr` → `BagItem::NewBagItem` → `Battle::SetItemPickupUi`** 를 한다.
64비트 비교 모양은 성장의 서 타이머와 같다(상위워드 `bgt`, 그다음 `bne`/`beq`).

그래서 `hl5_core/src/bossdrop.rs` 가 하는 일은 둘뿐이다:

1. 그 보스의 드랍을 **죽는 순간**(`Monster::SetDieMotion`) 한다.
2. 그 한 번의 호출 동안만 오토픽업 대답을 양수로 잡는다.

**무엇이 어떤 등급으로 소켓 몇 개로 나오는지는 한 줄도 안 바꾼다** — 게스트
자신의 `SetDropItem` 이 몬스터 자신의 사다리로 돌고, 원래 있던 두 끝 중 하나를
고를 뿐이다. 판정은 이름이다: `setEnemyData` 가 레코드의 이름을
`Monster+0x218` 에 `memcpy` 한다(0xC1B9C). 드랍그룹으로는 안 된다 — 헬의 55는
크리스·강화병과 공유다.

두 번 나가지 않는다: `SetDieMotion` 은 `CHAR::SetMotion(5)`(0xC086C)로 끝나고,
`EndProc` 의 두 호출(0xC24C4·0xC2844)은 둘 다 `GetMotion() != 5` 뒤에 있다.
드랍 뒤에는 사다리 일곱과 굴림 수를 0 으로 만들어, 장면이 허락해서 자연스러운
`SetDropItem` 이 나중에 돌더라도 아무것도 안 굴리게 한다(행 8 과 같은 모양이고
다음 스폰 때 `setEnemyData` 가 파일에서 다시 채운다).

**죽은 쌍둥이 대비**: 죽은 몬스터의 사다리가 비어 있으면, 부팅 때 아카이브에서
읽어 둔 **표시된 행**의 사다리·그룹·굴림을 난이도(`HERO+0x277`)에 맞춰 넣고
굴린다. 호스트가 지어낸 숫자가 아니라 표의 숫자다. 부팅 로그가 세 난이도의
그 숫자를 그대로 찍는다.

### 51.4 장신구 1%

같은 자리에서 독립 굴림 하나(`Rand(0,99) == 0`)로 `item_10.dat` 의 전설 네 행
(4 기사의징표 / 9 스폰의사슬 / 14 홀리즈이어링 / 15 카오스이어링) 중 하나를
같은 `NewDropItem` 으로 준다. 무기 밴드에서 확률을 빼앗지 않는다. 출하
드랍표에 이미 `kind 10` 칸이 38개라 드랍·줍기 경로는 게임 자신의 것이다.

### 51.5 인수 기준

1,241 프레임 5장면이 **HEAD 대비 바이트 동일**(worktree A/B), 단위시험 136 →
**141**, `--selftest` 29, `cargo check -p hl5_android --target
aarch64-linux-android`. 오토픽업 후크는 해제 상태에서 원본의 세 명령을 그대로
재현하므로 다른 어떤 드랍도 움직이지 않는다.

**기기 미시험.** 판정은 로그다: `bossdrop:` 줄이 세 난이도의 사다리를 찍고,
보스를 잡으면 `spawned with the empty row` 가 나오는지(= 맵이 행 8 을 쓴다)
또는 안 나오는지(= 행 7)로 어느 쪽이었는지가 그 자리에서 갈린다.

## 52. 게임패드 — 버튼은 키로, 스틱은 게스트 자신의 조이스틱으로 (2026-09-08)

사용자 요구: **패드 사용자에게 키 맵핑을 주고, hero3 가 쓰는 방식을 그대로 쓰고,
가능하면 퀵슬롯마다 어떤 패드 버튼인지 화면에 표시한다.**

hero3(=hero2 포크)의 참조 구현은 `wie_android/src/{gamepad.rs, padhud.rs}` 이고,
그 **방법론은 그대로 왔다**: 안드로이드 키코드 → 게스트 키 표, 텍스트 파일
오버라이드, 그리고 **배지는 그 표의 역룩업**이라 remap 하면 배지가 따라 움직인다.
문자가 아니라 **위치**를 그리는 것도 그대로다(키코드 96 은 Xbox 에서 `A`,
Switch 에서 `B`, PS 에서 `✕` 다).

**그런데 이식되지 않는 것이 하나 있고, 그것이 이 절의 본체다.**

### 52.1 이 빌드에서 방향키는 주인공을 움직이지 못한다

`WARRIOR::Proc+0x228`(0x147418, 그리고 `ROGUE`/`GUNNER`/`KNIGHT` 의 같은 세
명령)이 `HERO::WorkMoveHero(this, speed, 1, (s16)gv+0x278)` 를 부르고,
그 함수의 **첫 명령이 관문**이다:

```
0x8d020  ldr  ip, [pc, #0x1cc]
0x8d024  cmn  r3, #1                ; angle == -1 ?
0x8d038  beq  #0x8d1d0              ; -> pop {r4..r7, pc}   ← 그대로 반환
```

즉 **각도가 −1 이면 이동이 0 픽셀**이다. 그리고 네 클래스의 `KeyPress` 는
방향키를 받아서도 그 키를 이동에 쓰지 않는다 — 0x147858 에서 **`gv+0x278` 을
다시 읽어** `HERO::SetAngleMoveKeyChk` 에 넘기고, 키 자신은 바라보는 방향
(`SetWalkMotion`)과 더블탭 대시 판정에만 쓴다.

`gv+0x278` 에 쓰는 곳을 전수 조사하면(capstone, `.text` 전체에서 즉치 0x278
52곳) **전부 터치다**:

| 쓰는 곳 | 무엇 |
|---|---|
| `StateGame::UpdateNaviAngle+0x84/+0xf8` | 5° 로 양자화한 각도. **호출자는 `TouchMoved+0xfc` 와 `TouchPressed+0x700` 둘뿐** |
| `StateGame::TouchMoved` / `TouchReleased` / `TouchPressed` | 래치·해제 |
| `HERO::SetStandMotion+0x104` | −1 로 지운다 |

> **따라서 패드의 십자키를 WIPI −1..−4 로 보내면 걷는 모션만 나오고 제자리에
> 선다.** 이것은 우리 셸의 버그가 아니라 출하된 안드로이드 이식의 구조다 —
> 이 이식은 이동 벡터를 터치 조이스틱의 각도에서만 얻는다. hero3 는 WIPI Java
> 게임이라 키가 곧 이동이었고, 그래서 hero3 의 "스틱 → 방향키" 절반은 여기
> **그대로 오면 안 된다.**

### 52.2 그래서 스틱은 게스트 자신의 조이스틱에 얹는다

호스트가 **가짜 손가락**을 넣는다. `Vm::touch` 는 이미 포인터 id 를 받으므로
(§37.6/§38.4) 후크도 게스트 메모리 쓰기도 **0개**다.

* 누르는 자리는 **두 터치방식이 공통으로 중심이라고 보는 점** (0x41, H−0x41):
  고정 스틱 방식(`gv+0x8f & 1`)은 0xE2150 에서 `gv+0x274/0x276` 에 그 값을 쓰고
  `gv+0x120/0x122` 로 복사한 뒤 공통 래치 0xE1CC8 로 가고, 부동 방식은 누른
  점을 `x ≥ 0x2d`, `y ≥ 0x41` 로 클램프해 같은 자리에 쓴다. **그래서 정확히 그
  점을 누르면 두 방식이 같은 제스처를 본다.** 눌러 놓고 끌어야 하는 이유가
  여기다 — 부동 방식은 **누른 점이 원점이 되므로** 기울어진 점을 누르면 이후
  모든 각도가 어긋난다.
* 반경은 **48 캔버스 픽셀**이다. 게스트는 `TouchMoved+0x130` 의 ±10 px 데드존
  (`cmn dy,#0xa` / `cmp dx,#0xa`)을 넘어야 방향을 래치하므로, 우리 engage 문턱
  0.25 × 48 = 12 px 가 그것을 넘는다.
* 포인터 id 는 **0x70**. 셸은 실제 손가락에 0 부터 슬롯을 주므로
  (`App::touch_slot`) 부딪히려면 손가락 113개가 필요하다. 그래서 진짜 엄지로
  스킬을 눌러도 패드의 스틱이 풀리지 않는다(그 반대도).
* **십자키도 같은 길로 간다.** 패드의 절반은 십자키를 HAT 축으로 보내고
  (그러면 스틱과 같이 합산된다) 나머지 절반은 `KEYCODE_DPAD_*` 키 이벤트로
  보낸다. 후자를 그대로 게스트 키로 넘기면 §52.1 대로 제자리 걸음이 되므로,
  **동작이 방향키인 컨트롤은 키를 내지 않고 조종한다** — 디지털 해트 벡터
  (`Pad::dhat`)에 ±1 로 더해져 스틱과 같이 양자화된다. 그래서 십자키 둘을 같이
  누르면 대각선이 되고, 하나를 떼면 남은 하나로 계속 걷는다. 이것은 십자키
  특례가 아니다 — `hl5_gamepad.txt` 로 아무 버튼을 방향에 묶어도 같다.
  (구현 중에 이 구멍을 놓쳤다가 잡았다: 처음 판은 십자키를 키 표에 그대로
  두어서, 키 이벤트를 보내는 패드에서는 십자키가 죽은 것처럼 보였을 것이다.)
* **필드 밖에서는 같은 스틱이 방향키가 된다.** 메뉴는 키로 돈다. 판정은
  `StateGame::GetTouchJoyXY` 관측 후크다 — 호출자가 `DrawTouchUi+0x84` **하나**
  이고 그 자리는 `DrawTouchUi` 의 세 조기반환(`gv[0x27c]->[4]==1`,
  `gv[0x148c]->[0x79]==0`, `Battle::IsGameUIView()==1`) **뒤**라, 그것이 불렸다는
  사실이 곧 "필드 HUD 가 이 그림에 있다"다. 래치는 §28 의 `Hud::frame_start` 와
  같은 시계(게스트의 재칠)에 둔다.

**오프라인 증명**(같은 재생, 같은 바이너리, 이동 일곱 줄만 다르다):

| `tools_re/scripts/gamepad.txt` | `--count 0x8d03c` (= `WorkMoveHero` 가 −1 관문을 통과한 횟수) |
|---|---|
| `pad-stick` · `pad left/down` 있음 | **50** |
| 그 줄들을 지우면 | **0** |

그리고 `pad-2-walk-right.png` 에서 게스트 자신의 스틱 손잡이가 오른쪽으로
기울어 그려지고 맵이 스크롤한다. 처음 만든 재생은 위로 너무 오래 걸어서 NPC
대화가 열렸다 — 스틱이 실제로 주인공을 옮긴다는 뜻이라 그것 자체가 증거였지만,
배지 사진으로는 나쁜 화면이라 걸음을 짧게 줄였다.

### 52.3 버튼은 후크가 필요 없다 — 이 게임의 터치 UI 가 키 합성기다

`StateGame::TouchPressed` 는 버튼마다 `KeyPress`(vptr+0x1c)를,
`TouchReleased` 는 `KeyRelease`(vptr+0x20)를 낸다. 즉 **화면 버튼과 패드 버튼은
게스트에게 같은 것**이다.

| 화면 컨트롤 | 키 | 누름 자리 | 뗌 자리 |
|---|---|---|---|
| OK | −5 | 0xE1B94 | 0xE19A8 |
| 벨트 1~5 | `'1' '3' '9' '*' '0'` | 0xE2514 / 0xE24FC / 0xE24CC / 0xE24B4 / 0xE249C | 0xE1A5C / 0xE1A44 / 0xE1A14 / 0xE19FC / 0xE19E4 |
| 콤보 | `'7'` | 0xE24E4 | 0xE1A2C |
| 벨트 페이지 | `'#'` | 0xE285C | — |

**그리고 "페이지 이동"이 실제로 있다.** `'#'` → `HERO::ChangeHotKeyToggle`
(0x88B80)이 `toggle = 1 - toggle` 이고, 다섯 슬롯이 전부
`HERO::GetHotKeyBeltType(slot, toggle)` = `HERO[0x1b4c + slot*2 + toggle]` 을
읽는다 — **벨트는 5칸 × 2페이지**이고 `'#'` 이 그 사이를 오가는 유일한 키다.
화면에도 그 버튼이 있다(`this+0xf8/0xfa`, 30×30, 넓은 캔버스에서
`(W−0xbf, H−0x2a)`).

**덤으로 인게임 바 세 칸도 키다** — `Battle::KeyPressed`(0x48E70)가 키를 클래스
`KeyPress` 에 넘긴 **뒤** −10 / −7 / −6 을 자기가 처리한다. −10 은
`HERO::SaveAll` + `GetCommonText(0xbc)`(0x49010) 이라 **패드로 저장이 된다** —
hero3 가 호스트 기능으로 만들어야 했던 빠른저장이 여기서는 게스트 자신의 키다.
−6 은 `BaseState::StateMgr(2, 7)`(vptr+0x6c, vtable 덤프로 확인), −7 은 상점 쪽
분기다.

### 52.4 기본 배치 — hero3 그대로, 단 **스틱 없는 패드**에서 두 칸을 옮겼다

첫 판은 hero3 를 버튼 하나까지 그대로 베꼈다. **사용자가 자기 패드로 그것을
반증했다**: "L3랑 R3가 없는 패드도 있는거 아니야? 내 패드에는 L1,2 R1,2밖에
없는거같은데?" 맞다 — 스틱이 없는 블루투스 패드에는
`KEYCODE_BUTTON_THUMBL/THUMBR` 가 **존재하지 않는다**. 그런데 첫 판은 hero3 를
따라 **벨트 페이지를 R3, 저장을 L3** 에 두었고, 그러면 그런 패드에서
**사용자가 이름을 대서 요구한 페이지 넘기기가 아예 닿지 않는다.**

스틱 없는 패드의 물리 컨트롤은 십자키 4 + A/B/X/Y + L1/R1/L2/R2 + SELECT/START
= **이동 빼고 버튼 10개**다. 넣고 싶은 동작은 12개(OK·취소·벨트 1~5·콤보·페이지·
지도·저장·대시)라 **둘은 버튼을 못 받는다.** 무엇을 뺄지는 측정이 정한다 —
**지도(−6)와 저장(−10)은 인게임 상단 바의 자기 칸**이고 그 바는 필드 화면마다
그려지므로(§28), 탭으로 닿고 전투 중에 쓰는 것도 아니다. 그래서 그 둘이 스틱
누르기로 가고, 전투에 쓰는 것 전부 + 페이지가 실제 버튼에 남는다.

| 위치 | hero3 | hero5 |
|---|---|---|
| 면버튼 아래 (A) | OK | OK (−5) |
| 면버튼 오른쪽 (B) | CLR | CLEAR (−16) |
| 면버튼 왼쪽 (X) | 퀵 1 | 벨트 1 (`'1'`) |
| 면버튼 위 (Y) | 퀵 2 | 벨트 2 (`'3'`) |
| L1 / R1 / L2 | 퀵 3·4·5 | 벨트 3·4·5 (`'9' '*' '0'`) |
| R2 | 보조무기 (7) | 콤보 (`'7'`) |
| **SELECT** | 지도 (\*) | **벨트 페이지 (`'#'`)** ← 옮겼다 |
| **START** | CLR (B·BACK 과 중복) | **대시** ← 옮겼다 |
| R3 | 퀵바 페이지 | 지도 (−6) — 있는 패드만 |
| L3 | 빠른저장 | 저장 (−10) — 있는 패드만 |
| 십자키 / 왼스틱 | 이동 | 조이스틱 (§52.2) |

`the_whole_field_kit_avoids_the_stick_presses` 가 이 성질을 지키는 시험이다 —
전투용 아홉 + 페이지가 `StickPress` 나 `Dpad` 에 앉으면 실패한다. 되돌리기 전에
그 시험을 먼저 보라.

`hl5_gamepad.txt` 한 줄로 전부 바꿀 수 있다(`<키코드>=<토큰>`, 토큰은
`UP DOWN LEFT RIGHT OK CLR MAP SHOP SAVE PAGE DASH NONE` 과 `0`..`9` `#` `*`).

**대시는 게스트 자신의 더블탭이다.** 화면 대시 버튼은 키가 아니라
`SetAttackMotion(7,dir)`+`SetDashFlag`+`SetDashCheckFrame` 을 직접 부르지만
(0xE1EEC 계열), `<CLASS>::KeyPress` 가 방향 누름마다
`SetDashFlag(dir,1)`+`SetDashCheckFrame(10)` 으로 무장하고 10프레임 안의 다음
누름에서 대시한다(0x147880 / 0x147A24). 그래서 방향키 누름을 **두 번** 큐에
넣는다 — 하나가 무장하고 다음이 대시하며, 우편함 때문에 서로 다른 틱에
도착하는 것이 오히려 맞다. 뗌은 보내지 않는다(방향 뗌은 걷기를 끝낸다).

**방향은 `gv+0x11c`(마지막 래치)가 아니라 주인공의 바라보는 방향이다** —
`HERO+0x2d`(`CHAR::GetDir`, 0x49B84). 게스트의 대시 관문이 `GetDir() == dir` 를
비교하므로(0x147A4C) **그 값만이 대시할 수 있다.** 걷는 중에는
`WorkMoveHero` 가 매 틱 `CHAR::SetDir` 로 각도에서 갱신하니 같은 값이고, 서
있을 때도 유효하다. 처음에 `gv+0x11c` 를 읽게 만들었더니 **선 자리에서 대시가
안 나갔다**(`--count 0x147a50` 이 0) — 고친 뒤 1이다.

### 52.5 큐가 필요한 이유 — 우편함이 종류마다 한 칸이다

§37.8 그대로: `BaseState::KeyPress`(0x44CBC)는 `gv+0xE4 == 0` 일 때만 기록하고
같은 틱의 두 번째 키다운을 **조용히 버린다**. 패드는 36 ms 안에 두 에지를
쉽게 만든다. 그래서 모든 패드 에지는 큐에 들어가고 `Vm::pad_pump` 가
**빈 칸을 확인하고 하나씩** 넘긴다:

| 종류 | 확인하는 바이트 |
|---|---|
| 키 누름 / 뗌 | `gv+0xE4` / `gv+0xE5` |
| 터치 누름 / 뗌 / 이동 | `gv+0xF0` / `gv+0xF1` / `gv+0xF2` |

스틱 드래그는 큐에서 **합쳐진다**(넘기지 못한 이동은 최신 위치만 의미가 있고,
게스트도 `gv+0xF2` 가 서 있으면 기록조차 안 한다). 재생에서 면버튼 둘을 한 틱에
눌러도 둘 다 도착하는 것이 그 증거다.

### 52.6 배지는 호스트가 완성된 그림 위에 그린다

이 게임은 터치 UI 를 **게스트가** 그리므로 hero3 처럼 호스트 HUD 에 배지를 달
자리가 없다. 그래서 `Vm::gl_draw_body` 가 텍스처에서 캔버스(`GlState::fb`,
`0x00RRGGBB`)를 다 만든 **직후**에 그린다. 결과 셋이 전부 이득이다:

* **게스트 메모리를 안 쓴다** — 블리터·블렌딩·프레임버퍼 복사가 이 주석을 되읽을
  길이 없다.
* 전부의 위에 올라간다(전부 뒤에 그리므로).
* **`--dump` PNG 에 나온다** — 그래서 이 기능이 오프라인 픽셀로 증명된다.

좌표 변환도 없다: 캡처 계층의 프레임버퍼가 곧 논리 캔버스다.
컨트롤 사각형은 후크가 게스트 자신의 `StateGame` 에서 읽는다 —
`+0xe0`/`+0xec` 여섯 버튼(30×30), `+0xf8`/`+0xfa` 페이지 버튼,
`+0xb8` 의 `IRect`(OK). 실측으로 나온 값은
`buttons [(240,192) (272,179) (304,158) (335,133) (354,100) (364,64)]
page (209,198) ok (339,170,60,60)` (400×240 캔버스) — 오른쪽으로 올라가는 호이고,
그래서 배지는 컨트롤 **안쪽 왼아래**에 둔다(림에 두면 옆 버튼과 겹친다).

배지는 30 px 컨트롤에 맞춰 반지름 6.9 px 다(출하 4.0배에서 28 화면픽셀).
언어는 hero3 와 같다 — **집합을 그리고 해당 멤버를 밝힌다**: 면버튼은 네 점의
다이아몬드, 어깨는 2×2 막대, 가운데 버튼은 알약 둘, 스틱은 원 둘, 십자키는 십자.
**첫 시도에서 어깨의 미점등 셋이 안 보였다** — "오른쪽 아래 *넷 중 하나*"가
아니라 "오른쪽 아래에 뭔가"로 읽혀서, 미점등 막대를 보이게 하고 좌우 간격을
넓혔다. 8배 확대 대조로 판정했다.

`hl5_pad_hud.txt` = `auto`(기본, 패드가 붙어 있을 때만) / `on` / `off`.
연결 여부는 Java 가 `InputDevice.getDeviceIds()` 로 세고
`InputManager.InputDeviceListener` 로 계속 갱신한다(블루투스 패드는 게임이
떠 있는 동안 붙는 것이 보통이다). 값이 바뀌면 `GlState::forget_last_draw` 로
호스트의 "직전과 같은 그림" 생략을 무효화한다 — 안 하면 게스트가 다시 그릴
때까지 배지가 화면에 나타나거나 사라지지 않는다.

### 52.7 패드 이벤트가 Java 에서 넘어오는 길

`android_native_app_glue` 의 `default_motion_filter` 가 터치스크린이 아닌
MotionEvent 를 전부 버리고, 살아남았다 해도 winit 은 `MotionAction::Move` 를
**터치로** 읽어 스틱의 −1..1 을 터치 좌표로 넘긴다. 그래서 hero2/hero3 와 같은
결론이다 — **디스패치 체인의 맨 위에서 Java 가 잡는다**:
`MainActivity.dispatchKeyEvent`(패드/조이스틱 소스만) 와
`dispatchGenericMotionEvent`. 두 네이티브가 이벤트를 전역 큐에 넣고
`EventLoopProxy<PadWake>` 로 루프를 깨운다 — 깨우지 않으면 게임플레이 중
`plan_next_poll` 이 다음 틱 직전까지 굵게 자므로 최대 ~28 ms 늦는다.
**깨어나서 틱을 돌리지는 않는다** — `next_poll` 만 앞으로 당기고 `about_to_wait`
가 평소대로 서비스한다. 아날로그 스틱은 초당 60~100 샘플을 보내므로 샘플마다
틱을 돌리면 §35.4 가 134회로 줄여 놓은 웨이크업에 그만큼을 그냥 더한다.
트리거는 축으로만 오는 패드가 많아 `LTRIGGER`/`BRAKE` 중 큰 값을 히스테리시스로
디지털화해 합성 `BUTTON_L2/R2` 로 같은 표에 넣는다(그래서 remap 이 트리거에도
적용된다).

### 52.8 인수 기준

* `cargo test -p hl5_core --lib` **141 → 185** (게임패드 28, 배지 9, 나머지 기존)
* `cargo run -q -p hl5_probe -- --selftest` 29
* `cargo check -p hl5_android --target aarch64-linux-android`
* **`bash tools/ab_905.sh padmap` 1,241 프레임 5장면 — 전부 바이트 동일**
  (newgame 119 / ingame 543 / shop 134 / online 109 / npcmenu 336, differing 0).
  `Config::gamepad` 이 기본 꺼짐이라 이 기능의 코드 경로가 아예 없다. 켜져
  있을 때도 후크는 **항상 사양**하고 배지는 호스트 캔버스에만 그리므로, 게스트가
  보는 바이트는 어느 쪽으로도 바뀌지 않는다.
* 오프라인 픽셀: `tools_re/scripts/gamepad.txt` 가 배지 있음/없음/끔 세 장과
  걷기 다섯 장을 낸다. §52.2 의 카운터 A/B 가 이동을, `--count 0x147a50` 이
  대시(=1)를 증명한다.

**기기 미시험.** 사람 손이 필요한 항목:

1. **패드를 연결하면 배지가 뜨는가**, 그리고 빼면 사라지는가.
2. **왼쪽 스틱을 놓으면 멈추는가 — 걷는가보다 이것을 먼저 본다** (§52.9의 첫
   항목). 그 다음 **왼쪽 스틱으로 걷는가**, 그리고 걸으면서 A(공격)를 눌러도
   계속 걷는가 (§38 의 `keep_walking` 이 가짜 손가락에도 걸린다:
   `gv+0x125` 가 서 있다).
3. **면버튼 넷 + L1/R1/L2/R2 가 벨트 다섯과 콤보를 쏘는가**, 그리고 **SELECT** 가
   페이지를 넘기는가(같은 슬롯의 아이콘이 바뀐다). R3 는 지도다 — §52.4 가
   스틱 없는 패드 때문에 hero3 의 R3(페이지)를 SELECT 로 옮겼다.
4. **L3 로 저장되는가** (`저장했습니다` 시스템 메시지가 게스트 것이다).
5. **메뉴에서 스틱이 커서가 되는가** — 필드 HUD 가 없는 화면에서는 방향키다.
6. 홈으로 나갔다 오면 아무 것도 눌린 채로 남지 않는가(`pad_release`).

**기기에서만 갈리는 것 하나**: 십자키만 있는 기기(TV 리모컨, `SOURCE_DPAD` 만)는
Java 의 패드 소스 검사를 통과하지 못해 winit 경로로 그대로 간다 — 즉 §52.1 대로
제자리 걸음이 된다. 실제 패드의 십자키에는 `SOURCE_GAMEPAD` 비트가 같이 서 있어서
잡히고, 잡힌 뒤에는 키로 오든 축으로 오든 조이스틱으로 간다. 그런 기기를 살리려면
Java 의 소스 검사에 `SOURCE_DPAD` 를 더하면 되는데, 그러면 일반 키보드의 화살표도
같이 잡히므로 지금은 안 했다.

### 52.9 코드 리뷰가 잡은 여섯 (2026-09-08, 기기 시험 전)

기기에 올리기 전에 이 기능만 따로 리뷰했다. **여섯 전부 실재했고 전부 고쳤다.**
넷은 동작 버그, 둘은 코드가 주석의 약속을 안 지키는 것이었다.

#### ① 스틱에 데드존이 없었다 — **손을 떼도 안 멈춘다**

`MainActivity` 가 `getAxisValue` 를 그대로 넘기고 체인 어디에도
`MotionRange.getFlat()` 이 없었다. 그런데 **스틱을 놓아도 0 을 안 보내는 패드가
흔하다** — 중립에서 한두 할쯤 기울어진 값을 영원히 보낸다. 옛 `RELEASE = 0.12`
는 그보다 낮아서, 한 번 밀었다 놓으면 가짜 손가락이 안 올라가고 주인공이
영원히 걸었다.

문턱 둘을 짐작이 아니라 **게스트 자신의 데드존에서 유도한다**:

```
GUEST_DEAD = 10 / RADIUS = 0.2083     StateGame::TouchMoved+0x130 의 ±10 px
RELEASE    = GUEST_DEAD + 0.04 = 0.25
ENGAGE     = RELEASE     + 0.10 = 0.35
```

둘 다 `GUEST_DEAD` 위라 **내려간 손가락은 언제나 조종한다** — 손가락은 내려가
있는데 게스트는 방향을 안 잡는 구간이 사라진다. `menu_dir` 도 같은 상수를
쓴다(흔들리는 스틱은 메뉴를 혼자 스크롤시킨다).
자바 쪽 `flattened()` 는 안드로이드가 문서로 정해 둔 처리다 — `flat` 안이면 0,
밖은 `(|v|-flat)/(1-flat)` 로 되펴서 최대 편향이 그대로 1 이 되게 한다. 자기
흔들림을 신고하는 패드는 이것이 잡고, `flat == 0` 이라고 거짓말하는 패드는 위
하한이 잡는다.

> **그래서 기기 시험의 첫 항목이 바뀐다.** "스틱으로 걷는가"가 아니라
> **"놓으면 멈추는가"** 다. 걷는 것은 이 버그가 있어도 된다.

#### ② `paint_pending` 이 배지가 꺼진 동안 안 지워졌다

`badges_visible()` 관문이 `take_seen()` **앞**에 있어서, 패드가 없는 동안에는
플래그가 소비되지 않았다. `paint_pending` 은 "게스트가 방금 만든 **이 그림**에
필드 HUD 를 그렸다"라는 뜻이고 그 그림과 함께 만료돼야 하는데, 안 지우면 **옛
그림에 대한 사실**로 살아남는다: 패드 없이 필드에서 놀다가 캐릭터 시트를 열고
패드를 꽂으면, 첫 칠해진 프레임이 컨트롤이 하나도 없는 화면에 옛 필드 HUD
좌표로 배지 여덟 개를 찍었다.

고침은 순서 하나이고, 그 순서를 **시험 가능한 함수**로 뽑았다 —
`Pad::take_badge_target()` 이 플래그를 먼저 가져가고 그 다음에 배지가 보이는지
묻는다. 주석이 아니라 시험이 지킨다.

#### ③ `NONE` 이 언제나 이겼다 — 그리고 시험이 다른 코드를 재고 있었다

셸이 파일을 `bound` 와 `unbound` 두 컬렉션으로 나눠 읽고 `App::boot` 이
**bound 전부 → unbound 전부** 순서로 넣었다. 그래서 `96=NONE` 다음에 `96=OK`
라고 써도 그 버튼은 죽었다 — 파일을 읽는 사람의 기대와 정반대다. 그런데
단위시험이 재던 `gamepad::parse_map` 은 last-wins 였고 **그 함수의 프로덕션
호출자는 0개**였다. 파일 포맷의 정본이 둘로 갈려 있었고, 시험은 아무도 안 쓰는
쪽을 증명하고 있었다.

이제 `gamepad::parse_map_reporting` **하나**가 파일의 뜻을 정한다(거절한 줄을
같이 돌려주므로 셸은 로그만 찍는다), `App::pad_map` 은
`HashMap<i32, Option<PadAction>>` 하나다 — **`None` 도 바인딩**이기 때문이다.
같은 키코드는 아래 줄이 이긴다.

#### ④ `PAD_WAKE` 를 아무도 set 하지 않았다

선언과 `.get()` 두 곳뿐이고 `create_proxy()` 가 없었다. 그래서 `push_pad` 의
깨우기는 늘 no-op, `App::user_event` 는 죽은 코드였고, 패드 입력은 여전히
`plan_next_poll` 마감(게임플레이 중 ~28 ms)까지 기다렸다 — 루프를
`with_user_event` 로 바꾼 이유 자체가 없어져 있었다. 빌더 바로 옆에서 set 한다.

#### ⑤ `pos_for_action` 은 `ALL_CODES` 18개만 훑는다 — 고칠 것이 아니라 적을 것

18개 밖의 키코드로 remap 하면 **입력은 되지만** 배지가 안 따라온다. 그런데
`pos_for_code` 가 위치를 아는 것도 정확히 그 18개라, **가리킬 자리가 없어서
불가능한 것**이지 빠뜨린 것이 아니다. 그래서 코드를 늘리는 대신 경계를 네 곳에
적었다(모듈 머리말·함수·`GAMEPAD.md`·`CLAUDE.md`). `.and_then` → `find_map` 은
두 표가 나중에 갈려도 맞게 남으라는 것뿐이다.

#### ⑥ `gl.rs` 의 주석이 코드보다 강한 약속을 하고 있었다

"본체가 실제로 다시 그렸을 때만"이라고 적혀 있는데 `pad_paint_badges()` 는
무조건 불렸다. 지금 안 깨지는 것은 **첫 드로우가 `paint_pending` 을 이미
먹었기 때문**이지 불변식이 강제돼서가 아니었다. `fb_gen` 을 호출 전후로
비교해서 진짜 다시 그린 프레임에서만 부른다 — 본체의 조기반환 다섯과 생략을
바깥에서 구분할 수 있는 유일한 값이다.

#### 인수 기준 — 그리고 ⑥ 은 `ab_905.sh` 로 못 잰다

* `cargo test -p hl5_core --lib` **181 → 185**. 새 셋: 흔들리는 스틱이
  조이스틱을 놓는다 / 메뉴 키를 놓는다, 옛 HUD 목격을 나중 그림에 못 쓴다,
  같은 키코드는 아래 줄이 이긴다.
* `bash tools/ab_905.sh padfix` **1,241 프레임 5장면 전부 동일.**
* **그런데 그 A/B 는 양쪽에 같은 변경이 들어가므로 ⑥ 을 못 본다.** 그래서
  게이트를 뺀 빌드와 넣은 빌드로 `gamepad.txt` 를 각각 필름으로 찍어 직접
  비교했다 — **810 프레임 동일, 배지도 1320 대 1320.** 픽셀은 안 바뀌고
  불변식만 생겼다는 것의 증거이고, 동시에 "우연히 돌고 있었다"의 확인이다.
* `build_android.ps1` **BUILD SUCCESSFUL** — 자바와 aarch64 러스트가 실제로
  컴파일된다는 것은 이것이 증거다. 맨 `cargo check --target
  aarch64-linux-android` 는 `android-activity` 가 NDK 의 `clang++` 를 요구해서
  `cargo ndk` 환경 밖에서는 실패한다(코드 문제가 아니다).

#### 덤으로 문서 둘이 틀려 있었다

* `--count 0x8d03c` 는 **50 이 아니라 55** 다. 데드존 상수만 되돌려 다시 재
  봤더니 게스트 명령어 689,362,814 개까지 **양쪽이 완전히 같아서**, 이 세션의
  변경 탓이 아니라 원래 낡은 숫자였다. 이동 줄을 지운 대조군은 그대로 **0** 이라
  A/B 자체는 유효하다.
* 기기 시험 항목이 "R3 로 페이지"라고 적혀 있었다. §52.4 가 스틱 없는 패드
  때문에 SELECT 로 옮긴 뒤였고, 그대로 두면 시험자가 지도 버튼을 누르게 된다.

## 53. 정령 스킬창은 방향키로 고르고, `#` 이 메뉴의 정보 토글이다 (2026-09-09)

사용자 요구 둘. **둘 다 게스트가 이미 갖고 있던 기능이고, 호스트가 한 것은
"어느 패드 버튼이 그 키를 내는가"를 화면에 따라 바꾼 것뿐이다 — 게스트 후크도
패치도 0개다.**

### 53.1 정령 스킬창 — 게스트가 이미 −1..−4 를 기다리고 있다

`HERO::UseSpiritSkill` 로 가는 길은 하나뿐이고 네 클래스가 똑같다
(`WARRIOR::KeyPress+0x28`, 0x147490):

```
r8 = NumKeyToNaviKey(key)
if (IsSpiritSkillMenu()) {          ; ldrsb [this+0x1FB6] > 0   (0x8921C)
    if (r8 == -16) { ApplyTimestop(0); [this+0x1FB6] = 0; return }   ; 취소
    if (r8 <  -16) return
    if ((unsigned)(r8 + 4) > 3) return          ; -4..-1 만 통과
    SetSpiritSkillIndex(r8)                     ; 0x94AC8
}
```

`SetSpiritSkillIndex(navi)` 는 `navi+4` 짜리 4항 점프테이블이고 **case 0 이
`pc+8`** 이라(§31 의 함정) 짝은 이렇게 붙는다:

| navi | 방향 | 읽는 슬롯 |
|---|---|---|
| −1 | UP | `HERO+0x1792` |
| −2 | DOWN | `HERO+0x1793` |
| −3 | LEFT | `HERO+0x1794` |
| −4 | RIGHT | `HERO+0x1795` |

읽은 값을 `HERO+0x1FB7` 에 넣고 **부호 있는 값이 양수면 그 자리에서
`UseSpiritSkill` 을 부른다** — 커서가 아니라 시전이다. 성공하면
`HERO+0x1FB6` 을 0 으로 만들어 창을 닫는다. 빈 슬롯은 무반응이고 창은 남는다.

**그래서 패드로 안 되던 이유가 정확히 둘이다.** ① 필드에서 우리 스틱·십자키는
방향키가 아니라 가짜 손가락이다(§52.1). ② 페이스 버튼 넷이 내는 것은
OK(−5)·CLEAR(−16)·벨트 키인데, 위 코드가 **−1..−4 와 −16 말고는 전부 조기
반환**한다. 즉 취소만 되고 선택이 안 된다 — 사용자가 보고한 그대로다.

**고침**: `HERO+0x1FB6` 이 양수인 동안만 `Pad::button` 이 페이스 셋과 R1 을
방향키로 바꾸고, **그 방향은 진짜 키로 보낸다**(`as_key`) — 이 게임에서 WIPI
방향키가 무언가를 하는 유일한 자리다. 같은 조건에서 `Pad::steer` 는 필드
HUD 가 그려져 있어도 키 경로를 타므로 스틱·십자키로도 네 방향이 다 된다.

**동(오른쪽)이 `B` 가 아니라 R1 인 것은 사용자 지시다** — 취소가 이미 B 에서
되고 있고, 이 창은 시간을 멈추므로 나갈 길을 뺏으면 안 된다.

두 가지가 저절로 맞는다: `held` 가 **누를 때 정한 동작**을 담으므로 창이
손가락 밑에서 닫혀도(=시전 성공) 릴리스는 자기 프레스의 키로 나간다. 그리고
`release_all` 은 방향을 건너뛰는데(그것들은 조종이지 전송이 아니다) 이쪽은
진짜로 보낸 것이라 `keyed` 집합이 예외로 잡는다.

### 53.2 메뉴의 정보 토글은 `#` 이고, `*` 는 비어 있다

`CommonUi::TogleAutoInfo`(0x775AC, 원본 오타)가 `gv+0x198A` 를 0 ↔ 100 으로
뒤집고 `gv+0x198B` 를 지운다. 화면의 `#` 표시는 `CommonUi::DrawSharpInfo`
(0x77D20) → `DrawTouchInfoToggle` 이고 **34곳**에서 그려진다.

그 토글을 부르는 **키 핸들러가 22개**다 — 가방·특성·스킬·장비·정령·연계기·
도감·조합·제련·판매·구매·스킬구매·창고·우편함·캐시샵·익스퍼트상점. 키는 전부
`'#'`(0x23) 이고, 우리 패드는 이미 SELECT 에 그것을 갖고 있다.

**`'*'`(0x2a)는 상태 키 핸들러 55개 중 한 곳에서만 쓰인다** —
`StateMainMenu::KeyStart`(0x122F38, 슬롯선택에서 차 있는 슬롯 = 캐릭터 삭제).
가방의 두 점프테이블에서도 `'*'` 칸(0x1094FC / 0x109634)이 기본 분기다.

**고침**: 필드 HUD 가 없을 때, **퀵슬롯 4 에 묶인 컨트롤**이 `'#'` 을 낸다.
키코드가 아니라 *동작*으로 판정하므로 remap 하면 정보 키도 따라가고, 배지는
같은 역룩업이라 계속 맞는다. 잃는 것은 슬롯선택 화면의 캐릭터 삭제 단축키
하나이고 그것은 탭으로 그대로 된다.

### 53.3 오프라인으로 증명한 것과 못 한 것

* **증명됨(단위시험 8개, 185 → 193):** 창이 떠 있으면 Y/A/X/R1 이 −1/−2/−3/−4
  를 내고 B 는 CLEAR 그대로, 창이 닫히면 즉시 원래 동작, 창이 프레스 도중에
  닫혀도 릴리스가 대칭, `release_all` 이 정령 방향을 놓아 준다, 필드에서는
  R1 이 `'*'` 이고 메뉴에서는 `'#'`, 그리고 퀵슬롯 4 를 remap 하면 정보 키가
  같이 옮겨간다.
* **못 함:** 게스트가 실제로 설명 상자를 켜는 것을 오프라인 재생으로 못 봤다.
  이유는 후크가 아니라 **장면**이다 — `--pad` 재생으로 닿을 수 있는 메뉴가
  전부 "설명할 것이 없는" 상태였다. 도감(`npc-menu 7`)은
  `KeyCollection+0x86c` 가 `GetBagItemTotalBunchCount() <= 0` 이면 토글을
  건너뛰는데 이 세이브의 도감이 **0/386** 이고, 인게임 메뉴는 재생마다
  열리지 않았다. `--count 0x10FDBC`(그 관문 자체)도 0 이라 커서가 바깥 칸에
  있었다는 뜻이다.
* **그래서 기기 시험이 이 항목의 인수다.** 그리고 그 절반은 **지금 깔린
  vc53 에서 이미 확인할 수 있다** — 메뉴에서 SELECT 를 누르면 같은 `'#'` 이라
  설명이 켜져야 한다. 켜지면 R1 도 켜진다.

## 54. `+랜덤옵션` 셋째 줄 — 세 번째 문, 그리고 문마다 막지 않는 그물 (2026-09-13)

사용자 보고: **아이템 옵션 셋째 줄이 실제 랜덤옵션이 아니라 `+랜덤옵션`
문구 그대로인 드랍이 있다.** 전에 고친 것이 되살아난 것인지 물었다 — 아니다.
같은 구멍의 **세 번째 문**이고, 2026-09-08 의 `lostdrops.rs` 가 연 것이다.
그리고 이번에는 문마다 막는 대신 **그물**을 쳤다.

### 54.1 화면의 규칙 — 줄 수는 등급이 정하고, 채움은 안 본다

`CommonUi::DrawItemDiscription`(0x7B350) 의 옵션 줄:

```
0x7b3c4  ldrbeq r5,[sl,#0x15c]     ; 등급 (부호 확장)
0x7b3d4  cmp r5,#1 / beq -> fp=1
0x7b3dc  cmp r5,#2 / beq -> fp=2
0x7b3e4  movgt fp,#3 / movle fp,#0 ; 3 이상은 전부 셋 — 전설·세트 포함
0x7b7a4  ldrb r2,[r7,#0x15f] ... asr #24 / cmp #0 / ble -> 0x7b840
0x7b840  GetCommonText(0x11f=287)   ; "+랜덤옵션"
0x7b83c  bgt 0x7b7d8               ; kind > 0 이면 옵션 이름
```

즉 **등급 1 → 1줄, 2 → 2줄, 3 이상 → 3줄**이고 그 안의 칸마다 종류 바이트가
`<= 0` 이면 문구다. `shop::option_lines` / `shop::empty_option_lines` 가 이
규칙의 전사이고 이 절의 모든 판정이 그것 하나를 쓴다.

옵션 칸의 자리: `SetItemOption(this, item, slot)`(0xA0FF8) 이 `item + slot`
을 `sl` 로 잡고 `strb [sl,#0x15f]`(종류) / `strb [sl,#0x162]`(값) 을 쓴다 —
종류 셋이 `+0x15f..+0x161`, 값 셋이 `+0x162..+0x164`. 밴드는 **slot 그대로**
(`r8 = slot*2` 로 `option.dat` 행의 세 누적분포 중 하나를 고른다).

### 54.2 출하 표의 모양 — 전수 조사

`item_00..10` 789행의 등급 × 옵션 종류 바이트:

| 등급 | 칸 0 | 칸 1 | 칸 2 | 행 수 |
|---|---|---|---|---|
| 0 노멀 | — | — | — | 362 |
| 2 에픽 (장신구) | 있음 | 있음 | `0xFF` | 17 |
| 3 영웅 | 있음 | 있음 | **`0xFF`** | 242 |
| 3 영웅 | 있음 | 있음 | 있음 | 10 (뮤탈 계열 스태프 ·장신구 6) |
| 4 전설 | 있음 | 있음 | 있음 | 101 |
| 5..18 세트 | 있음 | 있음 | **`0xFF`** | 56 |

**영웅과 세트의 셋째 칸은 표가 비워 두고 공장이 채우는 자리다.**
`MakeItemOption`(0xA10E8) 이 그 공장이다 — 등급 1 → 칸 0, 2 → 칸 0·1,
3·≥5 → **칸 2**, 0·4 → 없음 — 그리고 호출자는 `NewItemEx` · `ApplyNormalMix` ·
`ApplySpecialMix` 셋뿐이다. `NewItem`(상점·시작장비) 도 `NewDropItem` 도
`NewBagNetItem` 도 부르지 않는다.

### 54.3 원본 드랍은 왜 멀쩡한가 — 드랍 레코드가 짝을 실어 보낸다

`droptable.dat` 레코드 13바이트 `[u16 reclen=11][group][kA][vA][kB][vB]
[kind4][idx4][kind5][idx5][kind6][idx6]` 를 `Monster::SetDropItem` 이
밴드마다 다르게 `NewDropItem(x, y, kind, index, grade, k0,v0, k1,v1, k2,v2)`
로 넘긴다 (0xBCA60~; 메모리 버퍼는 `[u16 count]` 를 단 파일 그대로라
`13*i+5..+14` 가 body[3..12] 다):

| 밴드 | kind/index | grade | 옵션 오버라이드 |
|---|---|---|---|
| 1 (0xBCCA8) | `Rand(0,9)` / `Monster+0x272` | 0 | 없음 |
| 2 (0xBCCF8) | 같음 | 1 | 칸 0 = (kA, vA×pct) |
| 3 (0xBCDA0) | 같음 | 2 | 칸 0 = (kA, vA×pct), 칸 1 = (kB, vB×pct) |
| **4** (0xBCB54) | body[5..6] | −1 (표) | **칸 2 = (kB, vB×pct)** |
| 5 (0xBCD60) | body[7..8] | −1 | 없음 — 전부 −1 |
| **6** (0xBCE30) | body[9..10] | −1 | **칸 2 = (kB, vB×pct)** |

`pct` 는 `(s8)Monster+0x274` 이고 `v * (100 + pct) / 100`(부호 있는 /100,
`smull 0x51EB851F`), 그 뒤 `and #0xff`. `NewDropItem` 은 인자가 `>= 0` 일 때만
`+0x15c` / `+0x15f..+0x164` 에 덮어쓴다(0xA7A64~0xA7AD8).

252 레코드 × 세 밴드를 표에 대조하면(단위시험
`shipped_drop_bands_never_leave_a_displayed_slot_empty`): 밴드 4 의 대상은
영웅 235 + 전설 2 + 장신구 영웅 1 + 세트 2 이고 전부 레코드의 kB 가 셋째 칸을
채우며, 밴드 5 의 대상 144 는 에픽 17(두 줄, 둘 다 있음)·영웅 15(셋 다 있음)·
전설 112 라 채울 것이 없고, 밴드 6 은 세트 148 에 kB 다. **빈 줄이 나오는
조합이 0 이다.** 그리고 레코드 252개의 kA·kB 는 전부 > 0 이다. 원본은 이
규칙을 표와 레코드 사이에서 정확히 지킨다 — 문구가 나올 길이 없다.

### 54.4 세 번째 문 — `lostdrops` 의 밴드 4 는 짝 없이 넘겼다

`lostdrops::extra_lost_rolls` 는 갇힌 칸을 `NewDropItem(…, kind, index,
0xFF ×7)` 로 떨어뜨렸다. **밴드 5 라면 그것이 게스트와 같지만, 밴드 4 는
아니다** — 게스트는 그 레코드의 (kB, vB×pct) 를 칸 2 에 넣는다.
`LOST_SLOTS` 의 밴드 4 항목 15 (다크코트·다크워커·스톰브링거·데몬브레이커·
속사의피마펭·오웬스피어·소울랜스·빙해의갑옷·빙해의장화·사티라스 셋·
리트무쇠갑옷·무극위상코트·무극위상장화) 는 **전부 영웅이고 셋째 칸이
`0xFF`** 이다. 그래서 그 열다섯은 떨어질 때마다 셋째 줄이 `+랜덤옵션` 이었다.
밴드 5 의 열 항목(장신구 에픽/영웅)은 자기 줄을 다 갖고 있어 멀쩡했다.

문 셋의 계보: ① 유물함 와이어(`NewBagNetItem`, §40·§42) → 호스트가 와이어에
굴려 싣는다. ② 헬 세트 드랍(§49) → `MakeSocket` 후크가 세트 등급에만
`MakeItemOption`. ③ 이것. 셋 다 "옵션 공장을 안 지나는 문에 영웅/세트 행을
넣었다"이고, ②의 후크가 **세트 등급으로 좁혀져 있어서** ③을 못 잡았다.

### 54.5 고침 둘 — 짝은 레코드대로, 그물은 화면 규칙대로

1. **`lostdrops` 가 게스트와 같은 인자를 넘긴다.** 굴릴 때 게스트 자신이
   읽는 표(`[gv+0x1444]` → `ItemTable+0x214`)에서 그 레코드의 body[3..4] 를
   읽고, 밴드 4·6 이면 `(kB, scaled_option_value(vB, pct))` 를 칸 2 로,
   밴드 5 면 −1 을 넘긴다. 스톰브링거는 이제 원본 밴드 4 가 줬을 바로 그
   옵션(종류 35, 값 2×pct)을 달고 떨어진다.
2. **`MakeSocket` 후크가 그물이 됐다** (`legendaries::fill_empty_options_on_drop`).
   `NewDropItem+0x48c` 복귀주소에서만, 아이템의 `empty_option_lines` 가 비어
   있지 않으면 원본 `MakeSocket` 뒤에 **빈 칸마다 `SetItemOption(item, slot)`**
   을 부른다 — `MakeItemOption` 이 하는 바로 그 호출을 칸 단위로. 등급 조건이
   없다. 54.3 의 전수 조사가 "출하 드랍은 절대 안 걸린다"를 보장하므로 원본
   드랍의 바이트는 하나도 안 바뀌고, 앞으로 어떤 호스트 문이 짝을 빼먹어도
   화면에 문구가 닿기 전에 잡힌다. 유물함의 `shop::fill_options` 도 같은
   규칙으로 넓혔다 — 공장이 채우는 칸을 굴린 뒤 화면이 아직 비었다고 보는 칸을
   한 번 더 굴린다(출하 행에서는 두 집합이 같다).

**첫 시도의 함정:** `NewDropItem` 의 스택 인자는 여덟(index, grade, k0, v0,
k1, v1, k2, v2)이다. 짝을 붙이면서 `0xFF` 를 하나 덜 지우면 k2 가 v2 자리로
가고 v2 는 떨어진다 — 재생에서 "짝을 실었는데도 그물이 돌고 칸 2 가 엉뚱한
종류"로 바로 드러났다(`drop 0 17 35 2` 가 31 을 냈다).

### 54.6 증명

* **재생 `tools_re/scripts/dropoptions.txt`** — 프로브 명령 `drop <kind>
  <index> [k2 v2]` 가 게스트 자신의 `NewDropItem` 을 주인공 발밑에 부르고
  결과 아이템의 등급·옵션 여섯 바이트를 찍는다(복귀주소가 실제 드랍과 같아
  그물이 같은 자리에서 본다). 여덟 드랍 전부 `empty=[]`:
  스톰브링거 맨몸 → 칸 2 굴림 / 짝 (35,2) 실음 → 그대로 35,2 /
  사티라스코트 맨몸·짝 (15,6) / 전설 궁니르(80) 그대로 / 에픽 쟈칼의발톱 두
  줄 그대로 / 세트 지크문드 굴림 / 다크코트 짝 (15,2) 그대로.
  `--count _ZN9ItemTable13SetItemOptionEP8ItemInfoa` = **3** — 그물이 돈 건
  맨몸 둘과 세트 하나뿐이다.
* **단위시험 193 → 199**: 화면 규칙, 전설의 구멍은 채우고 이름 있는 전설은
  안 건드림, 값 스케일(2·6·14 × 108% = 2·6·15), 밴드 4·6 만 짝을 실음, 갇힌
  밴드 4 행 15 가 전부 영웅+`0xFF` 이고 레코드에 짝이 있음, 출하 드랍 밴드
  전수(532 결과)에 빈 줄 0.
* **1,241 프레임 HEAD 대비** — 아래 커밋 메시지.
* **기기 미시험.** 이미 가방에 있는 문구짜리 열다섯은 안 고쳐진다 — 지급
  표와 같은 이유(세이브의 `+0x15f..` 를 손대지 않는다). 다시 떨어뜨려야 한다.

## 55. 메뉴·저장 버튼을 누르면 잠깐 멈춘다 — 블러와 DES 가 그 한 프레임의 전부였다 (2026-09-25)

사용자 보고: **S9 에서 메뉴 버튼이나 저장 버튼 등을 누르면 0.5초쯤 멈췄다가
메뉴가 열리거나 저장이 된다.** 원인은 둘이고, 둘 다 **버튼 한 번에 한 번,
한 프레임 안에서** 도는 게스트 함수다. 그리고 둘 다 형제 크로노스 소드가 이미
같은 이유로 이식했던 함수의 이 바이너리 판이다(`cs_core/src/des.rs`,
`host_box_blur`).

### 55.1 전체 프로파일은 이것을 못 본다 — 프레임 비용을 찍는 도구

§43.4 의 프로파일(2,718 프레임 전체)에서 둘은 **1.8% (`desRound`)와 1.5%
(`BoxBlur565`)** 였다. 틀린 숫자가 아니라 평균이라서 안 보인 것이다 —
멈춤은 한 프레임이다. 프로브에 둘을 더했다:

* `--frame-cost <path>` — 프레임마다 `번호 게스트명령어 호스트µs` 한 줄, 끝에
  `costliest frame` 요약. 스파이크의 **프레임 번호**가 곧 다음 명령의 인수다.
* `--profile-frames <a>..<b>` — 그 프레임 창 안에서만 표본을 뜬다
  (`--profile` 을 함의). 부팅·재생 앞부분이 보고서에 아예 안 들어간다.

`ingame.txt` 에 메뉴 버튼(380,45)·지도·두 번째 저장을 붙인 임시 재생
(400×240, 게임플레이 프레임 ≈ 0.3 M 명령어):

| 프레임 | 무엇 | 전 | 후 |
|---|---|---|---|
| 794 | 타이틀 탭 → 주 메뉴 (`KeyTitle+0x94` 의 블러) | **19.5 M** | 0.23 M |
| 3718 | 저장 버튼 | **51.7 M** | 0.52 M |
| 3924 | 인게임 메뉴 버튼 | **20.6 M** | 0.62 M |
| 4172 | 두 번째 저장 | **51.7 M** | 0.52 M |

그 프레임만 프로파일하면 한 줄로 끝난다:

```
저장 (3718):  87.6% desRound  5.4% startDes  2.9% binary2char
              1.5% swap32bit  1.4% char2binary              = 98.8%
메뉴 (3924):  73.4% Graphic::BoxBlur565  23.6% __divsi3      = 97.0%
```

**출하 캔버스(585×270)에서는 블러가 더 크다** — 픽셀 수에 비례해서
`newgame.txt --screen 2340x1080 --no-patch` 의 타이틀 탭 프레임에서
`BoxBlur565 + __divsi3` 만 **31.6 M** 이다. 저장은 캔버스와 무관하다.
S9 가 초당 ~34 M 명령어를 돌리므로(§43.2) 메뉴 ≈ 0.9 초, 저장 ≈ 1.5 초,
발열 뒤에는 그 1.7배 — 사용자의 "0.5초?" 는 이것의 체감값이다.

### 55.2 저장 = `startDes` — 크로노스와 같은 DES, 다른 드라이버

사슬: `Battle::KeyPressed` −10 → `HERO::SaveAll` → `Save*Data` →
`StaticUtil::SaveEncryptFile` → `MX_desEncrypt` → `startDes`. 한 번의 저장이
레코드 일곱을 암호화한다 — `SL_0.sav` 11,679 B, `H_0` 639, `M` 386, `B_0` 291,
`EX` 72, `ET` 11, `OP` 8. 전부 `<files>/kickass`(15,396 B) 한 파일에 들어간다.
시스템 탭의 저장하기도, 슬롯 목록의 `LoadDecryptFile` 도, `LoadResDecrypt` 도
같은 드라이버다 — `startDes` 의 호출자는 `MX_desEncrypt`(0x395F8)와
`MX_desDecrypt`(0x39540) 둘뿐이다.

크로노스 대비 전수 대조(`.symtab` 이름으로 두 바이너리의 바이트 비교):

| | 결과 |
|---|---|
| `MAT_IP` `MAT_IP_1` `MAT_EXP` `MAT_PERMU` `MAT_SBOX` `KEY4ENCRYPT` | **바이트 동일** — 오염된 S1 행 3 열 10 (`2`, FIPS 는 `3`) 까지 |
| `char2binary` `binary2char` `swap32bit` | 바이트 동일 |
| `desRound` | 명령어 동일, PLT·GOT 오프셋만 다르고 같은 네 표로 풀린다 |
| `startDes` | **다르다** — 772 B (크로노스 760). **모든 경로에서 `blocks*8` 을 돌려준다** (`[sp,#0x20]`, 0x39268 / 0x39328). 래퍼 둘은 버리지만 호스트도 돌려준다 |
| `MX_desEncrypt` / `MX_desDecrypt` | 184 B — 크로노스보다 한 줄 많다: **호출마다 `MAT_SBOX[0x3a] = 2` 를 다시 쓴다** (0x39624 / 0x3956C). 출하값도 2 라 효과는 없지만, 표가 상수가 아니라 상태라는 증거다 |

그래서 `des.rs` 는 크로노스의 구조를 따르되 **이 바이너리의 명령어에서
옮겨 적었다**: 표는 호출마다 게스트 메모리에서 이름으로 읽는다(래퍼가
`KEY4REAL` 을 제자리에서 뒤집고 `MAT_SBOX[0x3a]` 를 쓴다). 순열 색인이
`1..=n` 밖이거나 키 바이트가 0/1 이 아니면 사양한다 — 게스트는 그 경우
스택의 이웃 지역변수를 읽을 것이고 그것은 옮길 수 없다(출하 표에서는 안
일어난다: `MX_desInit` 이 `char2binary` 출력을 `strb` 한다).

**그리고 믿기 전에 증명한다.** `Vm::des_selftest` 가 부팅 끝(=`MX_desInit`
뒤)에 방향마다 128 바이트를 게스트 자신의 `startDes` 와 호스트에 같이
돌려 비교하고, 다르면 그 방향을 **영원히** 게스트에 남긴다(로그에
`DISAGREES`). 키 스케줄이 아직 없으면(`KEY4ENCRYPT` = 0xff) 첫 실제 호출에서
대신 한다. 참조 호출이 폴트해도 프레임을 죽이지 않고 사양한다.
기기 로그의 기대값: `des: host decrypt verified …` / `des: host encrypt
verified …` 두 줄.

### 55.3 메뉴 = `Graphic::BoxBlur565` — 2×2 박스인데 픽셀마다 나눗셈 셋

호출자 16곳(`StateInGameMenu` 생성자·`Process`·소멸자, `StateInExpertMenu`,
`StateGame::KeyExpert`, `StateMainMenu::KeyTitle`/`Process`)이 **전부**
`BoxBlur(GetFB(), 2)` 이고, `Graphic::BoxBlur`(0x557A8)가 `this[0x5c] == 1`
이면 여기로 꼬리분기한다(그래서 `lr` 이 원래 호출자의 것이다).

* 출력 `(x, y)` 의 박스는 열 `x-1 ..= x+k-2` × 행 `y-1 ..= y+k-2` — **왼쪽
  위로 한 칸 밀린** 박스다. 표면 밖 표본은 건너뛰고, 채널 합을 **살아남은
  표본 수로** 나눈다(`__divsi3` 셋, 절삭). k = 2 면 안쪽은 4, 첫 행·열은 2,
  (0,0) 은 1.
* 소스는 `a1`(= `this[0]`), 목적지는 **`this[8]`** — `onStartApp+0x3f4` 가
  캔버스 크기로 만드는 오프스크린 버퍼(585×270 캔버스에서 `(0x249, 0x10e)`).
  둘 다 **소스 폭으로 빽빽하게** 읽고 쓴다. 되돌려 복사는 없다 — 메뉴가 그
  버퍼를 배경으로 그린다. 크로노스는 스크래치에 쓰고 되돌려 복사했으니 여기가
  다르다.
* k ≤ 1 은 표본 수 0 으로 나누게 되므로 사양, 두 버퍼가 겹쳐도 사양(제자리면
  뒤의 박스가 앞의 출력을 본다). 실제로는 둘 다 안 일어난다.

호스트는 같은 박스·같은 가변 제수·같은 절삭·같은 565 합성을 열 합을
행으로 내려 나르는 미끄럼 창으로 계산한다 — 출력당 O(1). 단위시험
`box_blur_matches_the_guest_loop` 가 게스트 루프의 문자 그대로의 전사와
7 크기 × k 2..6 에서 비교한다.

### 55.4 증명

* **`bash tools/ab_905.sh blurdes`**: 5장면 **1,241 프레임 전부 동일**, 그리고
  두 쪽의 storage 디렉터리 **8 파일 전부 바이트 동일** — `kickass`(암호화된
  세이브 컨테이너) 포함. 즉 호스트 DES 가 쓴 세이브가 게스트 DES 가 쓴 것과
  같다. 장면마다 `des: host … verified` 두 줄, 사양 2회(= 자가시험의 참조
  호출 둘).
* **메뉴·저장 임시 재생** 2,173 프레임(`--film-every 2`) 동일, storage 동일.
* **출하 캔버스** `--screen 2340x1080` 5장면 patched vs `--no-patch`: **1,241
  프레임 전부 동일, storage 8 파일 동일.** 블러가 585 폭에서 도는 것은 여기서만
  재진다(재생 좌표가 400×240 기준이라 장면 내용은 기본 A/B 와 다르다).
* **사양 0회**: 패치된 심볼에 `--count` 를 걸면 사양된 진입만 센다 — 메뉴·저장
  재생에서 `BoxBlur565` 0, `startDes` 2(= 자가시험의 참조 호출), 그리고
  `host DES 18 startDes call(s) served` = `MX_desEncrypt` 14 + `MX_desDecrypt` 4.
  실제 호출은 전부 호스트가 받았다.
* **인게임 메뉴 탭 아홉 + 캐시샵**을 전부 열어 봤다: 이식 뒤 어느 프레임도
  0.71 M 명령어를 안 넘는다.
* **덤으로 부팅이 짧아졌다** — `--boot` 만: 36.5 M → **9.1 M** 명령어,
  `--count desRound` 924 → 32 (남은 32 는 자가시험의 참조 호출 두 번 × 16
  블록). §31 이 "부팅 항목"으로 미뤄 둔 `LoadResDecrypt` 가 같은 드라이버라서다.
  S9 에서 실행마다 ~0.8 초.
* 단위시험 +4 (블러 1, DES 3).
* **S9 실기기 (SM-G960N, vc58, 555×270)**: 부팅 로그에 `des: host decrypt verified`
  와 `des: host encrypt verified` — 게스트 자신의 DES 와 기기에서 대조해 통과했다.
  이식 목록 끝에 `_ZN7Graphic10BoxBlur565Eii, startDes`, 게스트 부팅
  `9527686` 명령어. **버튼을 눌러 본 체감은 아직이다** — 그 설치본의 세이브로
  사람이 해야 한다(벤치 빌드는 세이브가 따로다).

### 55.5 남은 스파이크 — 지도 버튼과 장면 로드는 VFS 목록 걷기다

이식 뒤 가장 비싼 프레임은 장면 로드(4.25 M)와 **지도 버튼(1.74 M ≈ S9
50 ms)** 이고, 그 30~50% 가 `MIDASKernelManager::getAssetSizeFromVFS`
(0x151968)다 — 리소스를 열 때마다 `dup`+`fdopen` 후 아카이브 2,189 엔트리를
`fread`×2 + `fseek` 로 **처음부터** 걷는다(HLE 왕복이 엔트리당 셋). 해시 →
오프셋 색인이면 O(1) 이지만, 이식은 그 `dup`/`fdopen`/`fseek`/`fclose` 의
부수효과를 프로브와 안드로이드의 파일 계층 양쪽에서 재현해야 한다.
반초짜리 멈춤이 아니라서 이번에는 하지 않았다.

## 56. 해상도에 `세로` 를 더하고, 값을 퍼센트로 (2026-10-14)

사용자 요구 둘: **A/B/C 를 `FULL / 90% / 80%` 로 보이게**, 그리고 사용자가
따로 받아 쓰던 `dist/영웅서기5 vc58 해상도추가.apk` 의 **세로 모드를 같은
행의 넷째 값으로.** 그 APK 는 우리 vc58 을 바이너리로 고친 것이고, 패키지가
`kr.heroeslore.five.vc58dual`, 서명이 `PORTRAIT`, 시작 화면이
`kr.heroeslore.dual.Launcher`("세로 · 한손 키패드" / "원본 · 가로 터치")였다.

### 56.1 그 APK 가 바꾼 것 — 셋

| 자리 | 바뀐 것 |
|---|---|
| `lib/arm64-v8a/libmain.so` 77 B | (1) `widened_canvas_for` 인라인 자리(0x8C990..)를 `w=240, h=320, full=240` 상수로 (2) `Vm::fit_screen` 의 long/short `csel` 두 개를 `mov` 로 (0x12C970) (3) 게스트 파일 이름 `libHeroesLore5.so` 그대로 — 원본 모드용 사본 `libmainoriginal.so` 는 `libHeroesOrig5.so` 를 연다 |
| `assets/libHeroesLore5.so` 340 B = **111 워드** | 240폭 터치 UI 재배치. 원본은 `libHeroesOrig5.so` 로 보존했고 **우리 `payload/` 와 바이트 동일**하다 |
| `classes2.dex` / `classes3.dex` | `PhoneLayout`(24키 키패드, 게임 뷰 위 3:4 + 아래 키), `Launcher`, `Mode`(방향·라이브러리 선택, `:game` 프로세스를 끝낼 때 kill) — `GameActivity.onCreateSurfaceView` 에 `Mode.install` 한 줄을 꽂았다 |

`fit_screen` 고침이 없으면 세로 캔버스에서 배율이 0.75 가 된다 — `(long,
short) = (320, 240)` 으로 240×320 을 재기 때문이다. 터치가 3분의 1 바깥으로
떨어진다.

### 56.2 111 워드가 하는 일 — 게스트에 원래 240폭 배치가 있다

`StateGame::SetTouchKeypad`(0xE1194) 와 `GMenu::DrawSkillBelt`(0x8361C) 가
둘 다 `cmp GetWidth(),#0xf0` 로 **240 이하일 때 쓰는 버튼 좌표표**를 따로
가진다(2011년 QVGA 폰의 오른쪽 호 배치). 같은 24 halfword 표가 `.rodata` 에
**두 벌** 있다 — `DrawSkillBelt` 는 `GOT + 리터럴 = 0x157D1C` 에서 `+0x78`
(=0x157D94)을, `SetTouchKeypad` 는 0x159C38 을 읽는다. 모더는 **그리는 쪽만**
고쳤다: 벨트 여섯 칸이 `W-232 .. W-72` 에 `H-46` 한 줄로 내려오고, 터치 판정
사본은 원래 호 그대로다(키패드가 대신 누르므로 문제 없다).

나머지: `DrawTouchUi` 가 조이스틱 판·터치취소 띠를 안 그리고(`bl` 일곱 개가
`nop`), 터치취소 코드 자리(`+0x1a8`)에 `BFont::DrawText` 루프를 넣어 벨트
아래 `1 3 7 9 * 0` 을 쓴다. 상단 바(map/shop/save) 판정을 건너뛰고(`+0x7bc`)
하단 모서리(`y 260..272`, `x<30` 지도 −6 / `x>=210` 상점 −7)와 (109,40) 3×3
저장(−10) 점을 `KeyPress`(vptr+0x1c)로 답한다. `mainKeyPressed+0x324` 는
`TouchPressed+0x844` 의 네 워드 스텁을 지나고, 미니맵이 `-40 → -4` 로 내려온다.
워드 단위 표는 `hl5_core/src/portrait.rs` 의 `RUNS` 가 정본이다.

### 56.3 여기서는 — 같은 결과, 다른 문

* **캔버스**: `ScreenMode::Portrait` → `widened_canvas_for` 가 표면과 무관하게
  `portrait::CANVAS = (240, 320)`. 셸은 `full_canvas_w = canvas_w` 라 가로
  레터박스가 없다.
* **`fit_screen`**: 표면을 *캔버스와 같은 방향으로* 읽는다
  (`(w>=h) == (ww>=wh)`). 가로 캔버스에서는 예전 식과 같다(시험이 S9 의
  400×240 → 4.5/210/0 까지 박아 둔다).
* **게스트 워드**: `.so` 를 두 벌 싣는 대신 `portrait::apply` 가 **부팅 첫
  명령어 전, 어떤 후크보다 먼저** 같은 111 워드를 게스트 메모리에 쓴다. 쓰기 전
  111 개 원래 값을 전부 대조하고 하나라도 다르면 **하나도 안 쓴다.** 인터프리터에
  디코드 캐시가 없고, 패치 범위 안에 기존 후크가 하나도 없다(전수 grep). 이
  프로젝트에서 호스트가 게스트 코드를 쓰는 **유일한** 자리이고, 세로가 아니면
  한 바이트도 안 닿는다.
* **키패드**: 모더의 `PhoneLayout` 은 Java 에서 게임패드 키코드와 가짜
  MotionEvent 를 만들어 `dispatchKeyEvent`/`dispatchTouchEvent` 로 흘렸다.
  여기서는 `PhoneKeypad.java` 가 **인덱스만** `nativeKeypad(i, down)` 으로
  넘기고, 그것이 `KEYPAD_CODE_BASE(0x10000) + i` 라는 키코드 밖 구간의 패드
  버튼이 되어 `Pad::action` 이 `hl5_gamepad.txt` **보다 먼저** 푼다 — 패드를
  remap 해도 키패드는 안 바뀐다. 나머지는 전부 §52 의 패드 계층이다: 같은 틱
  두 번째 키를 안 잃는 큐, 필드에서는 게임 자신의 조이스틱(65, H−65 = 세로에서
  정확히 모더의 `ANCHOR (65,255)`), 메뉴에서는 방향키, 게스트 더블탭 대시.
  `2 4 6 8` 과 화살표가 같은 방향이고, `1 3 9 * 0` 이 벨트, `7` 콤보, `#`
  페이지(메뉴에서는 정보 토글), `5`/`OK` 확인, `L`/`☎` 지도(☎ 는 §56.7 에서 `⚙` 가 됐다), `R`/`CLR` 취소,
  `저장` −10. 키패드의 `*` 는 메뉴에서도 `*` 다(패드의 R1 은 메뉴에서 `#`
  으로 바뀌지만 키패드에는 따로 `#` 가 있다).
* **Java**: `MainActivity` 가 `super.onCreate` **전에**
  `getFilesDir()/files/hl5_resolution.txt` 를 읽어 `P` 면 세로를 요청하고,
  `onCreateSurfaceView` 뒤에 키패드를 붙인다. 게임 뷰는 **정수 배율이 2 이상
  나오면 정수**(S25/S9: 960×1280, 4배)로, 아니면 들어가는 최대 3:4 로 잡는다
  — 모더 빌드는 1070×1427(4.46배)이었다. 위 여백은 카메라 컷아웃의
  `safeInsetTop` 이다(모더: 폭의 10.2% 고정). 키패드는 게임 뷰 **아래만**
  차지한다 — 모더 빌드는 키패드 뷰가 게임 뷰 아래 110 px 와 겹쳐 그 띠의 탭을
  삼켰다(게스트 y 295..320, 벨트 줄).

### 56.4 행과 파일

값 상자는 `FULL / 90% / 80% / 세로`(31 px 상자에 `세로` 22 px), 재시작 문구는
`해상도를 90%로 바꿨습니다.` 꼴이다 — `FULL로`·`세로로` 가 첫 줄 27 바이트
천장에 정확히 닿는다. **파일은 글자 그대로 `A/B/C` + `P`** — 이미 `B` 를 고른
설치본이 다른 값으로 뜨면 안 되기 때문이다. `parse` 는 `90`·`90%`·`portrait`
도 받는다. 세로 ↔ 가로 전환은 다른 모드처럼 재시작 팝업의 `예` 로 끝난다.

### 56.5 인수

* `--screen-mode p` 는 `--screen` 이 필요 없다. 부팅 로그에
  `portrait: 240x320 canvas, 111 guest word(s) relaid` .
* `tools_re/scripts/portrait.txt` — 필드까지 키로만 가서(컷신 SKIP 은 `#`)
  하단 HUD 그림, 그리고 `pad key:6`/`pad key:2` 로 걷기:
  `--count 0x8d03c` 가 **28**, 두 줄을 지우면 **0**.
* `option.txt` 류로 가로·세로 양쪽에서 네 값과 재시작 문구를 그림으로 확인.
* 1,241 프레임 A/B: 기본 플래그는 세로·아바타 모두 꺼짐이라 경로가 없다.
  HEAD 대비 대조 결과는 §57.6.
* **S25 실기기**: 960×1280(정수 4배)로 세로로 뜨고, 위 여백이 컷아웃을
  피하고, 키패드 OK·화살표가 프롤로그·주 메뉴·클래스 선택을 움직이고,
  환경설정에서 `FULL` 로 바꾸면 재시작 문구 → `예` → 다시 켜면 가로 585×270,
  키패드 없음. **필드에서 걷기·멈춤은 아직 손으로 안 해 봤다**(오프라인 28/0).

### 56.6 물려받은 한계

모더 빌드가 그랬듯 **벨트는 그림만 한 줄로 내려왔다** — `SetTouchKeypad` 가 읽는
터치 판정 사본(0x159C38)은 원래 호 배치다. 세로에서 벨트 아이콘을 **손가락으로**
누르면 그림과 판정이 어긋난다(키패드 `1 3 7 9 * 0` 이 정상 경로). 같은 이유로
세로에서 실물 게임패드를 붙이면 패드 배지(§52)가 판정 사본의 자리에 찍힌다.
고치려면 0x159C38 의 앞 열두 halfword 도 같은 값으로 옮기면 되지만, 그러면
모더 빌드와 다른 동작이 되므로 사용자 확인 뒤로 미뤘다.

### 56.7 키패드 편집 — `☎` 자리의 `⚙` (2026-10-14)

사용자 요구: **세로 키패드의 버튼 배치를 사용자가 바꾼다** — 왼쪽 끝 전화기
버튼을 없애고 설정 버튼, 누르면 모든 버튼이 이동·크기 조절, 드래그로 위치,
두 손가락으로 크기.

* **자리.** `☎`(index 18)는 `L` 과 같은 `KEY_MAP` 이었으므로 빼도 잃는 동작이
  없다. 인덱스는 계약이라 그대로 두고 뜻만 바꿨다 — `portrait::KEYPAD[18]` =
  `("설정", None)`, Java 는 18 을 `nativeKeypad` 로 **보내지 않는다**(`None`
  은 잘못 와도 아무 일 없게 하는 두 번째 벽). 그림은 톱니바퀴(글꼴 의존이
  없도록 `Canvas` 로 그린다).
* **여는 법.** `⚙` 에 **내려앉은** 손가락이 `⚙` 위에서 떨어질 때만 연다 —
  걷던 엄지가 스쳐 지나가다 떼는 것으로는 안 열린다. 열면 `releaseAll()` 로
  잡혀 있던 키를 전부 게임에 놓아 준다.
* **편집.** 키는 게임에 안 간다. 한 손가락 = 끌기(터치 슬롭을 넘어야 움직인다),
  키를 짚은 채 둘째 손가락 = 핀치(두 손가락 거리 비로, 키 중심 기준). 첫
  손가락이 빈 곳이면 둘째 손가락 아래 키, 아니면 두 손가락 가운데 키를 잡는다.
  핀치의 한 손가락을 떼면 남은 손가락이 이어서 끈다. 크기는 기본 칸의
  **0.5~3배**이고 키패드보다 커지지 않으며, 키는 키패드 밖으로 못 나간다.
  잡은 키는 맨 위로 올라온다(겹치면 위가 눌린다). `⚙` 를 움직이지 않고 한 번
  탭하거나 게임 화면 위 판의 `완료` 로 끝낸다. `초기화` 는 격자로 되돌리고
  파일을 지운다(확인 없음). 그 판은 게임 화면을 덮어 편집 중 탭이 게임으로
  안 새게 한다.
* **저장.** `<files>/hl5_keypad.txt`(= `getFilesDir()/files`, `hl5_resolution.txt`
  옆), 제스처가 끝날 때마다 임시 파일 + rename. 한 줄 = `키번호 가로중심
  세로중심 크기`, 중심은 키패드 폭·높이에 대한 비율이라 컷아웃 크기가 다른
  기기로 옮겨도 모양이 유지된다. 줄 순서가 그리는 순서다. 읽기는 관대하다 —
  못 읽는 줄·중복 키·범위 밖 값은 **그 줄만** 버리고 그 키는 격자 자리.
* **걷기 미끄러짐.** 걷는 엄지가 숫자 블록 안의 빈 곳·`5` 로 미끄러져도 계속
  걷던 규칙(§56.3)은 이제 *숫자 블록 키들의 현재 경계상자* 를 쓰고, 그 안에
  들어온 **숫자 블록 밖의 키**(사용자가 옮겨 넣은 `OK` 등)는 원래대로 눌린다.
  기본 격자에서는 예전과 같은 동작이다.
* **범위 밖으로 남긴 것.** 키를 게임 화면 위로 올리는 것(키패드 뷰가 게임 뷰
  아래만 차지하므로 겹치려면 뷰 구조를 바꿔야 한다), 되돌리기, 초기화 확인.
* **시험.** Java 만의 기능이라 오프라인 프로브로 재현할 수 없다. `cargo test`
  (222 통과)와 디버그 APK 빌드(javac 통과)까지가 에이전트 몫이고, 손 시험은
  사용자가 한다(에뮬레이터 금지 — CLAUDE.md 머리말).

## 57. 캐릭터 페이지의 `*아바타` 버튼과 옷장 (2026-10-14)

사용자 요구: **인게임 메뉴 → 캐릭터 페이지 하단 `#상세` 버튼의 왼쪽 끝에
아바타 버튼**, 그리고 hero3 처럼 이 게임 도트에 맞는 아바타들. 코드는
`hl5_core/src/avatar.rs`, 스위치는 `Config::avatar`(셸 on, 프로브 `--avatar`),
상태 파일은 `<files>/hl5_avatar.txt`.

### 57.1 버튼 자리는 게스트의 빈 칸이고, 이미 눌린다

`StateInGameMenu::DrawCharacterMenu`(0xFBC9C)는 `x0 = GetHalfWidth()-0x58`,
`y0 = GetHalfHeight()-0x66` 기준으로 그리고(400×240 → (112,18), 585×270 →
(204,33), 240×320 → (32,58)), 하단 막대는 `CommonUi::DrawHalfBox(.., 0x45, 0x11,
0, 0, 0xff)` 판 둘이다 — `*취소`(x0, 반환 0xFC8D8)와 `#상세`(x0+0x6b, 반환
0xFC998). **`*취소` 는 스탯 포인트가 남았을 때만 그려진다**(`this+0x20..0x26`
s16 넷 중 하나라도 > 0, 0xFC85C). 그런데 `TouchCharacterMenu`(0x103ACC)는 두
하위 페이지 모두에서 그 칸의 탭을 **그려졌든 아니든** `KeyPress('*')` 로
바꾼다(0x103C9C). 그래서:

* 버튼은 그 칸에 `*취소` 가 없을 때 그린다. 라벨 `*아바타` — 게스트의
  `*취소`·`#상세` 와 같은 꼴, 같은 판 스타일(0,0), 같은 회색 0x808080.
* 터치 후크가 필요 없다. 게스트가 `'*'` 를 내고, `KeyCharacterMenu` 진입 후크가
  **`CancelApplayStat` 이 취소할 것이 없을 때만** 그 `'*'` 를 가져간다. 그려진
  것과 눌리는 것이 정확히 같다. 키패드의 `*` 도 같은 길이다.
* 패드는 메뉴에서 R1(`*`)이 `#` 으로 바뀌므로(§53.2) `'0'` 도 연다 —
  `KeyCharacterMenu` 가 처리하지 않는 키이고 L2 가 보낸다.
* 상세 페이지에서 누르면 `SetSubState(0)` 뒤에 연다(옷장은 기본 페이지 위에 산다).

### 57.2 옷장 — 스탯 블록 위에, 게스트의 판과 글꼴로

열리면 헤더 아래 선과 막대 사이(`y0+0x40..0xa0`)를 판 배경(16,28,41)으로 칠하고
탭 여섯(투구 갑옷 장갑 신발 무기 방패, 28 px 간격)과 네 줄 목록을 그린다.
판은 옵션 행과 같은 `DrawHalfBox` 스타일 쌍(커서 (1,1) / 나머지 (4,0)), 글은
`BFont::DrawText`. **헤더는 게스트가 계속 그린다** — 거기에 영웅 미리보기가
있으므로(`CHAR::GetSpritePtr` + `SPRITE::DrawSprite`) 줄을 고르면 그 그림이 그
자리에서 바뀐다. 첫 줄은 언제나 `기본 외형`(벗기), 입은 줄에는 주황 `착용`.

조작: 위/아래 커서, 좌/우 탭, OK 입기, `*`·`0`·CLR 닫기, `#` 은 닫고 게스트에
넘겨 상세로. 탭: 탭 줄 → 탭 전환, 목록 줄 → 그 줄을 바로 입음, 버튼 → 닫기,
그 밖(뒤로 모서리·`#상세`·아래 아이콘 줄) → 닫고 게스트에 넘김. 페이지를
벗어났다 오면(그리는 간격 > 2 재칠) 닫힌 채로 시작하고, 스탯 포인트가 생기면
닫힌다.

### 57.3 입히기 — 영웅 시트 이름이 정해지는 유일한 곳

`HERO::SetLoadImgPath`(0x8DE58)가 `_szSprFile`(0x100 × 10)과
`_nSprPalFile`(s16 × 10)을 지우고 착용품 `ItemInfo+0x150/+0x152` 로
`/c/sp/img<N>/%03d.mgr` 와 팔레트를 쓴다. 호출자는 네 `<CLASS>::LoadSprite`(게임
로드)와 `HERO::ChangeSpriteImage`(모든 재착용·제련·오브), 그리고 그 결과가
**필드와 메뉴 미리보기가 공유하는** 스프라이트(`gv+0x1468/0x146c`)다. 후크는
원본을 `vm.call` 로 돌린 뒤 입은 층만 덮어쓴다 — **아이템은 한 바이트도 안
건드린다**(`+0x150` 은 세이브에도 안 들어간다, §45).

* 무기 층은 **손에 무기가 있을 때만**(`HERO::IsNoneEquipWeapon`) — 빈손에
  무기 외형이 생기면 안 된다. 방패 층은 게스트가 그 층을 채웠을 때만(나이트 +
  방패 착용).
* 투구보기 토글은 `SetLoadImgPath` 를 안 지나고 `KeySystemMenu+0x144` 가
  층 0 을 직접 써서 `SPRITE_IMAGE::ChangeSprImg`(0xDB01C)를 부른다. 그 함수
  진입에서 층 0 을 다시 입히고 사양한다.
* 입기 직후 `HERO::ChangeSpriteImage(hero, 0)` — 게임 자신의 재적재.
* 슬롯 = `(i8)gv[0x1970]`(`Battle::KeyPressed+0x1a0` 이 `SaveAll` 에 넘기는 값),
  클래스 = `HERO+0x22c`. 파일은 `slot class layer=look ...` 한 줄씩이고, 슬롯의
  영웅이 기록과 다른 클래스면(새 캐릭터, 스토리의 다른 인물) 입히지 않는다.

### 57.4 옷장에 무엇이 있나 — 1차는 게임 자신의 도트

외형은 `(층, 시트, 팔레트, 클래스)` 하나다. 1차 카탈로그는 부팅 때 **아이템 표에서
만든다**: 등급 ≥ 4(전설·세트) 행의 `(+0x150, +0x152)` 를 중복 없이, 이름은 그
행의 이름 그대로. 투구·갑옷·장갑·장화 각 14(세트 열넷 — 등급 4 투구 셋은 보석 헤어핀
류로 시트 0·착용 계층 7 이라 빠진다), 무기 64(클래스별, 각 표가 곧 클래스), 방패 12(나이트).
방어구의 착용 계층 6(워리어·나이트 전용)은 그 둘에만 보인다. 132 개 전부 아카이브에
실제로 있는 시트다(단위시험이 VFS 로 확인 — §45 에서 없는 시트가 머리를 지웠다).

**세트 외형은 대개 같은 시트에 다른 팔레트다**(가시나무갑옷 = img1/005 + 팔레트
123). 새로 그린 의상은 같은 `Look` 으로 들어가고, 시트는 아카이브에 없는 번호로
호스트 섀도에서 서빙한다(§57.7).

### 57.5 인수

`tools_re/scripts/avatar.txt`(400×240, `--avatar`): 버튼이 그려지고, 탭으로
열리고, 갑옷·무기·투구(마법사 모자)를 고르면 헤더 그림이 바뀐다 — 요약 줄
`opened 1x, 3 change(s)`, 파일 `0 0 0=100001 1=6023 4=26`. 무기 `페이튼:워액스` 를 입으면 미리보기의 대검이 도끼가
되고 닫은 뒤에도 남는다. 세로(240×320)에서도 같은 페이지가 그대로 맞는다.

### 57.6 A/B

기본 플래그(아바타·세로 꺼짐)로 HEAD(074ab44) 대비 **1,241 프레임 5장면 전부
바이트 동일**(`git worktree` + `ab_905.sh head new` / `cur new` +
`ab_compare.py`).

### 57.7 새로 그린 의상 — 게임 머리 위에 얹는 격자, 합성은 부팅 때

hero3 처럼 **이 게임 도트에 맞춘 의상을 직접 그린다.** 첫 다섯은 머리
(`img0`) — 마법사 모자, 황금 왕관, 고양이 귀, 산타 모자, 밀짚모자.

* **형식은 `tools_re/mgr.py` 가 정본이다**(아카이브의 427 시트 전부 바이트
  동일 왕복). `.mgr` = `[u32 n]` + n × `[u32 size][GBM]`, GBM =
  `[type][ncolors][u16 w][u16 h][565 팔레트][픽셀 4|8bpp]`, 투명은 색
  `0xF81F`. `.cif` 는 이미지를 **중심**에 놓고 인덱스 경계 검사가 없다.
  `.pal` 은 색 치환표이고 출하 장비는 전부 하나씩 단다(`000.pal` 도 다섯 색).
* **머리는 하나다.** 네 직업이 같은 주인공이라 `img0/000` 하나를 쓰고(이미지
  7장, 3/4 앞이 80회로 가장 많다), `img5` 는 몸이 아니라 직업별 이펙트다 —
  그래서 머리 의상 하나가 네 직업에 다 맞는다.
* **게임 픽셀은 저장소에 없다.** `costumes/sets/*.py` 의 격자(우리 그림)만
  저장하고, `costumes/build.py` 가 그것을 `hl5_core/src/costume_sets.rs` 로 옮겨
  적으며, `hl5_core::costume` 이 부팅 때 **플레이어의 아카이브**에서
  `img0/000.mgr` 을 풀어 합성하고 다시 인코드해 섀도(`img0/256..260.mgr`)로
  쓴다. 캔버스는 양쪽으로 같은 만큼 넓혀 중심을 지킨다. 번호는 **256 이상**
  — 몬스터·NPC 경로 빌더는 번호를 부호 있는 바이트로 읽으므로 절대 못 가리킨다.
  섀도가 실제로 써진 의상만 옷장에 들어간다(없는 시트 = 안 그려지는 층, §45).
* 의상의 팔레트는 −1(자기 색). 슬롯 기록 id 는 100001.. 이고 재사용 금지.
* 시험: 원본 시트 디코드·재인코드 픽셀 동일, 모든 의상이 이미지 수를 지키고
  짝수로만 넓어진다, id·번호 유일, 4bpp 패킹. 프로브: 다섯 모두 캐릭터 페이지
  미리보기와 **필드**에서 보이고, 저장 → 새 프로세스로 슬롯을 불러도 그대로다
  (둘째 실행의 `1 layer(s) dressed` 는 `LoadSprite` 경로).

**아직 사람 눈이 필요한 것**: 다섯 시안의 미감(특히 3/4 마법사 모자의 곧은
뒤쪽 변), 그리고 의상 획득 방식 — 지금은 hero3 의 엔딩 보상과 달리 **전부
처음부터 열려 있다.** 몸통·팔·다리(`img1..3`, 12·13·17장)와 무기 의상은
같은 파이프라인으로 그릴 수 있지만 그리지 않았다.

### 57.8 아스트라 11종 — v1 은 버리고 v2 로 다시 그렸다 (2026-10-14)

사용자가 준 콘셉트 11장(`docs/astra_avatar/references`, `drafts`)을 다섯 부위
(`img0..4`, 시트 301..311, id `110000 + 번호×10 + 레이어`)로 옮긴 1차분(v1,
다각형 채우기 생성기)은 **사용자가 품질이 너무 낮다고 거절했다.** 이 크기에서는
다각형이 덩어리로만 읽힌다 — 머리가 17 px 이고 몸통 10×11 은 대부분 머리에
가린다(`.cif` 의 대기 프레임에서 머리 중심 y −24..−26, 몸통 −16..−17).

v2 는 생성 방식을 부위마다 바꿨다(`costumes/astra_studio.py`, 설계는
`costumes/astra/aNN.py`):

* **머리**: 얼굴(살색·눈·입과 턱선)은 게임 것을 남기고, 머리칼만 바꾸는 외형은
  게임 머리 시트의 청록·흰색을 **밝기 순위로** 새 램프에 사상해 원본 음영을
  지킨다. 모자·투구·탈·후드는 손 격자로 통째로 그린다.
* **몸**: 손 격자. 보이는 것은 머리 아래 다섯 줄과 프레임 바깥이므로 코트 자락·
  망토·털깃·견장은 프레임 밖으로 자란다(대칭 확장, 중심 불변).
* **팔·다리**: 게임 조각 재배색. 팔은 맨 윗줄(어깨)에서 8방향 BFS 로 걸어 나간
  거리로 손(끝 3)·커프(1)·소매를 가르고, 다리는 원본 팔레트 25색을 바지·부츠·
  띠·가죽·하이라이트로 분류해 둔다. 원본 음영 순서가 남아 애니메이션이 같다.
* **무기**: 칼 시트 네 방향(`img4/000`)의 축(자루 끝 → 칼끝)을 프레임마다 재 두고,
  그 축 좌표계에 벡터를 8배로 그려 칸마다 다수결로 축소한다. 쌍검은 로그 시트
  (`img4/016`) 재배색, 무기 없는 외형은 투명 시트(맨손, 판정 불변).

인수는 v1 과 같은 틀이다: CIF 부품 참조 532,301 개(11종 × 4직업) 통과,
`hl5_core` 단위시험, 워리어·로그 헤드리스 재생에서 옷장 변경 54 + 5 폴트 0,
그리고 **재생의 저장소에 런타임이 합성한 55 시트가 내보낸 MGR 과 픽셀 동일**.
id·시트 번호를 그대로 썼으므로 이미 저장된 `hl5_avatar.txt` 는 새 그림으로
그대로 입는다. 획득 단계는 여전히 없다 — 옷장의 행 필터는 직업 호환
(`Look::classes`) 하나뿐이라 모든 슬롯이 전부를 본다.

## 58. 아바타 v3 — 리그로 프레임마다 다른 그림, 직업별 무기, 아바타 색 이펙트 (2026-10-14)

요구: 프레임을 늘려 움직임을 자연스럽게, 초안(`docs/astra_avatar/drafts`)과
최대한 닮게, 부자연스러운 눈을 고치고, 모든 아바타를 올린다. 그리고 (추가 요구)
직업별로 컨셉에 맞는 무기 — v2 의 "맨손" 은 건슬링어가 맨손에서 총알을 쏘게
했다 — 와 무기에 따른 공격·스킬 이펙트.

### 58.1 "프레임을 늘린다" 의 실제 모양

게임은 틱마다 CIF 프레임을 정확히 하나 넘긴다: `WARRIOR::EndProc`(0x146700)이
`HERO::GetSkipFrame` 을 더해 `SetSpriteFrame` 하고 `CHAR::IsEndFrame` 이 아니면
`CHAR::IncreaseSpriteFrame`(0x49BA8). 모션 = 애니메이션 번호(`CHAR::SetMotion`
0x4AF5C 가 `GetMaxFrame` 을 `+0xc6` 에 캐시)이고, 서기 = 2·걷기 = 3, 무기가
없으면 `HERO::CheckWeaponMotion`(0x8DD58)이 0·1 로 내린다. 대기 6 / 걷기 6 /
무장대기 12 / 무장걷기 6 프레임 — 36 ms 틱에서 걷기 한 주기가 0.22 초다.

**프레임 수를 늘리면 모션 길이가 늘고, 공격 판정 프레임과 보폭이 바뀐다.** 그런데
그 프레임들은 머리·몸통으로 **같은 그림 한 장**을 1 px 씩 옮겨 쓸 뿐이다(머리
3/4 는 대기 6 프레임 내내 `L0#1`, 몸통 `L1#1`). 그러니 늘릴 것은 프레임 수가
아니라 **프레임마다의 그림**이다:

* 시트 뒤에 포즈 그림을 덧붙인다 — 머리 시트 37장(원본 7 + 머리칼 포즈
  3뷰×9 + 뒷머리 3), 몸 시트 27장(원본 12 + 자락 포즈 3뷰×5).
  `SPRITE_IMAGE::LoadSprImg`(0xDB510)이 층마다 `GbmMerger::GetMergedGbmCount`
  만큼 할당하므로 긴 시트는 통째로 실린다.
* 포즈는 CIF 자체에서 **유도**한다(`costumes/rig.py`): 프레임 사이 머리·몸통의
  이동(Δx, Δy)을 읽어 "느슨한 끝이 한 박자 늦는다" — 머리가 오르면 머리칼 끝이
  눌리고(hy=+1), 내리면 늘어나고(hy=−1), 옆으로 옮기면 반대로 남는다(hx). 걷기는
  보폭 주기 표를 더한다(앞·뒤는 좌우 흔들림, 옆은 뒤로 흐르며 펄럭임).
* **CIF 변형본**이 이 네 모션의 머리·몸통 부품의 **이미지 번호 바이트만** 바꾼다
  (재매핑 198건). 원본 직렬화가 아니라 제자리 패치다 — `SPRITE_DATA::LoadSprData`
  (0xDBC64)가 빈 슬롯 `0x7F` 도 부품 수에 세므로, `mgr.cif_decode` 처럼 그것을
  버리는 디코더로 다시 쓰면 바이트가 어긋난다(직업 0·1·2 에서 실측).

### 58.2 변형본을 언제 읽히나

CIF 는 `<CLASS>::LoadSprite` 에서 한 번 읽힌다: `SetLoadImgPath` → `new
SPRITE_DATA` → `gv+0x1468` 에 저장 → `LoadSprFile("/c/sp/cif/00N.cif", 1)`. 옷을
갈아입는 `HERO::ChangeSpriteImage`(0x9B338)는 `FreeSprImg` + `LoadSprImg` 로
**시트만** 다시 읽는다. 그래서:

1. `load_hook`(SetLoadImgPath)이 방금 입힌 머리·몸이 리그인지 기록한다.
2. `cif_hook`(LoadSprFile)이 `this == gv[0x1468]` 이고 경로가 직업 CIF 면 r1 을
   변형본 경로로 바꿔 사양한다 — 원본이 그 경로를 읽는다.
3. `change_hook`(ChangeSpriteImage)과 옷장의 `redress` 가 원본을 돌린 뒤 쌍이
   바뀌었으면 같은 객체에 `FreeSpriteData` + `LoadSprFile` 을 부른다.

`gv+0x1468` 의 다른 사용자는 전부 포인터를 넘기거나 해제할 뿐이다(`Battle::Paint`,
`Event_PlayerChange`, `ChangeHeroClass`, `<CLASS>::FreeSprite` …) — 같은 객체를
다시 채우므로 안전하고, 프레임 수가 같으니 현재 프레임 번호도 유효하다. 시트와
CIF 는 한 게스트 호출 안에서 함께 바뀌어, 변형 번호를 모르는 시트와 짝지어
그려지는 프레임이 없다. 변형본 파일은 `c/sp/cif/9<직업+1><1|2|3>.cif`(아카이브는
000..124 와 이름 붙은 파일뿐).

재생 확인: 11종을 차례로 입히는 재생마다 `2 rig cif read(s)`(기본 → 머리 → 머리+몸;
아스트라끼리는 같은 변형을 쓴다), 폴트 0.

### 58.3 눈

원본 머리(`img0/000`)는 눈을 **머리칼의 청록 램프**로 칠한다 — 정면 왼눈은
`(5,11)` 짙은 청록, `(5,12)` 밝은 청록, `(6,12)` 외곽선, `(6,13)` 중간 청록. v2 의
머리칼 재배색이 눈까지 주황·회색으로 바꾼 것이 "부자연스러운 눈" 이었다.
`astra_studio.EYES_BASE` 가 프레임 0·1·4·6 의 눈 픽셀을 역할(속눈썹·동공·홍채·
반사)로 짚고 디자인의 `EYE` 로 칠한다. 머리칼 격자는 피부·눈을 덮지 못한다
(가면·투구·탈만 `over_face`).

### 58.4 무기는 직업마다, 축은 시트마다

직업의 기본 무기 시트가 다르고 조각의 그림 방향도 다르다: 워리어 `img4/000`(검
4조각), 로그 `016`(단검 7조각, 4~6은 원근으로 짧아짐), 건슬링어는 손잡이가 보이는
`044`(총 5조각), 나이트 `050`(창 3조각, 2는 자루만) + 방패 `img6/000`. 조각마다
(손 끝, 먼 끝[, 총 손잡이 방향]) 축을 재고(`astra_studio.WEAPON_KINDS`),
`costumes/astra/arms.py` 가 그 축을 따라 8배로 그려 다수결로 줄이고 1 px 외곽선을
두른다. 짧아진 단검 조각만 재배색이다. v2 는 검 시트를 워리어·나이트 둘에게
줬는데(직업 비트 `0b1001`), 나이트 CIF 는 그 조각들을 창 자리에 놓는다 — 이제
시트마다 그 직업 하나다(단위시험이 기본 시트로 직업을 강제한다).

### 58.5 이펙트

`SetLoadImgPath` 는 `_nSprPalFile` 을 전부 −1 로 지우고 층 5 에 직업 이펙트
`img5/00N.mgr` 를 쓴다. 아스트라 무기를 입히면 같은 아바타의 이펙트 시트를 층 5 에
같이 입힌다. 이펙트 시트는 원본에서 **색만** 바꾼다: 베기·불꽃·구체·폭발의 채도
있는 색을 아바타 색조로 돌리고(명도·채도 유지, 흰 심 유지), 버프 글자·등급
화살표·먼지·탄피 같은 "색이 정보인" 그림은 그대로 둔다(`FX_KEEP`). 저장은
565→565 대응표다 — 런타임은 표를 적용할 뿐이라 파이썬 미리보기와 같은 픽셀이
나온다. 공격 재생에서 01 워리어 주황, 06 워리어 얼음빛, 05 로그 보라, 04
건슬링어 금빛 총구, 09 나이트 푸른 찌르기를 확인했다.

### 58.6 그림

머리칼은 원본 재배색 대신 뷰마다 손격자(01·06 스파이크는 `astra/kit.py` 공용),
몸통은 초안의 실루엣대로 다시 그렸다 — 발목까지 오는 장포·장교 코트(옆에서는
앞섶이 뒤로 물러나 앞다리가 나온다), 털망토(`kit.fur`), 갑옷·휘장, 가면·삿갓·
투구·탈. 다리는 색 이름이 아니라 **위치**로 나눈다(각 열의 맨 아래 = 발) — 원본
바지가 짙은 회색, 신발이 보라회색이라 v2 의 색 이름 영역이 반대였고 01 의 붉은
신발 램프가 바지를 칠했다. 팔은 맨살도 내 피부 램프로(원본 피부의 노란
하이라이트가 장갑 옆에서 금팔찌처럼 보였다), 권투 장갑은 손 영역을 1 px 키운다.

### 58.7 인수

* `cargo test -p hl5_core --lib` 225 — 그중 리그 CIF(직업 넷 모두 머리·몸통
  이미지 번호 바이트만 바뀌고 전부 시트 안), 변형 경로가 아카이브와 안 겹침,
  이펙트 시트(색만·유지 그림 불변·투명 불변), 무기 직업·이펙트 짝.
* `astra_preview.py`: 44 아바타×직업 조합의 모든 애니메이션 부품 참조 532,301건이
  변형본으로도 시트 안.
* 직업 넷 재생(`docs/astra_avatar/native/replay_<직업>.txt`) 폴트 0, 그리고
  `astra_verify.py`: 런타임이 합성한 **143/143 시트가 내보낸 MGR 과 픽셀 동일**,
  리그 CIF 12개.
* 아바타를 켜지 않으면 후크도 합성도 없다(`cfg.avatar`) — `ab_905.sh` 5장면 1,241 프레임
  전부 바이트 동일. 아바타를 켠 부팅은 143장 합성·대조로 데스크톱에서 80~190 ms 더 든다.
* 기기 미시험.

## 59. 아바타 옷장 — 드래그로 목록을 스크롤한다 (2026-10-14)

사용자 보고: 아바타 창에서 드래그해도 목록이 안 움직인다. 투구 탭은 30행, 무기는
108행인데 네 줄씩 키(또는 행 탭)로만 내려갈 수 있었다.

### 59.1 게스트 메뉴는 드래그를 모른다

`_ZTV15StateInGameMenu`(+8) 의 `+0x28` 이 `TouchReleased`(0xE6498), `+0x2c` 가
`TouchMoved`(0xE649C)이고 **둘 다 `bx lr` 한 줄**이다. 그래도 `mainKeyPressed`
(0x9CA18)는 틱마다 우편함을 비우며 그 둘을 부른다 — 누름 칸(`gv+0xf0`)이 차
있으면 `+0x24`, 아니면 이동 칸(`gv+0xf2`)이 차 있으면 `+0x2c`, 그 다음 뗌 칸
(`gv+0xf1`)이 차 있으면 `+0x28`. 그래서 그 두 주소가 후크 자리이고(그 vtable
말고 참조하는 데이터가 없다), 후크는 둘 다 **사양**한다 — 원본 `bx lr` 이 돈다.

### 59.2 우편함이 드래그를 깎는 방식 두 가지

* `BaseState::TouchMove`(0x44DCC)는 **칸이 비어 있을 때만** 쓴다(`streq`) —
  한 틱 사이의 이동은 **첫 것**이 남는다. 그래서 스크롤은 걸음을 누적하지 않고
  "누른 자리 대비 지금 자리"로 계산한다.
* `onHandleEvent` 는 누름마다 `TouchPress`(+0xfc) 뒤에 **같은 좌표의
  `TouchMove`(+0x11c)** 를 하나 더 넣는다(`--watch` 로 실측). 누름이 처리되는
  틱에는 이동 칸을 안 보므로 그 가짜 이동이 한 틱 더 칸을 차지하고, 그 사이의
  진짜 이동은 버려진다. 재생에서 "20 px 아래로 끌고 떼기" 가 통째로 탭이 되어
  행을 입혔다 — 이동 하나가 전부였기 때문이다. 기기에서는 이동이 계속 들어오니
  긴 드래그는 살아남지만 짧고 빠른 것은 똑같이 깎인다.
* `BaseState::TouchRelease`(0x44D28)는 플래그를 **언제나** 세우고 좌표도
  (`gv+0x126` 과 필드 상태가 겹치는 조이스틱 경우만 빼고) 기록한다. 그래서
  **뗀 자리가 최종 위치**다: 6 px 이상 떨어져 있으면 그 자리로 스크롤하고 입지
  않는다.

### 59.3 동작

목록 띠(`y0+0x58` 아래) 의 누름은 그 행을 켜고(커서) **무장만** 한다. 6 px
(`DRAG_SLOP`, 기본 캔버스에서 기기 24 px) 를 넘기면 드래그 — 18 px 마다 한 행,
반올림, 0..len−4 로 묶고, 커서는 보이는 창 안으로 끌려온다(키가 거기서 이어진다).
안 움직이고 떼면 그 행을 입는다 — **입는 시점이 누를 때에서 뗄 때로 옮겨졌다.**
탭·탭 전환·옷장 닫기는 전과 같다. 행 판 폭을 0xa8 → 0xa4 로 줄이고 오른쪽 3 px 에
스크롤바(트랙 + 손잡이, 손잡이 최소 6 px)를 그린다.

### 59.4 인수

* 재생 `tools_re/scripts/avatar-drag.txt`: 슬롭 안 이동은 스크롤 없음, 위로
  70 px → 4행, 아래로 20 px 끌고 떼기 → 1행 되돌림(뗀 자리), 탭과 3 px 흔들린
  탭은 입힘. 요약 줄 `2 change(s)`, `5 row(s) scrolled`, 파일 `0 0 0=100005`.
* `avatar.txt` `3 change(s)`(파일 `0 0 0=100001 1=110011 4=110024` — 아스트라가
  갑옷·무기 탭 맨 위에 들어온 뒤의 값이고 §57.5 의 `1=6023 4=26` 은 그 전 값),
  아스트라 워리어 재생 `2 rig cif read(s)`.
* 단위시험 227(행 단위 반올림·범위, 손잡이가 트랙 끝에서 끝까지).
* 아바타를 끄면 후크가 설치되지 않는다 — A/B 1,241 프레임 경로에는 코드가 없다.
* 기기 미시험.

## 60. 가방·창고를 10페이지로 — 조사만, 코드 0줄 (2026-10-14)

사용자 요구: **가방(싱글가방) 5페이지 → 10페이지, 네트워크 가방(공용창고)도
마찬가지.** 범위를 물었고(창고 10 + 가방 6 을 추천했다) 사용자는 **둘 다
10페이지(대공사)** 를 골랐다. 이 세션은 사용자 지시로 **분석과 문서까지만**
했다. 순서는 **창고 먼저, 가방은 그다음** — 창고는 순수한 즉치 재작성으로
끝나고 세이브를 안 건드리며, 가방은 세이브 형식과 HERO 할당 크기까지 간다.

### 60.1 왜 상수 하나로 안 되는가 — 배열 셋이 객체 안에 붙박여 있다

**`BagItem` 은 HERO 안에 인라인이다** (`HERO::GetBagItemPtr` 0x890C8 =
`add r0,r0,#0x17c0`, 생성자는 `HERO::HERO+0x30` 이 `this+0x17c0` 으로 부른다).
레이아웃은 생성자 `BagItem::BagItem`(0xA0548)과 소멸자·`SetItemFilter` 에서
확정했다:

| 오프셋 | 크기 | 뜻 | 근거 |
|---|---|---|---|
| `+0x000` | i32 | 소지금 | `GetMoney`, `SaveBagData` 첫 워드 |
| `+0x004` | i16 | **용량** (생성자 `0x30` = 48 = 3페이지) | `GetSize`/`SetSize` |
| `+0x006` | i16 | **개수** — 가방은 **압축 리스트**다 | `GetCount`, `IsOverItem` = count > size |
| `+0x008` | 96 × ptr | `ItemInfo*` | `GetBagItemPtr(s)` = `[this, (i+2) lsl 2]` |
| `+0x188` | 96 × i8 | 칸별 **스택 수** | `IsFull` 의 `[r7, #0x188]`, ≤ 99 |
| `+0x1e8` | 96 × ptr | `BagItemFilter*` (필터 목록) | 소멸자 0x60 루프, `SetItemFilter` |
| `+0x368` | i16 | 필터 목록 길이 | `SetItemFilter+0x150` `strh [fp,#0x368]` |

다음 HERO 필드는 `HERO+0x1B2C`(`GetEquipItemPtr`) 이므로 `sizeof(BagItem)` 은
**0x36C** 이고 뒤에 빈자리가 없다. `BagItemFilter` 는 0x18 바이트 힙 객체로
`+0x12` = 원래 칸 번호(i16), `+0x14` = `ItemInfo*` 다.

**슬롯 i ≥ 96 은 세 배열이 서로를 덮는다** — `ptr[96]` 이 곧 `bunch[0..3]` 이다.
용량을 160 으로 쓰기만 하면 첫 줍기에서 스택 수가 포인터를 덮어쓴다. 그리고
메모리 층에서 가를 수도 없다: 32비트 접근 `[this+0x1e8, +0x368)` 은
`filter[0..95]` 일 수도 `ptr[120..]` 일 수도 있다.

**"지금 5페이지"의 출처**: 기본 48 + `item_18` 14행 `창고 확장`(`싱글 창고를
16칸 늘려줍니다. 최대 2개까지`) × 2 = **80 = 5페이지**. 구매 관문은
`StateCashShop::KeyPressed+0x4CC`(0xDDDF0):
`size + 16 × (가방에 든 그 아이템 수) > 0x4F` 면 `common_text 0x12F` 로 거절.
**사용해서 `+0x004` 를 늘리는 자리는 이 세션에서 못 찾았다** — `SetSize` 호출자
0, HERO 상대 `0x17c4` 쓰기는 `LoadBagData` 뿐, 선형 추적기도 못 잡았다. 다음
세션의 첫 조사 항목이다(용량을 160 으로 둔 뒤에도 그 경로가 +16 을 더하면
배열 밖으로 나간다). `--watch _ZN7BagItem7GetSizeEv` 로 그 아이템을 쓰는
재생에서 잡는 것이 빠르다.

96 = 80 + 16: **배열 용량 96 은 5페이지 + 넘침 16칸**이다. 캐시샵 지급
(`NewBagNetItem`, flag 1 → `movne r8,#0x60` 0xA562C)은 용량이 아니라 96 까지
채우고, 그 뒤 `StateInGameMenu::Process`(0x10AAC0)가 count > size 를 보고
`common_text 0x122` 로 정리를 요구한다.

### 60.2 창고 쪽은 깨끗하다 — 따로 할당된 객체다

`NetStorageItem` 은 **`StateNetMenu` 생성자가 `new(0x1E8)` 로 따로 잡아
`StateNetMenu+0x24` 에 둔다** (C1Eia 0x139CC0 / C2Eia / C1Ev / C2Ev 넷, 실제
호출자는 `BaseState::CreateState+0x5C` → C1Eia 하나). 레이아웃(생성자
0xA03CC, `GetBunchCount` 0x9F994, `GetItemState` 0x9F9B8, `DeleteAll` 0xA68F8):

| 오프셋 | 뜻 |
|---|---|
| `+0x000` i32 / `+0x004` i16 / `+0x006` i16 | 화폐 / 용량(생성자 0x30) / 개수 |
| `+0x008` | 80 × ptr |
| `+0x148` | 80 × i8 스택 수 |
| `+0x198` | 80 × i8 상태 (생성자가 memset 하지 않는다) |

**할당 크기가 즉치이므로 객체를 키우고 배열을 다시 깔 수 있다.** 160칸 배치:
`+0x008` ptr[160] (..+0x288) / `+0x288` 스택[160] / `+0x328` 상태[160] /
`sizeof` **0x3C8** (0xF2<<2, 인코딩 가능). 바뀌는 것은 전부 즉치다 — 할당 크기,
생성자 memset 둘(0x140→0x280, +0x148/0x50 → +0x288/0xA0), `GetBunchCount`
`#0x148`→`#0x288`, `GetItemState` `#0x198`→`#0x328`(ldrb imm12), `DeleteAll`
`cmp #0x50`→`#0xA0`, 그리고 아래 전수의 나머지.

`#0x148`/`#0x198` 전수(19곳/13함수, **NetTradeItem 등 다른 구조체가 섞여 있다 —
하나씩 확인할 것**):

```
7  NetStorageItem::NewNetStorageItem        0xa6b80 0xa6b84 0xa6c00 0xa6c04 0xa6c7c 0xa6d90 …
1  NetStorageItem::GetBunchCount / GetItemState / C1 / C2 / DeleteNetStorageItemPosIndex(0xa68ac)
1  NETWORK::writeNetItem 0xc88e4   NETWORK::setSystemError 0xc8fd4 (?)   NETWORK::ProcPacket_Inventory 0xcb070
1  StateNetMenu::SetPopupState 0x12ba5c   GetSimpleItemInfoFromNetItem 0x12d2b8
1  StateNetMenu::TouchBuy 0x12f228 (?)    DrawStorageUI 0x131ec8
cmp #0x50: NetStorageItem::DeleteAll 0xa6918, StateNetMenu::KeyStorage 0x12de04
```

호스트 쪽: `warehouse::DEFAULT_CAPACITY` 80 → 160 (옛 파일은 이미 **위로만**
늘린다, §36.1), `4/6` 이 160 레코드를 보낸다(count = 용량 = 레코드 수).
**`NewNetStorageItem` 은 칸 번호를 검사하지 않는다** — 호스트가 160 미만을
보장해야 한다. 원 서버(`Config::server`)에 붙으면 서버가 80 을 보내므로 아무
것도 안 바뀌어야 한다 — 객체만 크고 용량은 서버 값이다.

**창고 화면은 가방도 그린다** — `DrawStorageUI+0x830` 이 `GetBagItemPtr` 뒤에
`add r0,r0,#0x188`(가방 스택 수)을 한다. 이 한 줄은 2단계(가방) 소관이다.

### 60.3 가방 설계 — 포인터와 스택 수는 제자리, 필터만 밖으로

세 배열을 다 옮기면 포인터 배열 접근(가장 많다 — 인라인 `GetBagItemPtr` 와
`[rP,#8]; add rP,#4` 루프)까지 전부 고쳐야 한다. **포인터 배열을 제자리에 두면
그 접근은 루프 상한만 바뀐다.** 계산:

* `+0x008..+0x368` 에 포인터 N 개 + 스택 N 바이트: 5N ≤ 0x360 → **N = 172**.
  ptr `+0x008..+0x2B8`, 스택 수 **`+0x2B8`**(0xAE<<2, `add` 즉치 가능, ldrb imm12
  가능) `..+0x364`, 필터 길이 `+0x368` 그대로.
* 용량(보이는 칸) **160 = 10페이지**, 넘침 12칸(원래 16) — `NewBagNetItem` 의
  `0x60` → `0xAC`.
* **필터 배열은 HERO 할당 끝으로 간다.** HERO 는 언제나 하위 클래스로 할당되고
  크기가 즉치다: KNIGHT·ROGUE·GUNNER `0x2018`, WARRIOR `0x2020`
  (`mov r0,#0x2000; add r0,r0,#0x18|0x20` → `_ZnwjPci`). 할당 자리 **16곳** —
  `CommonUi::LoadHero`, `StateInGameMenu::ChangeHeroClass`,
  `StateMainMenu::NewGame`, `StatePopup::Process` 각 넷. 이것을 전부 **0x22D0**
  (0x2000 + 0xB4<<2)으로 키우고 필터를 `HERO+0x2020` = **`bag+0x860`** 에 둔다:
  `[r, #0x1e8]` → `#0x860`(imm12), `add rI,rI,#0x7a` → `#0x218`(0x86<<2) 뒤
  `[rB, rI, lsl #2]`, `add r0,#0x1e8` → `#0x860`(0x86<<4). 셋 다 인코딩된다.
* HERO 상대로 쓰인 것: 스택 수 `HERO+0x1948`(`add #0x1940; add #8` 꼴) →
  `HERO+0x1A78`(`#0x1a40` = 0x69<<6, 그리고 `#0x38`), 필터 `HERO+0x19A8` →
  `HERO+0x2020`.

**모든 변경이 명령어 한 워드의 즉치 교체다** — 호스트 후크가 필요 없다.
`portrait::apply`(§56)와 같은 방식으로 `(주소, 원래 워드, 새 워드)` 표를 두고
**전부 대조한 뒤 하나라도 다르면 하나도 안 쓴다**. 스위치 하나 뒤에 두고
(`Config` 기본 off → A/B 1,241 프레임 경로에는 없다), 켜면 `ab_905.sh` 대신
아래 60.6 의 인수 기준을 쓴다. **구현하면 CLAUDE.md 의 "portrait 가 호스트가
게스트 코드를 쓰는 유일한 자리" 문장을 고쳐야 한다.**

### 60.4 세이브 — 스택 수 96 바이트가 고정 길이로 들어간다

`HERO::SaveBagData`(0x8F4F4): `[i32 money][i16 size][i16 count]` + count 개 레코드
(`kind` `index` 2바이트, `kind ≤ 10` 이면 int64 셋 + `+0x15c..+0x16D` 18바이트
= 0x2C) + **스택 수 0x60 바이트**(0x8F748 루프 `cmp r3,#0x60`) + 핫키 등
(`HERO+0x1B2C..`) → `SaveEncryptFile(path, buf, fp+0x21)`(0x8F8D8). 버퍼 크기는
`HERO::GetBagDataSize`(0x89130) = `8 + Σ(2 | 0x2C) + 0x95`.
`HERO::LoadBagData`(0x8EB4C): `MX_sdGetSize − 0x10` 만큼 읽어 복호화, 먼저
`DeleteBagItemPosIndex` 를 **0x60 번** 돌리고(0x8EBD4) 순차로 파싱한다.

계획: **96 바이트 구역은 그대로 두고 `bunch[96..172)` 76 바이트를 파일 끝에
꼬리로 붙인다.** 그러면

* 새 빌드가 옛 세이브를 읽으면 꼬리가 없다 → 0.
* 옛 빌드가 새 세이브를 읽으면 꼬리를 안 본다 — **count ≤ 96 일 때만 안전**.
  count > 96 인 세이브를 옛 빌드로 열면 옛 배열을 넘친다. 되돌리기 경고로 남긴다.
* `LoadBagData` 가 뒤에 남는 바이트를 정말 무시하는지(길이 검사·체크섬 없음)는
  **아직 확인 안 했다** — 함수 꼬리 0x8EEE4~ 를 읽을 것.
* 자리: `GetBagDataSize` 반환 +76, `SaveEncryptFile` 을 `SaveBagData+0x3E8`
  복귀주소에서만 잡아 꼬리를 붙이고, 로드 쪽은 `LoadDecryptFile` 이 돌려준 길이로
  꼬리를 찾는다. `tools/savedump_decode.py` 도 꼬리를 알아야 한다.
* **용량은 모두에게 160** — 생성자 `mov r3,#0x30`(0xA0550 근처) → `#0xA0`, 그리고
  로드 뒤 `max(저장값, 160)`. 그러면 위 구매 관문이 확장 아이템을 스스로 거절한다.

`HERO::LoadHeroData`(0x8E8AC, 0x8EA4C)·`SaveHeroData`(0x8F3DC)에도 `cmp #0x60` 이
있다 — 가방인지 다른 96 짜리인지 확인할 것.

### 60.5 전수 — 다음 세션이 하나씩 읽을 목록

**게임 코드는 단일 번역 단위다** (`_GLOBAL__I_._code_Heroslore5.cpp`). 그래서
`BagItem` 의 작은 메서드가 **어디에든 인라인될 수 있고**, 메서드만 고쳐서는
끝나지 않는다. `HERO::GetBagItemPtr` 호출 435곳(직접 분기)이 있고, 반환값을
레지스터로 따라간 선형 추적(호출 뒤 400 명령, `mov`/`add #imm` 전파, `bl` 에서
r0–r3 소멸)이 **배열을 직접 읽는 함수 37개**를 냈다:

```
StateInGameMenu: ChangeHeroClass(6) DrawBagMenu(3) TouchSpiritMenu TouchEquipMenu
  DrawSpiritMenu KeySpiritMenu TouchBagMenu KeySkillMenu KeyEquipMenu KeyBagMenu
  Process TouchSkillMenu
StateInNpcMenu: KeyRefine TouchOrbCombine DrawOrbCombine DrawCollection KeyOrbCombine
  KeyCollection TouchRefine DrawRefine Process TouchCollection DrawItemSale
StateNetMenu: Process DrawStorageUI
RefineItem: IsItemRefinnableItem IsOrbCombinablePart GetItemRefineMoney
  ApplyItemDecompose ApplyItemCompose ApplyOrbCombine ApplyItemRefine
NETWORK::ProcPacket_Inventory   (EquipItem/EquipItemSpirit Get/Set 은 오탐일 수 있다)
```

선형 추적은 **분기를 무시하고, 객체 멤버에 저장됐다 다시 읽힌 bag 포인터를
못 따라간다** — 하한이지 전부가 아니다. 나머지 근거:

* `cmp #0x60` 18곳/11함수: `LoadHeroData`×2, `LoadBagData`×2,
  `GetBagItemPtrCategoryIndex`×2, `GetBagItemTotalBunchCount(aa)`×2,
  `DeleteBagItemCategoryIndex(aah)`×2, `BagItem::~` C1/C2 ×2씩, `SaveHeroData`,
  `SaveBagData`, `SetItemFilter(aa)`, `SetItemFilter(i)`.
* `mov #0x60` 9곳: `BagItem::BagItem` C1/C2(memset 길이), `NewBagNetItem` 0xA562C,
  `NewBagItem` 0xA5B60, `ProcPacket_Inventory`×2, 그리고 오탐 후보
  `readNetItem` 0xC8588 / `StateGame::SetTouchKeypad` / `StateInNpcMenu::DrawItemBuy`.
* 스택 수 `#0x188`: `BagItem` 메서드 전부(`DeleteBagItemPosIndex` 5, `SortBagItem`
  4, `NewBagNetItem` 4, `NewBagItem` 4, `DeleteBagItemBunchPosIndex` 2, `IsFull`,
  `GetBagItemPtrCategoryIndex`, `GetBagItemTotalBunchCount`,
  `DeleteBagItemCategoryIndex`, 생성자) + `HERO::PickUpMapItem`×2,
  `RefineItem::IsOrbCombinablePart`×2, `ChangeHeroClass`×2, `DrawSpiritMenu`×2,
  `DrawBagMenu`, `TouchSpiritMenu`, `KeySkillMenu`, `KeyCharacterMenu`,
  `DrawEquipMenu`, `DrawCharacterMenu`, `DrawOrbCombine`, `DrawItemSale`,
  `StateInNpcMenu::Process`/`KeyItemBuy`, `StateNetMenu::DrawStorageUI`/`DrawSale`/
  `Process`/`KeyMailBox`, `ProcPacket_Shop` … (이 즉치는 **몬스터·맵·스프라이트에도
  흔하다** — 함수 이름만 보고 고르지 말고 기준 레지스터를 확인할 것).
* 필터 `#0x7a`(= 0x1E8/4) 112곳/39함수, 그중 가방 UI 는 `DrawOrbCombine` 13,
  `StateInNpcMenu::Process` 11, `DrawRefine` 8, `KeyOrbCombine` 7, `DrawSpiritMenu`
  5, `DrawCollection` 4, `DrawBagMenu` 3, `KeyRefine` 3 … (`Map::*`·`ParticleMgr` 는
  오탐). `#0x1e8` 은 `SetItemFilter`×2, 소멸자, 생성자, `StateInGameMenu::Process`,
  `TouchComboMenu`, `TouchItemSale` …. 필터 길이 `#0x368` 은 가방 UI 의
  `Touch*`/`Key*`/`Draw*` 20여 함수(NETWORK 의 `#0x368` 은 다른 구조체).
* **탭 다섯**: `DrawBagMenu` `cmp #5` 세 곳(+0x2C8, +0xD50, +0x1294),
  `DrawStorageUI` 두 곳(+0x118, +0xB54). 페이지 수 = `GetSize()/16`
  (`add #0xf; asr #4`)이 `DrawBagMenu`·`TouchBagMenu`·`KeyBagMenu`×3·
  `StateInNpcMenu::{Draw,Key×3,Touch}ItemSale`·`DrawStorageUI`·`TouchStorageUI`·
  `KeyStorage`·`DrawStorage` 에 있다 — 페이지 수는 따라오지만 **탭 그림은 5칸에
  맞춰져 있어 10탭 배치는 화면을 보고 정해야 한다.** 이 세션은 화면을 못 찍었다:
  `npcmenu.txt` 로 만든 storage 가 창고접속의 슬롯선택에서 세 칸 다 `empty` 로
  보였다(`LOCAL.sav` 는 있었다). 세이브 만드는 경로부터 다시 볼 것.
* 온라인 인벤토리(`ProcPacket_Inventory`, 카테고리 0)도 0x60 을 쓴다. 오프라인에서는
  안 닿지만 `Config::server` 로 원 서버에 붙으면 서버가 96 을 전제로 말한다.

### 60.6 인수 기준 (구현할 때)

동작을 바꾸는 일이라 바이트 동일 A/B 는 정의상 못 쓴다. 대신:

1. **스위치 off 에서 1,241 프레임 HEAD 대비 바이트 동일** (경로가 없음을 증명).
2. **동적 교차검증** — 프로브에 일회용 관측을 붙여, 켠 상태로 가방을 160칸까지
   채운 재생(정렬·판매·제련·오브·도감·조합·창고 입출고·직업변경·저장/재시작)을
   돌리며 `bag+[0x008, 0x36C)` 와 `bag+0x860..` 에 닿는 모든 PC 를 기록하고,
   **패치 표에 없는 PC 가 하나라도 있으면 실패**. 정적 전수의 구멍을 잡는 유일한
   방법이다. 필요한 것: 아이템 N 개를 가방에 넣는 프로브 명령(`drop` 처럼 게스트
   자신의 `NewBagItem` 을 부르는 것).
3. **세이브 왕복** — 120칸 가방을 저장 → 재부팅 → 같은 바이트로 다시 저장. 그리고
   옛 세이브(꼬리 없음)를 새 빌드로 열어 count·스택이 그대로인지.
4. 창고: 160칸 왕복(`warehouse_move.txt` 를 6페이지 이후 칸으로), 그리고
   원 서버 모드에서 용량이 서버 값(80)으로 남는지.

## 61. 정령 렌의 아바타 — 정령 페이지의 `*아바타` 와 렌의 옷장 (2026-10-15)

요구: 인게임 정령 메뉴에서도 캐릭터처럼 렌에게 아바타를 입히고, 사용자가 준
레퍼런스 10장(무기는 빼고)을 **매우 높은 퀄리티의 도트**로. 코드는
`hl5_core/src/spirit_avatar.rs`, 그림은 `costumes/ren_studio.py` +
`costumes/ren/rNN.py`, 상태는 `hl5_avatar.txt` 슬롯 줄의 `s=<id>`.
스위치는 캐릭터 아바타와 같은 `Config::avatar`(셸 on, 프로브 `--avatar`).

### 61.1 렌은 스프라이트 하나이고, 그 이름이 정해지는 곳도 하나다

`Battle::LoadSpiritRes`(0x4908C)가 `_szSprFile[0] = /c/sp/img0/042.mgr`(머리 6장),
`[1] = /c/sp/img1/025.mgr`(몸 46장), `[2..4] = empty.mgr`, `[5] = /c/sp/img5/021.mgr`
(렌의 이펙트·컷인)을 쓰고 `cif/065.cif` 로 `SPRITE_DATA`(`gv+0x1478`),
`SPRITE_IMAGE::LoadSprImg`(`+0x110`)로 이미지(`gv+0x147C`), `Spirit` 객체
(`gv+0x1484`)를 만든다. 둘째 `LoadSprImg`(`+0x248`)는 공용 감정 말풍선(`imo`)이다.

그 **한 스프라이트**를 세 곳이 그린다: 필드(`Battle::Paint` → `Spirit::Draw`),
정령 페이지(`DrawSpiritMenu+0x11FC`: `gv+0x1484` 의 `CHAR::GetSpritePtr` →
`SPRITE::DrawSprite(fb, x0+0x22, y0+0x57, 애니 0, 방향 2, 프레임)` — 방향 2 가 3/4
옆모습), 스킬 컷인. 그래서 이미지 둘만 바꾸면 셋이 같이 바뀐다.

정령 상태는 `HERO+0x1790`(`HERO::GetSpiritInfoPtr`)의 바이트다: 0 없음, 1·2 안
그림, 3 나옴. 새 게임에는 `Spirit` 객체가 있지만(`LoadSpiritRes` 는 전투마다 돈다)
상태가 0 이고 렌은 스토리 한참 뒤에 합류한다. 프로브 명령 `spirit 3` 이 게스트
자신의 `EventProc::Event_PlayerAppearSpirit`(0x6DFB0)으로 불러낸다.

### 61.2 입히기 — 두 길

* **로드 때**: `LoadSprImg` 진입 후크가 복귀주소가 `LoadSpiritRes+0x114` 일 때만
  `_szSprFile[0]`·`[1]` 을 입은 옷의 시트로 바꾸고 사양한다 — 게스트 로더가 읽는다.
* **고를 때**: 게스트 자신의 `SPRITE_IMAGE::ChangeSprImg(image, data, 층, 경로)`
  를 층 0·1 에 부른다. 이 함수는 **옛 시트가 정한 배열에 그대로 다시 읽어 넣는다**
  (옛 개수만큼 `DestroyImage` → `GetMergedGbmData`) — 그래서 렌의 의상은 이미지
  수를 바꾸지 않는다(단위시험). 그리고 그 함수가 읽는 `_nSprPalFile[층]` 은 그때쯤
  주인공 것이라 호출 동안만 −1 로 두고 되돌린다.

재생 확인: 한 재생에서 10벌을 차례로 입혀 `10 re-dress(es)`, 같은 저장소로 새
프로세스를 띄우면 옷장을 안 열어도 `1 load(s) dressed` 이고 필드의 렌이 그 옷이다.

### 61.3 버튼과 옷장

* `KeySpiritMenu`(0x10527C)는 기본/장비(하위 상태 0)에서 `key + 0x10` 의 0x44칸
  점프테이블로 가르고, `'*'`(0x2A)와 `'0'` 은 기본 분기(아무 일 없음)다. 그래서
  캐릭터 페이지와 같은 키가 연다. 버튼 자리도 같은 `(x0, y0+0xa2)` `0x45×0x11` —
  이 페이지에서는 장비 줄과 메뉴 아이콘 막대 사이의 빈 띠다. `TouchSpiritMenu` 에는
  그 띠의 판정이 없으므로 터치 후크가 직접 받는다.
* 옷장은 렌의 그림을 가리지 않는다: 오른쪽 열(`x0+0x3e..0xae`, `y0+0x2a..0xa0`)에
  여섯 줄 + 스크롤바, 렌 아래(`x0+2..0x3e`, `y0+0x5c..0xa0`)에 **같은 스프라이트로
  앞(방향 1)·뒤(방향 0)** 를 더 그린다 — 게스트의 그림이 옆이므로 세 방향을 한
  화면에서 본다.
* 드래그 스크롤은 §59 그대로이고 후크도 공유한다: `StateInGameMenu::TouchMoved`/
  `TouchReleased` 후크가 주인공 목록에 손가락이 없으면 렌 쪽으로 넘긴다. 누르면
  커서만, 떼면 입는다.
* 저장: 슬롯 줄 끝의 `s=<id>`. 옛 빌드는 `s` 가 층 번호가 아니라 그 쌍만 건너뛴다.
  같은 클래스 규칙(슬롯의 주인공이 바뀌면 렌도 벗는다).

### 61.4 렌의 도트 — 조각과 그리는 순서

* 머리 `img0/042`: 0 정면, 1 3/4(**화면 왼쪽을 본다** — 방향 2 가 그대로, 3 이
  뒤집어 쓴다), 2 다른 3/4(애니 4·37), 3 뒤. 4·5 는 cif 가 안 쓴다.
* 몸 `img1/025`: 팔 0·1(+ 변형 6·8, 든 팔 31·32), 몸통 33 앞·34 옆·35 뒤(턱 아래로
  **6×7 쯤만 보인다**), 하체 13–22·25–28·37–45(걷기·대기·시전·앉기·만세·소환),
  빛꼬리 29·30(cif 가 **불투명도 150/255** 이펙트로 그린다), 36 소환 용.
* 그리는 순서 — 앞: 꼬리29, 팔0, 하체, 몸통, 머리, 팔1, 꼬리30 / 뒤: 하체, 몸통,
  팔1, 팔0, 꼬리30, 꼬리29, 머리 / 옆: 꼬리29, 팔0, 하체, 몸통, 팔1, 머리, 꼬리30.
  정면에서 머리가 몸통 **위**에 그려지므로 긴 머리는 팔 바깥으로 내렸다.

### 61.5 그리는 법 — `costumes/ren_studio.py`

* **라벨**: 빌드 때 아카이브에서 픽셀마다 역할을 정한다 — 머리(머리칼·얼굴·귀·눈·
  꽃장식), 팔(소매·트림·보석·커프·장갑·손), 하체(치마·보라 패널·트림·안감·그늘·
  다리·발), 꼬리. **함정**: 부츠가 치마의 자주색을 하이라이트로, 무릎이 안감의 연회색을
  쓴다 — 치마는 보라·금이 있는 마지막 줄까지만 친다(안 그러면 다리 한 줄이 치마가 된다).
* 머리칼은 밝기 순위로 새 램프에, 꽃장식은 가장 가까운 머리칼 색으로(또는 유지).
* 손으로: 모자·귀·리본·백합·갈매기·버블 헬멧, 긴 머리 가닥(`kit.lock`), 뒷머리
  (`kit.curtain`, 가닥 묶음·흔들리는 가르마·들쭉날쭉한 끝), 깃털 날개(`kit.wing`,
  칼깃을 바깥부터 깔고 덮깃을 그 뿌리 위에), 몸통 세 방향 격자.
* 팔·다리는 영역 램프 — 원본 음영 순서가 남는다. `CUT`(레오타드), `ANKLET`.
* 긴 치마 `dress()` 는 **하체 프레임마다 그 프레임의 다리로** 모양을 정한다: 허리에서
  가장 낮은 발 바로 위까지, 종 모양으로 벌어지고, 왼쪽 위 조명, 아래로 퍼지는 주름,
  프릴 단·밑단·트임·앞트임(뒤 프레임 20·21·22·26·39·42·45 에는 없다)·별·길이.
  그래서 걷기·앉기·만세에서도 치마가 다리를 따라간다.
* 게임 픽셀은 저장소에 없다: `sets/ren_NN_L.py` 는 오버레이이고 런타임이 합성한다.
  id 는 `130000 + n×10 + 층`, 시트는 `img0/32n`·`img1/32n`(321–330). 120xxx 는
  아스트라 이펙트가 쓰고 있었다.

### 61.6 열 벌

| # | 이름 | 핵심 |
|---|---|---|
| 01 | 이브닝 드레스 | 금발 웨이브 포니테일·청록 귀걸이, 흰 뷔스티에+청록 보석, 흰 롱장갑, 흰 롱드레스+검은 프릴 단·트임, 검은 부츠 |
| 02 | 바니걸 | 검은 토끼귀(한쪽 접힘), 금발 사이드 포니테일+파란 슈슈, 칼라·나비넥타이, 하이컷 레오타드, 털 커프스, 맨다리 |
| 03 | 장교복 | 정모(금 휘장·청록 보석), 회백 머리, 남색 더블 코트·금 단추·견장, 흰 셔츠·검은 넥타이, 검은 롱부츠 |
| 04 | 여신 | 긴 흑발, 백합+라벤더 리본, 초록 보석, 흰 상의·은 허리띠, 금빛 베일, 맨발 |
| 05 | 눈사람 | 머리 위 갈매기, 빨간 리본, 줄무늬 목도리, 달걀형 눈사람 몸(구 음영), 곰발 장갑, 짧은 흰 다리 |
| 06 | 별빛 망토 | 유리 버블 헬멧, 연보라 백발, 분홍 칼라+주황 리본, 금별 남색 대형 망토, 흰 드레스·타이츠 |
| 07 | 천사 | 깃털 날개(빛꼬리를 다시 그림), 관자놀이 깃털, 금발 하이포니테일, 흑금 홀터, 별 박힌 검은 롱스커트, 금 샌들 |
| 08 | 마녀 | 끝이 휜 마녀 모자+붉은 무늬 띠, 크림색 장발, 검은 로브+붉은 안감·흰 트림, 청록 브로치 |
| 09 | 고딕 드레스 | 버건디 리본, 부채꼴 장발+무지갯빛, 금 자수 검은 드레스, 붉은 안감 트임, 금 밑단 |
| 10 | 여름 나들이 | 파란 리본 흰 챙모자, 붉은 머리, 분홍 비키니, 빨간 줄 검은 무릎치마, 흰 프릴 발찌, 오렌지 바구니 |

빛꼬리는 렌의 표지라 대부분 남기고 옷 색으로 물들였다(눈사람만 숨김, 천사는
날개로 다시 그림). 불투명도는 cif 가 정하므로 짙은 색은 탁해진다 — 꼬리 램프는
밝게 둔다.

### 61.7 인수

* `cargo test -p hl5_core --lib` 230 — 렌 의상은 렌의 두 시트 위에만, 머리·몸 쌍,
  이미지 수 유지·덧붙임 없음; 빛꼬리 둘만 비울 수 있다; `s=` 왕복; 옷장 배치.
* `tools_re/scripts/spirit-avatar.txt`(`--avatar`): 버튼 → 열기 → 1행 입기(떼는
  순간) → 필드. 10벌 쇼케이스 재생 `10 re-dress(es)`, 파일 `0 0 s=130100`. 새
  프로세스 `1 load(s) dressed`. 폴트 0.
* 아바타를 끈 기본 플래그로 HEAD(2553056) 대비 1,241 프레임 5장면 **전부 바이트 동일**
  (`git worktree` + `ab_905.sh renhead new` / `rencur new` + `ab_compare.py`).
* 기기 미시험. 안 한 것: 스토리 컷신의 렌(NPC 로 따로 그린다)은 안 입는다, 획득 단계
  없이 전부 열려 있다, 빛꼬리의 불투명도는 cif 그대로다.

## 62. 모자는 얹는 것이 아니라 쓰는 것 — 챙은 이마까지 (2026-10-15)

사용자 보고: 캐릭터와 렌 둘 다 모자 아바타가 머리 꼭대기에 **얹힌** 느낌이다.
원인은 같았다 — 챙이 머리칼 부피의 **위**에 있었다. 주인공 머리(`img0/000`)는
1~12행이 머리칼이고 이마(피부)가 10행, 눈이 11~13행인데, 마법사 모자·산타 모자의
챙 밑단이 5~7행이어서 그 아래로 흰 앞머리와 옆머리가 챙만큼 넓게 부풀어 나왔다.
산타 모자 털 띠는 머리보다 왼쪽으로 1 px 치우쳐 오른쪽 머리칼이 삐져나왔다.

**쓴 모자의 규칙** (`costumes/hats.py` 가 주인공 모자 다섯에 적용하고 세트를 쓴다):

* 챙 밑단 = 이마 바로 위 행 — 정면·3/4·옆·뒤 8행, 내려다보는 6번 뷰 10행. 챙 아래로는
  앞머리만 보인다.
* 가운데 맞춤은 **그 뷰의** 머리 폭으로(챙 행에서 머리칼의 양 끝) — 3/4 뷰는 정면과
  중심이 다르다.
* 챙 위의 머리칼은 지운다(`clear_above`) — 모자이거나 아무것도 아니다.
* 챙 바로 아래 한 줄은 머리칼을 **제 램프를 따라 한 단계 어둡게** — 챙의 그림자.
  얼굴·눈 픽셀은 안 건드린다(`astra_studio.face_mask`, `EYES_BASE`).
* 왕관·고양이귀처럼 챙이 없는 것은 머리칼 **속으로** 앉힌다(제 기준 행, 맨 위 뾰족한
  머리칼만 지움).

같은 규칙을 다른 모자에도: 아스트라 07·08 장교모 격자를 2행 내렸고(`astra/a07.py`,
`a08.py` → `astra_studio.py build 7`/`8`), 렌은 장교모 2행·마녀 모자 3행·챙모자 2행을
내리고 바니 귀를 2행 머리칼에 묻었으며, `HAT_SHADOW` {뷰: (행, x0, x1)} 로 챙 그림자를
렌의 머리칼 램프에서 한 단계 어둡게 넣는다(`ren_studio.head_canvas`). 삿갓(02·09)·
투구·후드는 이미 머리를 덮어서 그대로 뒀다.

인수: 단위시험 230, 주인공 재생(`avatar.txt`) `3 change(s)` 와 파일
`0 0 0=100001 1=110011 4=110024` 그대로, 아스트라 워리어 재생 + `astra_verify.py`
**143/143 시트 런타임 픽셀 동일**, 렌 쇼케이스 `10 re-dress(es)`. 아바타를 끈 경로는
건드리지 않았다(합성 표만 바뀌었고 그 표는 `cfg.avatar` 일 때만 읽힌다).

## 63. 아바타 품질 v4 — 게임 자신의 머리를 이식하고, 옷감에 재질을 준다 (2026-10-15)

사용자: "전체 아바타 퀄리티 좀 더 올려줘. 기존 게임 디자인에 비해 많이 떨어진다."

### 63.1 진단 — 원작 옆에 세워 보니 차이는 셋

원작 세트 외형(투구 `img0/108..123` + 갑옷 `img1`)을 입힌 워리어와 아스트라 11종,
원작 렌과 렌 10벌을 같은 배율로 나란히 놓았다.

1. **머리칼.** 원작 머리칼은 덩어리(클럼프)마다 빛 받는 면에서 그늘 면으로 도는 5~7단
   램프이고 픽셀마다 한 단씩 바뀐다(`img0/084`, `120` 갈기). 우리 손그림 머리는 3~4단
   면 채우기라 아이콘처럼 읽혔다. 원작보다 크기도 했다(01 의 불꽃머리 폭).
2. **큰 단색 면.** 08 붉은 코트 뒷면, 11 후드, 10 멧돼지 탈, 렌 뒷머리는 판이었다.
   원작 긴 치마(`img3/023`)는 3 px 간격의 주름 기둥(`858858858`)을 갖는다.
3. **어두운 옷감은 음영을 아예 못 받았다** — 아래 63.4.

그리고 **아카이브에 원작 화풍 머리가 이미 있다.** `img0` 127장 중 주인공과 같은
7장 배치(0 앞 · 1 3/4 · 3 뒤 · 4 옆 · 6 내려봄)가 이야기 인물 머리 ~25장과 투구·모자
16장이다: `001` 금발 뾰족, `016` 은발, `027` 긴 웨이브, `082` 은 투구, `109` 마법사
모자, `113` 장교모, `117` 챙 넓은 모자, `120` 갈기 투구 …

### 63.2 이식 — `costumes/donor.py`

디자인이 `DONOR = {'sheet': N, 'hair'|'groups': 색 → 램프}` 를 두면 그 머리의 머리칼
(또는 모자)을 디자인 램프로 재배색해 기본 머리 위에 깐다. 얼굴은 기본 것 그대로다.

* **자리는 게임의 것이다.** cif 는 머리 이미지를 제 중심으로 그리므로, 이식할 그림을
  중심 맞춤하면 게임이 그 인물의 머리를 그릴 바로 그 자리다.
* **재배색은 톤 간격을 지킨다.** 각 색의 "이식 원본 휘도 범위 안 위치"를 디자인 램프
  위에서 보간한다 — 원작의 두 톤이 하나로 합쳐지지 않고, 가장 어두운 톤은 검정이 아니라
  틴트된 어둠으로 남는다.
* **함정 1: 가장 어두운 머리칼 톤 = 얼굴 윤곽색.** `001` 의 `#733839` 는 둘 다다.
  원본 피부에 4방향으로 닿은 어두운 색은 얼굴 윤곽으로 보고 뺀다(`skin`).
* **함정 2: 모자 시트에는 주인공이 같이 그려져 있다.** `113` 의 크라운 밑으로 원작
  주인공의 청록·흰 머리가 보인다. 그래서 모자는 색 그룹(크라운/띠/금장/휘장/밑머리)으로
  나눠 따로 재배색한다.

주인공: 01 ← `001`(주황), 04 ← `082`(은 + 금, 손그림 투구 대체), 05 ← `027`(흰색, 아래로
이어지는 타래·커튼은 63.3 의 커튼), 06 ← `016`(흰색), 07 ← `113`(남색 + 금),
08 ← `113`(검정 + 금 + 붉은 휘장). 03 은 원래 원작 머리를 재배색하고 있었다 — 뒷머리
(원작 청록)만 램프를 밝혔다.

렌: 모자 셋 — 03 장교모 `113`, 08 마법사 모자 `109`, 10 챙모자 `117`. 렌의 머리는
주인공 머리가 아니므로 중심 맞춤 대신 **`fit_hat`**: 모자 챙(가장 넓은 줄)의 가운데를
렌의 `HAT_SHADOW` 행 바로 위에 앉히고, 챙 위로 삐져나온 렌의 머리칼·장식을 지운다.

* 모자 바닥 = 챙에서 내려가며 폭이 챙의 60% 이상인 마지막 줄 — **단 그 모자가 그려진
  주인공 얼굴의 이마 + 1 을 넘지 않는다.** `109` 의 3/4 뷰는 모자 그늘 속 뒷머리가
  모자 색이라 폭 규칙만으로는 바닥이 18행까지 내려가 모자가 떴다.
* `109` 의 금장 몇 색은 주인공 얼굴 하이라이트와 같은 색이다 → 주인공 맨머리
  (`img0/000`)에서 얼굴인 자리에서는 그 색을 얼굴로 본다(`face_cols`).

배치·색 매핑은 설계 파일에, 결과는 팔·다리 재배색(v2~)과 같은 방식으로 격자에 들어간다.

### 63.3 재질 — `costumes/finish.py` 와 페인터들

| 디자인 키 / 함수 | 무엇 | 쓰는 곳 |
|---|---|---|
| `FOLDS` | 옷감: 3~4 px 간격 주름 기둥(밝은 능선 + 오른쪽 그늘 골)이 면의 1/3 지점부터 밑단까지. 간격은 면의 왼쪽 위에서 재므로 같은 면은 프레임마다 같게 접힌다 | 아스트라 01·02·03·04·05·06·07·08·09·11 |
| `PELT` | 짐승 털: 2~3 px 짧은 획, 열마다 어긋나게 | 10 멧돼지 |
| `NOFORM` | 이미 칠한 것(원뿔, 눈사람 몸, 깃털, 유리)은 마감이 톤을 안 바꾼다 | |
| `astra/kit.fur` | 털 목도리: 2×3 털뭉치를 벽돌처럼, 위가 밝고 사이가 그늘 | 03·05·06 |
| `astra/kit.cone` | 원뿔 삿갓 7단: 꼭지에서 내려오는 방사 살결(뷰마다 위상이 돈다), 3행마다 엮음 고리 | 02·09 |
| `ren/kit.curtain` v3 | 긴 머리: 흔들리는 가르마로 나뉜 덩어리, 덩어리마다 왼쪽 밝음·오른쪽 그늘·사이 어둠, 정수리 아래 광택 띠, 아래 1/4 어둡게, 가운데 아래로 좁아짐(5 px 주기 줄무늬였다) | 렌 04·08·09, 아스트라 05 |
| 렌 06 `bubble` | 유리: 테 4단 + 빛 쪽 안쪽 반사 호 + 반대편 희미한 호 | |
| 모자 다섯(`hats.py`) | 마감 패스 + 펠트 주름(마법사·산타), 털(산타 테), 짚 결(밀짚) | |

**순서를 바꿨다: 마감 → 리그 포즈.** 포즈가 움직일 픽셀은 마감 전에 색으로 찾아
두고(`loose_points`), 포즈는 마감된 그림을 옮긴다 — 주름이 옷자락과 함께 흔들린다.

### 63.4 마감 패스의 버그 둘

* **흰 옷이 검게 접혔다.** 무채색 계열이 흰색과 검정(눈사람 단추)을 한 램프로 묶어,
  흰 면을 한 단계 어둡게 하면 검정으로 떨어졌다 — 렌 05 몸통의 검은 사선. 휘도가
  64 이상 벌어진 곳에서 램프를 끊는다(`RAMP_GAP`, `ramp_of`).
* **어두운 옷감은 면이 아니었다.** 휘도 44 미만인 우리 색은 전부 외곽선으로 처리돼
  검은 후드·남색 코트는 음영도 주름도 못 받았다. 4방향이 다 같은 색인 픽셀이 넷 이상인
  면은 어두워도 면이고, 외곽선은 그 밖의 어두운 픽셀이다.

### 63.5 인수

* `cargo test -p hl5_core --lib` 230.
* 아스트라: 워리어 재생 + `astra_verify.py out/v4-warrior` **143/143 시트 런타임 픽셀
  동일**, 리그 CIF 12개; 로그·건슬링어·나이트 재생 폴트 0, 캡처 모음 갱신.
* 렌: 쇼케이스 재생의 저장소에서 **20/20 시트 런타임 픽셀 동일**, 옷장 캡처 갱신;
  `spirit-avatar.txt` `opened 1x, 1 re-dress(es)`, 파일 `s=130010`.
* 주인공 재생(`avatar.txt`) `3 change(s)`, 파일 `0 0 0=100001 1=110011 4=110024`.
* 팔·다리 시트 22개는 글자 배정만 바뀌고 픽셀은 HEAD 와 같다.
* 비교: `docs/astra_avatar/native/before_after_v4.png`(디자인별 이전 | 이후),
  `overview_v3_vs_v4.png`, `docs/ren_avatar/native/overview_before_after.png`,
  `hats_hero_finish_before_after.png`.

남은 것: 렌 07 날개는 cif 가 59% 불투명으로 그려 어두운 바탕에서 회색으로 읽힌다(대비를
넓혔지만 그 이상은 cif 의 몫); 렌 02 다리의 정의; 기기 확인.

## 64. 렌의 모자는 머리를 감싸야 한다 — 챙은 속눈썹 줄, 크라운은 렌의 두개골 높이 (2026-10-15)

사용자: v4 의 렌 모자(03 장교모 `113`, 08 마법사 모자 `109`, 10 챙모자 `117`)가
아직 머리 꼭대기에 얹힌 느낌이다.

**원인 둘, 둘 다 숫자로 잰 것이다.**

1. **챙이 높았다.** 게임은 모자를 주인공에게 그린다. 주인공 맨머리(`img0/000`)는
   피부 첫 줄이 10행, 속눈썹이 11행이고, `113` 의 챙 밑단은 주인공 좌표로
   **11행 = 속눈썹 줄**이다 — 게임 모자는 이마를 덮고 눈썹까지 내려온다. §63 의
   `fit_hat` 은 렌의 챙을 7행(옛 `HAT_SHADOW` 행 − 1)에 앉혔는데 렌의 얼굴 첫 줄은
   정면 8 · 3/4 7 이라, 챙 밑으로 앞머리와 이마가 2~3줄 드러났다.
2. **크라운이 주인공 두개골 높이였다.** 주인공은 1행에서 챙까지 10행, 렌은
   2행(바보털 아래 첫 머리 줄)에서 7~8행이다. 같은 모자를 그대로 쓰면 크라운이
   렌 머리 꼭대기보다 4~5행 위로 솟는다 — "얹힌" 느낌의 대부분이 이것이다.

**고침** (`ren_studio.hat_fit` + `donor.fit_hat(head_top=…)`):

* 챙 밑단 = 렌 얼굴의 첫 줄 + 1 (정면 9, 3/4 8; 뒤는 얼굴이 없어 정면 값 9) —
  주인공과 같은 관계. 가운데는 그 줄의 머리+얼굴 폭.
* 크라운은 두 두개골의 비 `(챙 − 렌 머리 꼭대기) / (주인공 챙 − 1)` 만큼 줄인다.
  줄이는 방법은 **행 빼기** — 크라운 가운데에서 고르게, 둥근 꼭대기 두 줄과 챙은
  안 건드린다(재표본은 픽셀아트를 뭉갠다). 마법사 모자는 원뿔이 키가 곧 모양이라
  `squash` 로 두 줄 덜 뺀다.
* 챙 그림자 줄은 이제 계산한다: 챙 바로 아래 줄, 그 줄에 깔린 모자의 폭만큼.
  세 디자인의 `HAT_SHADOW` 고정 좌표는 지웠다.

인수: 단위시험 230, `spirit-avatar.txt` 재생 `opened 1x, 1 re-dress(es)`·파일
`s=130010`, 렌 **20/20 시트 런타임 픽셀 동일**, 쇼케이스 재생으로 옷장 캡처 갱신.
비교: `docs/ren_avatar/native/hats_ren_worn_before_after.png`.

## 65. 가방·창고 10페이지 — 구현 (2026-10-15)

§60 의 조사를 코드로 옮겼다. `Config::ten_pages`(기본 off, 셸 on) 하나 뒤에
**창고 160칸 + 가방 160칸(넘침 12칸 포함 배열 172)** 이 있다. 모듈은
`hl5_core/src/tenpages.rs`, 호스트 창고는 `warehouse.rs`.

| | 창고 | 가방 |
|---|---|---|
| 게스트 워드 재작성 | 21 + 수신 버퍼 1 (65.14) | 186 + 목록 확대 412 (65.11, 65.13) |
| 후크 | 탭 그리기·터치·키 | 탭 그리기·터치·키 ×2 화면, 세이브 꼬리 2 |
| 세이브 | 호스트 파일 둘(`hl5_warehouse.txt` + `_upper.txt`), 세대 번호로 함께 커밋 (65.12) | 게스트 세이브 끝에 80바이트 꼬리 |

### 65.1 전수의 방법 — 그리고 이번에 걸린 함정

1. **데이터흐름 분석** `tools_re/bagscan.py` — 함수마다 CFG 위에서 레지스터·스택
   슬롯의 *가능한 값 집합*을 굴린다. 값은 상수 / 포인터(HERO·BAG·gv·진입 sp +
   오프셋) / 색인 포인터 / 바이어스 색인이고, 오프셋마다 그것을 만든 명령어
   주소(출처)를 단다. 출처: HERO 계열·`BagItem` 메서드의 `this`,
   `HERO::GetBagItemPtr` 반환, GOT 의 `gv`, `gv+0x1474`. 합류점에서 같은 기준의
   오프셋이 갈리면 "그 배열 어딘가"로 넓힌다(루프가 끝난다).
   `tools_re/bagplan.py` 가 스택·필터 접근마다 바꿀 즉치를 고르고, 같은 즉치가
   다른 영역 접근에도 쓰이면 경고한다(0건).
2. **즉치 전수** `tools_re/immscan.py` — 위의 결과와 대조해 빠진 것을 찾는다.
3. **실행 중 검증** `--memwatch storage,bag`(`hl5_core::memwatch`) — 인터프리터가
   객체 영역에 닿는 load/store 를 PC 별로 기록하고, `tenpages::check_accesses`
   가 새 배치로 판정한다(포인터 영역의 바이트 접근·스택 영역의 워드 접근·틈 =
   위반, 포인터 배열 꼬리를 `+0x1e8` 부터 읽는 워드 접근 = 옛 필터 의심).

**함정: capstone 은 해석 못 하는 워드에서 디스어셈블을 멈춘다.** 큰 함수 한가운데
리터럴 풀이 있으면 그 뒤가 통째로 빠지고, 즉치 전수도 데이터흐름도 **조용히**
짧아진다. `DrawEquipMenu`(8,376 B)의 필터 접근 7곳이 이렇게 빠졌고, 실행 검증이
`+0x1e8` 의심으로 잡은 뒤 `ItemInfo::DrawIcon` 폴트로 확인됐다. `skipdata`
를 켜고 다시 돌리자 20워드가 더 나왔다(107 → 127). `disasm.py`·`immscan.py`·
`bagscan.py` 셋 다 고쳤다. **이전의 어떤 "전수 0곳"도 그 함수가 끝까지
해석됐는지 확인하기 전에는 믿지 마라.**

분석기 자체의 함정 둘도 고쳤다: `mov lr, pc; ldr pc, [rX, #n]`(가상 호출)을
함수 출구로 보면 그 뒤가 도달 불가가 된다(`ChangeHeroClass` 의 복원 루프가 이렇게
빠졌다), 그리고 색인 포인터 + 미지수를 놓치면 `StateNetMenu::Process+0xd70`
같은 `GetBagItemPtr + page*16 + cell` 이 빠진다.

**셋째 함정 (65.11 에서 발견): `stmib sp, {…}`(스택 인자, 쓰기 되돌림 없음)가
`sp` 를 지웠다.** 나머지 경우를 받는 갈래가 "첫 피연산자에 쓴다"로 처리해서,
스택 인자를 넘기는 호출 뒤로는 스택에 넣었다 꺼낸 포인터·상수를 전부 잃었다.
고친 뒤 전 함수를 다시 돌려 가방 배열(스택 수·필터) 접근을 대조했다 — **새로
드러난 것 0곳**(개수 하위 바이트 읽기 하나뿐이고 개수는 옮기지 않았다). 그러니
186워드 표는 이 버그의 영향을 받지 않았다. 실행 검증(위반 0)과 같은 결론이다.

### 65.2 창고 — 21워드

`StateNetMenu` 생성자 넷의 `new(0x1E8)` → `0x3C8`, 그리고 배치
`ptr[160] +0x008 / stack[160] +0x288 / state[160] +0x328`. 바뀌는 것은 생성자
memset 둘, 게터 둘, `DeleteNetStorageItemPosIndex`·`DeleteAll`,
`NewNetStorageItem` 의 출구 셋(각 스택·상태), `DrawStorageUI` 의 스택 포인터.
`#0x148` 의 나머지(`NewNetStorageItem+0x1b8`, `writeNetItem`, `TouchBuy` …)는
`ItemInfo`·프레임·다른 구조체였다.

목록(4/6)은 칸 번호를 와이어에서 받고 개수를 `SetSize` 로 쓰므로 호스트가 160칸을
보내면 끝이다. **호스트 파일은 둘로 나눴다** — 80칸 이후는
`hl5_warehouse_upper.txt`, 원래 파일은 영원히 `capacity 80`. 옛 빌드는
`hl5_warehouse.txt` 의 80 넘는 `slot` 을 "past capacity" 로 버리고 다음 입고에
그 파일을 다시 쓰므로, 한 파일이면 옛 APK 로 되돌리는 순간 6~10페이지가
**지워진다**. 두 파일이면 숨을 뿐이다. 기능을 끈 실행(80칸 제시)도 위쪽 칸을
잃지 않고, `아이템정리`(4/9)는 전체를 당겨 빈자리로 끌어내린다.

### 65.3 가방 — 186워드

* **스택 수** `+0x188 → +0x2B8`, **필터** `+0x1E8 → HERO+0x2020`(`bag+0x860`).
  포인터 배열은 제자리에서 172칸. 분석기 127워드 + 생성자(용량 0x30→0xA0,
  memset 0x180→0x2B0·0x60→0xAC, 주소 둘) + 소멸자·루프 상한(`#0x60`→`#0xAC`,
  `#0x5e/#0x5f`→`#0xAA/#0xAB`, 시프트된 `#0x600000`→`#0xAC0000` 둘
  (`DeleteQuestItemAll`), **마지막 칸을 비우는 `add fp, rX, #0x5e`** 둘) +
  넘침 스캔 `movne r8, #0x60` 둘 + `LoadBagData`/`LoadHeroData` 의 비우기 루프 +
  꼬리 길이 둘 + HERO 할당 16곳(`0x2018/0x2020` → `0x22D0`, 4직업 × LoadHero·
  ChangeHeroClass·NewGame·StatePopup) + `ChangeHeroClass` 7.
* **`ChangeHeroClass` 는 가방을 프레임 안의 지역 배열로 복사한다** — 포인터
  96개 `sp+0x270`, 스택 수 96바이트 `sp+0x3f0`. 프레임을 `0x464 → 0x7F0` 로 키우고
  두 배열을 `sp+0x480`(0x2B0)·`sp+0x740`(0xAC)로 옮겼다. 그 주소를 만드는 곳이
  **셋**(저장·복원·정리 루프)이고 셋째를 처음에 놓쳐 `ItemTable::DeleteItem`
  폴트가 났다. 스택 인자가 없는 함수라 에필로그 `add sp, #0x64` 하나만 같이 바뀐다.
* **용량은 모두 160.** 생성자가 160 을 쓰고, 로드 후크가 세이브의 48~80 을 160 으로
  덮는다. `창고 확장` 아이템 사용(`StateInGameMenu::Process+0x16d4`, §60 이
  "SetSize 호출자 0"이라 적은 것은 틀렸다)과 구매 관문은 둘 다 `> 0x4F` 에서 거절하므로
  배열을 넘을 길이 없다.

### 65.4 세이브 꼬리

`SaveBagData` 는 레코드 뒤에 스택 수를 **고정 96바이트**(`cmp r3, #0x60`) 쓰고
53바이트(장착 인덱스 8개·핫키)로 끝나며, `LoadBagData` 는 길이를 보지 않고 앞에서
읽는다. 그래서 형식을 바꾸지 않고 끝에 80바이트 — `BG10` + 칸 96..171 의 스택 수
76 — 를 붙였다. `GetBagDataSize` 와 `SaveEncryptFile` 에 넘기는 길이가 각각 +0x50,
저장 후크(`SaveBagData+0x3e4`, `bl SaveEncryptFile` 직전)가 버퍼 끝을 채우고,
로드 후크(`LoadBagData+0x510`, 버퍼 해제 직전)가 `fp` 로 본문 끝을 알아 꼬리를
읽는다. 꼬리는 게스트 자신의 버퍼 안이라 게스트 DES·MD5 가 그대로 덮는다
(`SaveEncryptFile` 은 8바이트 패딩까지 MD5 하고 길이를 16비트로 자른다 — 가방
최대 ≈ 7.9 KB). 옛 세이브는 꼬리가 없고, 로드 후크가 96 이후를 0 으로 둔다.

### 65.5 탭 — 게임의 가방 아이콘 두 열, 그리드는 12 px 오른쪽 (사용자 선택)

> **2026-10-15 개정.** 처음에는 숫자 칩(`1`~`10`, `DrawHalfBox`+글꼴)이었다.
> 사용자가 **원래 가방 아이콘을 그대로** 쓰고, 그 자리를 위해 **아이템 그리드를
> 오른쪽으로 미는** 쪽을 골랐다(싱글·네트워크 둘 다). 아래가 지금 모양이다.

네 화면이 같은 탭 열을 그린다: 창고·창고의 가방 보기(`DrawStorageUI`), 인게임
가방(`DrawBagMenu`), NPC 판매(`DrawItemSale`). 전부 상자 `i` 를
`(x0+13, y0+0x3d+24i)`, 터치 사각형을 `(x0+13, y0+40+24i)` 21px 에 두므로
블릿 좌표 하나가 열의 원점이다. 판은 176 px, 상자는 열린 것 24 px·닫힌 것 21 px
(블릿 `x` 부터, 아래끝이 블릿 `y` 한 줄 위), 그리드 칸 상자는 `x0+44` 부터 26 px,
29 간격 — 그래서 두 열(24+24)을 넣으려면 그리드가 움직여야 한다.

* **배치**: 상자 열 `x0+4` / `x0+28`(행은 그대로 24 간격), 그리드 `x0+44 → x0+56`
  (`tenpages::GRID_SHIFT` = 12). 오른쪽 열의 열린 상자와 그리드 사이 4 px, 그리드
  오른끝과 판 사이 7 px(왼쪽 상자는 판에서 4 px).
* **그리기**: 탭 루프의 `DrawImage`(현재·나머지)와 `DrawImageBlending`(그리드
  포커스일 때 흐린 나머지)를 복귀주소로 가려 **블릿의 `x`·`y`(r2·r3)만 옮기고
  사양한다** — 그림·프레임·흐림은 게스트 그대로. `DrawImage` 는 창고의 코인 숨김과
  공유라 `Vm` 의 디스패처(`draw_image`)가 둘에게 차례로 준다.
* **그리드 그리기 — 23워드** (`tenpages::GRID`). 그리드의 모든 `x` 가 판 원점 +
  즉치다: 가방 둘은 `halfW - 0x1f`(아이콘)·`halfW - 0x2b`(칸 상자, 여기서 `e`·핫키
  표시·스택 수·커서 상자가 나온다)·`x0 + 0x2c`(`#` 힌트)·`x0 + 0x2f`(이름 툴팁,
  왼쪽·오른쪽 정렬 둘) 다섯씩, 창고는 `[this+0x18]` 에 더하는 열셋(툴팁 패스 다섯 +
  그리드 패스 여덟: 칸 상자·아이콘 둘·스택 수·상태 표시·커서 상자·`e`·핫키). 헤더·
  `0.아이템정리`·자동 정보창은 다른 즉치나 화면 끝에서 오므로 안 움직인다.
* **그리드 터치 — 표 셋**. `TouchBagMenu`·`TouchItemSale`·`TouchStorageUI` 는 칸
  `x` 를 자기 `.rodata` 표 16개(`0x15A0A8`·`0x15A4DC`·`0x15A844`, 값
  44/73/102/131 ×4)에서 **스택으로 복사해** 판정한다. 복사가 끝난 첫 명령어
  (`+0x234`·`+0xd0`·`+0xf4`)의 관측 후크가 스택 사본이 출하값 그대로일 때만
  +12 하고 사양한다. 탭 열(`x < x0+56`)의 누름은 아래 터치 후크가 먼저 받는다.
* **증명**: 같은 바이너리·같은 키 재생을 이 변경 전후로 찍어 그리드 영역을
  12 px 어긋나게 대조했다 — 인게임 가방 30프레임은 판 배경 무늬(움직이지 않는
  `(16,28/32/36,41)` 셋)와 **탭 위치에 그려지는 터치 물결**(화면 고정)만 다르고,
  창고·창고 가방 61프레임은 밝은 픽셀(아이콘·숫자·커서·툴팁·표시) 전부 동일
  (판이 반투명이라 칸 사이로 보이는 타이틀 배경만 다르다). 터치는
  `tools_re/scripts/tenpages-grid.txt`(판매·가방)와 `tenpages-warehouse.txt`(창고·
  창고 가방)가 옮긴 칸 넷(출하 그리드가 안 닿는 `x0+165` 포함)과 오른쪽 열 상자를
  눌러 커서·페이지가 그 자리로 가는 것을 찍는다. 세로(240×320)도 같다.
* **터치**: 다섯 칸 `.rodata` 표와 `cmp #5` 를 지나기 전에, 탭 열(`x0..x0+56`,
  `y0+38..y0+158`) 안의 누름을 게스트 탭 루프가 하는 일 그대로 처리한다(페이지
  바꾸면 SubState·셀 0·`gv+0x198B`). 상자 하나의 터치 칸은 24×24 — 행 간격 전부.
  화면마다 다른 것: 인게임 가방은 SubState 0/1 에서만 판정, NPC 판매는 늘 판정하고
  탭을 누르면 늘 `gv+0x198B` 를 지운다.
* **키**: 위/아래는 원래대로 1→10 순환(전부 `size/16` 기준). 탭 열에서 왼쪽/오른쪽만
  열을 건너고, 오른쪽 열에서 오른쪽이면 원래대로 그리드.

### 65.6 되돌리기 경고 — 사람에게 먼저 말해야 하는 것

* **96개 넘게 든 가방의 세이브는 이 기능이 없는 빌드로 열면 안 된다.** 옛
  `LoadBagData` 가 96칸 배열 너머로 쓴다. 꼬리로 막을 수 없다. 96개 이하면 옛 빌드도
  연다(꼬리는 무시, 용량은 저장된 160 — 옛 빌드에서 97번째부터 넣으면 그 빌드가 넘친다).
* 창고는 반대로 안전하다(65.2): 옛 빌드에서는 6~10페이지가 안 보일 뿐이다.

### 65.7 필터 목록 — 127 에서 자르던 것은 철회했다 (65.11)

정련·오브결합·도감·장비·정령·스킬·가방 선택기 목록의 커서가 **부호 있는 바이트**다
(`KeyRefine` 의 `ldrsb [this,#0x11]` + `tst #0x80` 순환). 96칸 시절엔 목록이 96을 못
넘었지만 172칸이면 넘고, 실측으로 정련 띠가 128번째에서 비고 커서가 돌아오지 않았다.
처음에는 `SetItemFilter` 의 길이 저장 직전에 127 로 눌렀는데, **리뷰가 지적한 대로
그러면 128번째 이후 아이템은 그 화면에서 영영 고를 수 없다**(가방이 160칸이라 평범하게
생긴다). 그 후크는 지웠고, 커서를 넓혔다 — 65.11.

### 65.8 §60 정정

* `SetSize` 호출자는 있다 — 확장 아이템 사용 (65.3).
* `DrawStorageUI` 의 `cmp #5` 두 곳(+0x118, +0xB54)은 탭이 아니라 아이템 종류
  비교와 핫키 벨트(5칸×2) 루프다. 탭 상한은 그리기에 없고(`size/16`) 터치 루프에만
  있다(`cmp sl, #5`).
* §60.5 의 `#0x188` 후보 대부분(`PickUpMapItem`, `IsOrbCombinablePart`,
  `DrawSpiritMenu`, `KeySkillMenu` …)은 스택 프레임·리터럴 풀·HERO 능력치였다.
* 세이브를 만들 때 `npcmenu.txt` 는 저장하지 않는다 — `ingame.txt` 의 저장 탭이
  있어야 창고접속의 슬롯이 찬다.

### 65.9 인수 결과 (오프라인)

* 기능 off: 1,241 프레임 patched 대 `--no-patch` 동일. 기능 on: 목록 확대 표
  적용 대 미적용 1,241 프레임 동일(65.11).
* 단위시험 243: `tenpages` 7(표 워드 186+21 이 각각 옛 즉치를 갖고 새 즉치로
  인코딩되는지, 겹침 없음, 배치 크기, 탭 기하, 열 이동), `tenlists` 3(겹침·중복 없음,
  412워드(처음 414, 65.13)가 문서의 여섯 모양 중 하나이고 그 밖은 안 바뀜, 센티넬 비교마다 뒤집힌 분기),
  `warehouse` 두 파일 시험 2 + 커밋 시험 3(65.12), `memwatch` 1.
* 시나리오(`--ten-pages --memwatch bag,storage`, 위반 0):
  창고 160칸 목록·탭 10개 터치/키·8페이지 출고 왕복; 가방 160칸 채우기 → 저장 →
  재부팅 → **바이트 동일**(스택 수 3~61 의 오브가 104~118 칸); 인게임 가방 10탭·키;
  아이템정리(160칸, 집합 보존); NPC 판매 10탭; 정련·오브결합·장비·소켓확장 목록
  128 이후 선택과 적용(65.11); 무기 목록에서 가방 인덱스 148 장착 → 저장·재부팅 유지; 직업 변경
  (160칸, 집합 보존, 저장·재부팅 동일); 가방 10페이지 → 창고 입고(장착 표시가 한 칸
  당겨진 자리를 따라감); 넘침 172칸 저장·재부팅 동일; 옛 세이브 → 용량만 160.
* 프로브 도구: `--ten-pages`, `--memwatch storage,bag,lists`, 스크립트 명령
  `give <kind> <index> [n] [flag]`, `money <n>`, `bagdump [file]`(장착 인덱스·
  제련도·소켓·필터 목록·`gv+0x1B3E` 포함),
  `class <0..3>`(`ChangeHeroClass` 직접 호출 — 이 함수는 3번째 명령어에서 `r0` 를
  덮어써 `this` 를 안 읽는다), `tools_re/whfill.py`(10페이지 창고 파일 생성).

### 65.10 남은 것

* **기기 시험 전부.** 특히 가방 아이콘 탭의 손가락 크기(24×24 논리 px = S25 에서
  약 96×96 px), 옮긴 그리드의 칸이 손가락 아래에서 눌리는가(65.5), 패드 십자키로
  열 건너기.
* 원 서버(`Config::server`)에 붙었을 때 창고 용량이 서버 값으로 남는지 — 코드상
  그렇지만(용량 = 목록 개수) 실서버로 재지 않았다.
* 퀘스트 아이템 일괄 삭제(`DeleteQuestItemAll`)와 분해·조합의 가방 갱신은 정적
  전수로만 덮였다.
* 목록 확대(65.11) 중 스킬 조합·정령 장비 목록은 정적 분석만 — 배운 스킬·정령
  장비가 든 세이브로 기기에서 128 이후를 한 번 골라 볼 것.

### 65.11 목록 커서를 넓힌다 — 414워드 (2026-10-15; 65.13 에서 412)

**왜 "쪽 나누기"가 아닌가.** 125개씩 쪽으로 보여 주고 게스트가 끝에서 감길 때 호스트가
쪽을 넘기는 방법이 더 작아 보였다. 그러나 고른 위치가 쪽보다 오래 산다 —
`ApplyItemRefine` 은 목록을 다시 만든 **직후** `filter[gv+0x1B3E]` 를 읽는다
(0xA2A9C → 0xA2AB0). 게스트가 모르는 쪽 넘김은 그 위치가 *다른 아이템*을 가리키게
하고, 그 아이템이 제련·결합·장착된다. 그래서 게스트 자신의 산술을 넓혔다.

**규칙.** 목록 위치는 모두 부호 없이 읽고 레지스터 전폭으로 계산한다:
`ldrsb`→`ldrb`(145), `lsl/asr #24` 부호확장→이동(164), 피연산자 `…, rX, asr #24`
→`…, rX`(10), `cmp rX, #k<<24`·`cmp rX, rY, lsl #24`→시프트 없이(11),
`and #0xff`→`add #0`(52), `tst #0x80`→`tst #0x80000000`(11). `gv+0x1B3E` 의
−1("고른 것 없음")은 이제 `0xFF` 라 `cmp #0; blt`→`cmp #0xff; beq`(21, 9곳).
**목록 위치에서 나오는 모든 중간값이 [−8, 176] 안이므로, 목록이 127 이하면 각
명령어가 원래와 같은 비트를 낸다** — 이것이 아래 A/B 의 근거다.

**필드.** 화면마다 `this` 의 바이트 필드 중 필터 배열을 색인하거나 필터 개수와
비교되는 것: 정련·오브 `+0x11`, 도감 `+0x14`, 장비 `+0xf`, 스킬 `+0x12`, 정령·가방
`+0x10`. 같은 바이트를 다른 하위 화면이 작은 값(단축벨트 칸, 스킬표 번호)으로 쓰는
자리도 같이 넓혀지는데 무해하다. 메뉴에 들어올 때 `SetState` 가 이 필드들을 0 으로
지우므로(NPC `+0xe..+0x12`, 인게임 `+0xd..+0x13`) 큰 값이 다른 화면으로 새지 않는다.
`gv+0x1B3E` 의 독자는 정련·오브결합·`StateInNpcMenu::Process`·`ApplyItemRefine`·
`ApplyOrbCombine` 뿐이다.

**도구** `tools_re/widen.py` — 앞으로 전파하는 오염 분석(레지스터·스택 슬롯).
값에 *모양*(I = 위치, S24/S16 = 왼쪽 시프트된 것, SIGN)을 달고, 시프트된 값에는
그것을 만든 `lsl #24` 주소를 단다. 거기에 네 겹의 검사:

1. **시드 탐지기 둘.** bagscan 의 포인터 분석 + 오염 패스 안의 독립 탐지기(`this`
   복사·`GetBagItemPtr` 반환·스택 슬롯의 상수까지 따라간다). bagscan 이 `sp` 를
   잃어(65.1 셋째 함정) `DrawRefine+0x838` 이 스택에 넣었다 꺼낸 오프셋으로 읽는
   `gv+0x1B3E` 를 놓쳤고, **그 결과 141번을 고르면 제련 비용이 0, 같은 아이템인
   126번은 300 으로 보였다.** 둘째 탐지기가 23곳을 더 찾았고, bagscan 을 고친 뒤
   3곳(도감 터치의 개수 하위 바이트)이 더 나왔다. 오프셋 기준 구문 전수는 나머지 0.
2. **사슬 단위.** `lsl #24` 는 그 값의 *모든* 독자가 고칠 수 있는 모양일 때만
   이동으로 바꾼다. 하나라도 아니면 사슬 전체를 원래대로 둔다. 남은 것은 하나 —
   `DrawSpiritMenu` 정령 스킬 쪽의 `(x<<24) asr 8` = `x<<16`(x 는 스킬 번호, 8 미만).
   호출은 그 레지스터가 실제 인자일 때만 독자로 친다(맹글 이름에서 인자 수·타입을
   읽는다).
3. **must 분석.** 어떤 경로에서 다른 값이 들어오는 재작성은 6개(`TouchSkillMenu`
   스크롤 산술, 다른 값은 스킬 목록 길이) — 손으로 읽었다.
4. **실행 검증.** `--memwatch lists` 가 이 필드들을 읽는 PC 를 전부 기록하고
   `tools_re/listcheck.py` 가 시드와 대조한다. 5장면 + 아래 시나리오에서 95개 PC,
   목록 화면에서 시드 아닌 읽기 0.

`signed char` 로 위치를 받는 호출(`CommonUi::DrawScroll` 의 줄 번호,
스킬표 하위 화면의 `GetItemTableInfo`, 단축벨트)은 손으로 읽었다 — 전부 줄 번호이거나
43 을 못 넘는 목록이다. 표는 `hl5_core/src/tenlists.rs`(`WIDEN`), 부팅 때
`tenpages` 표 다음에 같은 규칙(전부 대조 후 쓰기)으로 쓴다.

**인수 (오프라인, `--ten-pages`):**

* 5장면 1,241 프레임, 표 적용 대 미적용 — **바이트 동일.**
* 정련 144개: 126→143→0 오른쪽 순환, 0→143 왼쪽 순환. 141번(가방 157)과 126번
  (같은 아이템 8:1)을 고른 화면이 **픽셀 동일**, 141번 제련 → 가방 157 제련도 0→1,
  제련석 20→19.
* 오브결합 153개: 129번(가방 130, 소켓 3) 선택 → 결합 성공 둘
  (`03 000000`→`03 010000`→`03 010100`), 넷째 시도의 파괴가 **바로 그 아이템**을
  지우고 선택이 0xFF 로 돌아간다(원작 벌칙).
* 장비 151개: 강조된 칸이 곧 장착된다 — 140번(가방 143) 장착. 앞선 두 실행은
  목록이 커서 1 에서 열려 오른쪽 140번이 141번이었고 그때도 강조 칸(가방 144)이
  장착됐다(100번 실행도 같은 이유로 101번). 처음엔 "한 칸 뒤를 장착한다"로 잘못 읽었다.
* 소켓확장(가방 선택기) 152개: 140번 → 가방 142 소켓 `00 ff…`→`01 00ff…`, 확장권 3→2.
* 리플레이로 못 간 것: 스킬 조합 목록(배운 스킬 필요), 정령 장비 목록(정령 장비 필요)
  — 정적 분석만.

프로브에 더한 것: `--memwatch lists`, 스크립트 `money <n>`(게스트
`BagItem::IncreaseMoney`), `bagdump` 에 제련도·소켓 바이트·필터 목록(가방 칸)·
`gv+0x1B3E`.

### 65.12 창고 두 파일은 한 번에 커밋한다 (2026-10-15)

리뷰가 잡은 것: 저장이 위 파일(80~159)을 먼저, 아래 파일(0~79)을 다음에 썼다.
`아이템정리`(4/9)는 아이템을 **파일 사이로** 옮기므로(80번 → 0번) 그 사이에
멈추면 아이템이 어느 파일에도 없다. 재현: 81개 입고 → 0번 출고·저장 → 정리 →
아래 파일 쓰기 실패 → 다시 읽으면 79개.

**커밋 지점을 하나로 했다 — 아래 파일의 rename.** 두 파일에 `generation N` 줄을 쓰고
(옛 빌드는 모르는 줄을 건너뛰고 다시 쓸 때 버린다), 저장은 ① 위 파일을
`hl5_warehouse_upper.new` 에 쓰고 ② 아래 파일을 rename(=커밋) ③ `.new` 를 위 파일로
rename. 읽기는 `.new` 의 세대가 아래 파일과 같으면 커밋 후 멈춘 것이라 마저 옮기고,
다르면(또는 아래 파일에 세대가 없으면 — 옛 빌드가 썼다) 커밋 안 된 것이라 지운다.
`write_atomically` 는 rename 전에 `sync_all`, 유닉스에선 디렉터리도 동기화한다.
시험 셋: 리뷰의 재현(아래 쓰기 실패 → 80개 그대로), ②와 ③ 사이 중단(→ 다음 읽기가
끝내고 81번째 중복 없음), 옛 빌드 파일 옆의 `.new`(→ 버림).

남은 것(이전부터): 호출부는 저장이 실패해도 게스트에 "성공"이라 답한다. 입고에서
그러면 게스트가 가방 사본을 지우므로, 다음 저장 전에 앱이 죽으면 그 아이템이 없다.
두 파일과 무관한 단일 파일 시절부터의 성질이라 이번에 바꾸지 않았다.

### 65.13 장비를 탭으로 바꿀 수 없었다 — 65.11 의 표가 비교 하나를 반쪽만 넓혔다 (2026-10-16)

**증상(사용자):** "처음 장비 장착하고 그 뒤에 장비 교체하려고 클릭 아무리 해도 교체가
안 된다." 장비 화면의 목록에서 칸을 탭하면 강조만 옮겨 가고, 같은 칸을 다시 탭해도
장착되지 않는다. 목록을 처음 열면 커서가 0 이라 첫 칸은 장착된다 — "처음은 된다"가
그것이다. 정령 장비도 같다.

**원인.** `TouchEquipMenu` 의 목록 판정(0x102B38 루프)은 칸 `i` 마다

```
0x102b70  lsl  r0, r0, #24        ; i << 24 (칸 루프 변수 — 위치가 아니다)
0x102b78  str  r0, [sp, #0x28]
0x102b80  ldrb lr, [r6, #0xf]     ; 커서 (65.11: ldrsb → ldrb)
  …                               ; r1 = 커서 % 5   (lsl/asr #24 → 이동)
0x102bb4  ldr  lr, [sp, #0x28]    ; i << 24
0x102bb8  cmp  lr, r1, lsl #24    ; "강조된 칸을 다시 탭했나" → 같으면 +0x4cc = 장착
```

이다. 65.11 의 표가 마지막 줄의 `lsl #24` 를 지워 `cmp lr, r1` 로 만들었다. 왼쪽은
위치가 아니어서 안 넓혀졌으므로 비교는 `i << 24 == 커서 % 5` — **`i = 0` 이고 커서가
5의 배수일 때만 참**이다. 나머지 칸은 영원히 "강조만 옮기기" 갈래로 간다.
`TouchSpiritMenu+0x8a0` 가 같은 모양이다(0x100240 의 `i << 24` 를 `[sp+0x24]` 에서
다시 읽는다). 정련·오브결합·도감·스킬의 같은 판정 넷은 왼쪽이 `커서 % 5` 를 시프트한
쪽(그 `lsl #24` 도 이동이 됐다)이라 맞았다.

**왜 `widen.py` 가 그렇게 냈나.** 시프트된 비교 규칙이 `a_forms <= {"S24"}` 였고,
왼쪽이 오염되지 않은 레지스터면 `a_forms` 가 **빈 집합**이라 그 검사를 통과했다.
`==` 로 고쳤고, 다시 돌린 표가 **정확히 그 두 워드만** 빠진 412워드다(이전 도구의
출력은 커밋된 414와 바이트 동일함을 먼저 확인했다). 두 워드는 출하 비트로 남는다 —
`커서 % 5` 는 커서가 몇이든 0..4 라 `lsl #24` 비교가 그대로 옳다.

**왜 65.11 의 인수에서 못 잡았나.** 1,241 프레임은 장비 화면을 탭으로 쓰지 않고,
장비 시나리오는 `key right` ×140 + `key select` 였다 — 키 경로(`KeyEquipMenu`)는
이 비교를 안 지난다. "127 이하에서는 같은 비트"라는 논증은 *위치에서 나온 값*에
대한 것인데, 이 비교의 왼쪽은 위치가 아니었다.

**인수 (오프라인, `--ten-pages`):**

* 새 워리어 + 롱소드 넷, 장비 → 무기 칸 → 착용하기 → 목록 둘째 칸 탭 두 번:
  고치기 전 `equip [0, …]` 그대로(강조만 둘째 칸), 고친 뒤 `착용하였습니다.` +
  `equip [4, …]` — 기능을 끈 실행(`[4, …]`)과 같다.
* 151개 목록에서 키로 140 에 간 뒤 그 줄의 셋째 칸 탭 두 번: `filter[142]` = 가방
  145 가 장착(`equip [145, …]`) — 127 너머의 터치 경로.
* 정령(`spirit 3`, `item_05` 81..83 셋): 정령 장비 칸 → 둘째 칸 탭 두 번 — 고치기 전
  `e` 표시가 첫 칸에 남고, 고친 뒤 둘째 칸으로 옮기며 `착용하였습니다.`.
* `tenlists.txt` 키 시나리오 넷의 기대값 전부 그대로(정련 142 r0→r1·제련석 20→19,
  오브 130 `03010101`, 소켓확장 142 `01 00ff…`·3→2, 장비 140 → 가방 143),
  `listcheck.py` 놓친 읽기 0.
* 단위시험 `a_compare_is_unshifted_only_with_its_left_side` — 시프트를 지운 비교마다
  바로 앞 워드가 그 왼쪽 레지스터의 `lsl #24 → 이동` 이어야 한다. 옛 0x32c 를 다시
  넣으면 실패한다(확인). `hl5_core` 251 통과.

**교훈.** 목록 화면을 고치면 키로만이 아니라 **탭 두 번**으로도 써 볼 것 — 이 게임의
터치 활성화는 "강조된 칸을 다시 누른다"이고, 그 판정이 키 경로와 별개의 코드다.

### 65.14 창고에 물건이 많으면 앱이 죽거나 칸이 사라졌다 — 게스트 수신 버퍼가 8 KB (2026-10-24)

**증상(사용자):** "창고 확장작업 이후에 창고에 물건이 많으면 창고 내부 물건이 소실되거나
앱이 다운되는 현상이 있다."

**원인 — 응답 프레임 하나가 게스트가 받을 수 있는 크기를 넘는다.** 받는 길은 셋이고
어느 단계도 길이를 상한과 비교하지 않는다:

1. `_midas_funcOnSmartReqSocketReadCB`(0x40DF0)가 `[u32 4+len]` 을 읽고 몸통 `len` 바이트를
   `_midas_netQueueReadBuffer` 에 모은다. 그 버퍼는 **0x5000**(`MX_smartConnect` 0x38094
   `mov ip, #0x5000`, `.bss` 20,480 B). 루프의 비교(`cmp r6, [queue+0x20c]`)는 "지금까지
   읽은 양"뿐이다.
2. `netLibCB`(0xCE258) 이벤트 0xa: 버퍼 첫 워드(게임 프레임 자신의 길이)가 `len` 과 같으면
   `b NETWORK::receiveData`, 아니면 `MX_netSocketReadContinue`.
3. **`NETWORK::receiveData`(0xC8438)** = `mov r0, #0x2000; add r0, r0, #4; bl DM_MALLOC;
   memcpy(buf+4, frame, len); strh len → writeLen`. 그리고 `readNet8/32/Buf` 는
   `buf + 4 + (i16)readPos` 를 읽을 뿐 `writeLen` 과 비교하지 않는다.

즉 게임 프레임 하나는 **0x2000 = 8,192 B 가 상한**이고, 넘친 바이트는 그 블록 다음의
게스트 힙을 덮는다(힙 메타데이터는 호스트 쪽이라 할당기는 안 깨진다 — 게스트 객체만
깨진다, `heap.rs`).

**왜 10페이지에서 처음 넘었나.** `4/6` 목록은 칸마다 `[i32 slot][u8 state]` + `_NET_ITEM_`
이고 장비 레코드는 68 B + 이름, 빈 칸은 4 B 다. 표에서 가장 긴 장비 이름은 13 B
(`(진)` 를 붙이면 17 B)라 **80칸은 전부 장비여도 7.6 KB** — 다섯 페이지 시절엔 원리상
넘을 수 없었다. 160칸이면 `1,472 + 74n` B(이름 10 B 레코드 기준)라 **장비 90개 = 8,132 B,
91개 = 8,206 B** 에서 넘는다. 160개면 13,312 B.

**실측 (고치기 전 빌드, 작은 가방 세이브 + `whfill.py --all-equip --slots N`, 창고관리 열기):**

| 장비 수 | 목록 | 결과 |
|---|---|---|
| 80 | 7,392 B | 정상 |
| 90 | 8,132 B | 정상 (단위시험이 이 경계를 쓴다) |
| 91, 94~120 | 8,206 B~ | **12개 중 10개가 폴트** — `NewNetStorageItem+0xc`(`this` 가 덮였다), `ItemTable::DeleteItem+0x14`, `EquipItemInfo::CopyData+0x18`, 엉뚱한 주소 쓰기. 기기에서는 앱 다운 |
| 92, 93 | 8,280·8,354 B | 폴트 없음 — 그런데 **1페이지 0번 칸의 아이템이 사라지고**(빈 칸, 툴팁 없음) **9·10페이지에 호스트 파일에 없는 유령**(스택 22)이 생긴다 |

뒤의 둘이 "소실"이다. 목록을 파싱하며 `NewNetStorageItem` 이 레코드마다 `ItemInfo` 를
할당하는데, 최적적합 할당기가 그것을 **넘친 바이트가 앉아 있는 빈 자리**에 놓으면
생성자가 아직 안 읽은 레코드를 덮는다. 어느 쪽이 되는지는 그 순간의 힙 배치, 즉 플레이
이력이 정한다 — 같은 개수도 세이브가 다르면 폴트와 소실이 갈린다(160칸 가방 세이브에선
96개가 `CopyData` 폴트, 작은 가방 세이브에선 91개가 `NewNetStorageItem` 폴트).

**§65 의 인수가 못 잡은 이유.** `whfill.py` 기본 채움이 짝수 칸 장비(78 B) + 홀수 칸 오브
(10 B)라 160칸 목록이 **7,872 B — 한계 320 B 아래**였다. 그 채움으로 한 모든 시험이 통과한
것은 맞고, 다만 그 크기가 실제 플레이에서 가장 흔한 경우(장비를 창고에 모은다)가 아니었다.
이제 `--all-equip` 가 있다.

**고침.**

1. **`tenpages::RECV` — 게스트 워드 하나.** `receiveData+0x8` 의 `mov r0, #0x2000`
   (0xE3A00A02)을 `#0x5000`(0xE3A00A05)으로. midas 수신 큐와 같은 크기라 **전송 계층이 줄 수
   있는 어떤 프레임도 `NETWORK` 가 담는다.** `readNet*` 의 커서는 `i16` 이라 32,767 까지
   읽을 수 있고, 160칸의 최악(이름이 `_NET_ITEM_+0x06..+0x20` 의 26 B 를 다 채워도)은
   15,872 B 다(단위시험 `the_largest_listing_fits_the_receive_buffer`). 다른 표와 같은
   규칙(부팅 첫 명령어 전, 대조 후 쓰기)이고 `Config::ten_pages` 뒤에만 있다 — 다섯 페이지는
   넘친 적이 없고, 기본 장면의 힙 배치를 안 바꾸려고.
2. **호스트가 담을 수 없는 프레임을 보내지 않는다.** `Session::frame_limit` 는 연결 때
   `tenpages::recv_capacity(vm)` 가 **게스트 메모리의 그 명령어를 읽어서** 정한다(스위치가
   아니라). `4/6` 이 그보다 크면 `[i32 1][창고 목록이 너무 커서;받을 수 없습니다.]` 로
   거절하고 아이템은 하나도 안 건드린다(단위시험
   `a_listing_the_guest_cannot_hold_is_declined_not_sent` — 90개 통과, 91개 거절, 버퍼를
   키우면 160개 전부). 1이 있는 한 발동하지 않는 안전망이다. 소지품과 함께 커지는 응답은
   지금 `4/6` 하나뿐이다.

**이미 겪은 사람에게.** 넘침은 게스트 힙에서 일어났고 **호스트 파일
(`hl5_warehouse.txt`·`_upper.txt`)은 한 번도 안 건드렸다** — 화면에서 사라졌던 아이템은
파일에 있고 이 빌드에서 다시 보인다. 예외 둘: 화면이 틀린 동안 `버리기` 한 칸(호스트는 그
칸 번호의 진짜 아이템을 버린다), 그리고 넘친 바이트가 덮은 게스트 객체를 게스트가 저장한
경우(예: 가방의 `ItemInfo` → `SaveBagData`). 시험한 두 무폴트 경우는 가방 덤프가 새 빌드와
바이트 동일했지만, 이웃이 플레이 이력에 달렸으니 기기에서 일어나지 않았다고는 말할 수 없다.

**인수 (오프라인, `--ten-pages --free-shop --offline-menus`):**

* 고친 빌드: 장비 91~120·160개 창고 전부 폴트 없음, `--memwatch storage` 위반 0, 160개의
  10페이지가 그려진다. 92·93개의 페이지 10장 중 위의 넷을 뺀 나머지는 구 빌드와 픽셀 동일.
* 장비 150개 창고(12,572 B): 10페이지 144번 출고 → 가방 25칸 → 같은 아이템 입고 → 144번,
  목록 세 번 다 통과.
* 단위시험 295(새 2). 표 검사 둘(`every_table_word_…`, `no_word_is_rewritten_twice`)에 `RECV` 포함.

## 66. 컷신의 아바타 — 스토리 장면은 주인공과 렌을 NPC 몸으로 다시 만든다 (2026-10-15)

### 66.1 증상

사용자: "현재 캐릭터와 렌의 아바타가 컷신에서는 적용 안 된다. 전투하다가 퀘스트로
넘어가며 대화할 때 아바타가 바뀌어 어색하다." §57·§61 의 치환은 필드와 메뉴에서만
보였고, 대화 장면이 열리면 둘 다 장비 차림으로 돌아갔다가 장면이 끝나면 다시 아바타가
됐다.

### 66.2 원인 — 장면은 주인공의 스프라이트를 쓰지 않는다

* **`EventProc::Event_PlayerChange(a)` 0x6DA78** 은 스크립트 함수다(호출자
  `onFunction+0xb8c`, 그리고 `Event_End+0x9c` 가 `(0)` 으로 되돌린다). 세 모드:
  `0` = 자기 스프라이트(`gv+0x1468/0x146c/0x1470`)로 `SetSpritePtr` 복귀,
  **`1` = NPC 몸을 새로 만든다**, `2` = 복귀하되 NPC 이미지의 `+0x38` 을 복사.
  `HERO+0x1feb` 이 "지금 NPC 몸" 플래그, `+0x1fed` 가 그 NPC 칸 번호다.
* 모드 1 의 순서: `Map::MapNpcG_set(map, &rec, 0)` → **`/c/csv/npc_g.dat` 레코드 0**
  (81 레코드 × 25 바이트, `[u16 count][u16 reclen]` 뒤 레코드마다 `+4` 타입 바이트,
  `+6..+0xe` 층 0..8 의 이미지 번호 아홉, `+0xf..` 팔레트 u16 일곱). 레코드 0 은
  **타입 0 = 플레이어**, ids `[0,0,0,0,255,22,255,255,255]`.
* **`Map::NpcPathSet(&rec)` 0xAE9F4** 가 `_szSprFile`(10×0x100, memset 0)·
  `_nSprPalFile`(−1)을 쓴다. **타입 0 이면 층 0..3 을 장착 아이템에서 만든다** —
  `GetEquipItemPtr()->GetEquipItem(1..4)` 의 `+0x150`/`+0x152`, 층 0 은 `gv[0x8a]`
  (투구보기) 0 이면 `img0/000` — `HERO::SetLoadImgPath` 가 하는 것과 같지만 **그
  함수가 아니다.** 나머지 층은 `/c/sp/img%d/%03d.mgr`(id, dir = 층 ≤ 6 ? 층 : 5),
  0xFF 는 `/c/sp/empty/empty.mgr`. 타입 ≠ 0 은 모든 층이 레코드의 번호다.
* 그 뒤 `new SPRITE_DATA` + `LoadSprFile("/c/sp/cif/%03d.cif", Map[0xc96+idx])` —
  `PlayerChange` 는 그 자리를 0 으로 두므로 **cif 000**(직업 cif 001..004 가 아니다),
  `NpcCreate` 는 `rec[0]` 타입을 cif 번호로 쓴다 — 그리고
  `SPRITE_IMAGE::LoadNpcSprImg(data, rec+2)` 가 `_szSprFile[층]` 을 `OpenGbmMerger` 로
  열어 적재한다(층 5·7·8 은 건너뛴다). 즉 §57 의 `SetLoadImgPath` 후크가 보지 못하는
  문이고, `LoadSprFile` 후크도 `this != gv+0x1468` 이라 사양한다.
* `NpcPathSet` 의 호출자는 넷: `Event_PlayerChange+0x2c8`, `Map::NpcCreate+0x310`,
  `Map::NpcChange+0x114`, `QuestMgr::LoadNpcBody+0x44`(퀘스트 창의 인물,
  `setQuestViewInfo` 가 부른다; `QuestMgr*` 는 `gv+0x1604`).
* **렌도 장면에서는 NPC 다.** 레코드 1 = 타입 53, ids `[42,25,255,255,255,21,…]` —
  `Battle::LoadSpiritRes` 가 `Spirit` 에 싣는 바로 그 시트(§61). 전수에서 (42,25) 쌍은
  이 한 레코드뿐. 장면의 렌은 `NpcCreate` 로 만들어져 cif 053 으로 그려진다(필드의
  Spirit 은 065; 시트가 같으므로 의상 시트는 둘 다 맞는다 — `costume::build` 가 이미지
  수를 원본과 같게 보장한다).

### 66.3 고침 — `Map::NpcPathSet` 후크 하나 (`hl5_core/src/npcbody.rs`)

원본을 `vm.call` 로 돌린 뒤 호출자의 `_NPC_G_DATA` 를 읽어 **타입 0** 이면 `TABS` 의
층마다 게스트가 시트를 남긴 층만(빈 문자열·`empty.mgr` 은 건너뛴다) `Avatar::worn` 으로
`dress_layer`, **(42,25)** 면 `_szSprFile[0..1]` 을 렌 의상 시트로, 그 밖은 아무것도.
`avatar`·`spirit_avatar` 가 설치된 뒤에 설치된다(`Config::avatar`, 셸 on).

* 무기·방패는 레코드가 비워 둔 대로다 — 장면의 주인공은 레코드의 뜻으로 비무장이고,
  그래서 이펙트 층도 입히지 않는다(어차피 `LoadNpcSprImg` 가 층 5 를 건너뛴다).
* **리그 포즈는 장면에서 안 움직인다** — cif 000/053 은 리그 변형 대상이 아니라 리그
  시트의 첫 이미지(원본 포즈)만 그린다. 모든 NPC 몸이 그렇게 그려지는 것이고,
  `dressed_rig`·`cif_loaded` 는 주인공 자신의 `SPRITE_DATA` 에 관한 것이라 손대지 않는다.
* 카운터 `Avatar::bodies_dressed`, `Wardrobe::bodies_dressed`(프로브 요약의
  `scene body(ies) dressed`), 입힐 때마다 로그 한 줄.
* 프로브 명령 `npcbody <record>`: `QuestMgr::LoadNpcBody(gv[0x1604], record)` 를
  부르고 `_szSprFile[0..1]` 을 찍는다 — 게스트→게스트 호출이라 후크가 걸린다(프로브가
  `NpcPathSet` 을 직접 부르면 `LR = RETURN_MAGIC` 이라 사양한다).

### 66.4 증명

* 재생 `tools_re/scripts/avatar-cutscene.txt` — `avatar.txt` + `spirit-avatar.txt` 가
  남긴 `hl5_avatar.txt`(`0 0 0=100001 1=110011 4=110024 s=130010`) 위에서 새 게임을
  같은 슬롯에 만들고, 오프닝 첫 장면만 SKIP 한 뒤 연구실 대화를 45프레임마다 탭으로
  진행한다. `PlayerChange(1)` 은 연구실 뒤 **어두운 "목소리" 장면**(프레임 ~3650)에서
  온다 — 연구실의 금발 검사와 오프닝의 바위 옆 인물은 플레이어가 아니라 자기 시트를
  가진 NPC 다(타입 0 이었다면 후크가 바꿨을 텐데 그 프레임들이 전후 동일하다).
* 후크 전후 41장 중 **40장 바이트 동일**, 다른 것은 프레임 3688 의 (191,76)-(212,120)
  — 인물 상자 하나뿐이다. 전: 청록 머리의 장비 차림, 후: 마법사 모자 + 아스트라 코트.
* `npcbody 1` → `/c/sp/img0/321.mgr / /c/sp/img1/321.mgr`(렌 의상 1), `npcbody 0` →
  `img0/256 / img1/301`(모자·코트). 요약 `3 scene body(ies) dressed`(연구실 진입의
  `NpcCreate` 레코드 0 + 어두운 장면 + `npcbody 0`) / 렌 `1`.
* 단위시험 250(`npcbody` 2: 레코드 분류). `ab_905.sh` 는 `--avatar` 없이 돌므로
  정의상 무관하고, 돌려서 1,241 프레임 동일을 확인했다.

### 66.5 남은 것

* **기기 미시험.** 볼 것: 퀘스트 대화 중 주인공이 아바타 차림인가(모자·몸통·팔·다리;
  무기는 원래 없다), 렌이 나오는 장면에서 렌이 의상인가, 퀘스트 창의 인물. 리그
  머리칼이 장면에서 안 흔들리는 것은 의도다.
* 투구보기 OFF 와의 관계는 필드와 같다 — 게스트가 `img0/000` 을 쓰고 그 위에 모자가
  입혀진다.

## 67. 환경설정 12행 — 화면필터·패드키(맵핑 화면), 세로 배지, 패드 지연 한 틱, 타격음 (2026-10-22)

사용자 요구 다섯: ① 터치방식 밑에 **패드키** 행(기본/커스텀, 커스텀이면 확인 팝업 →
예면 게임 화면 안의 키 맵핑 화면), ② 세로 모드에서 허공에 뜨던 패드 배지, ③ 패드
방향키 이동 반응이 ~0.1초 늦다, ④ 개정판에서 몬스터 타격 무기 효과음이 안 난다,
⑤ 화면 후처리가 있으면 화면효과 밑에 켜고 끄는 행. 코드는 `hl5_core/src/options.rs`
(목록), `padmap.rs`(맵핑 화면), `gamepad.rs`·`portrait.rs`(배지·지연), 그리고
`android_app/.../Hl5Audio.java`(효과음).

### 67.1 목록을 "덧붙이기"에서 "표"로 — 열 번째 행 기법은 가운데 삽입을 못 한다

§45.7 은 인덱스 9 를 호스트가 갖는 다섯 후크였다. 이번 두 행은 **가운데**(화면효과 밑,
터치방식 밑)라 그 방식이 안 된다 — 게스트의 `DrawOption`·`KeyOption` 이 인덱스로
9바이트 타입표와 `gv[0x84+idx]` 를 읽기 때문이다. 그래서 **표시 인덱스 ↔ 게스트
인덱스**를 한 표(`options::ORDER`, 12칸)로 두고 게스트가 인덱스를 읽는 자리마다
번역한다:

```
display  0 배경음 1 효과음 2 게임속도 3 진동 4 화면효과 5 [화면필터]
         6 오토타겟 7 투구보기 8 터치방식 9 [패드키] 10 데이터 관리 11 [해상도]
```

* **`DrawOption+0x234`(행 루프 머리)** — 매 바퀴 `display = (idx − row) + slot` 을
  커서 바이트에서 다시 계산한다(첫 바퀴 뒤의 `r7`·`sb` 는 지난 바퀴 값 +1 이라 못
  믿는다). 게스트 행이면 `r7 = g`, `sb = sp+0x5c + 2g`(라벨 id 배열), 그리고
  **`[sp+0x48] = g − slot`** — 선택된 행 arm(0x11DE68)이 `r7` 이 아니라 캐시된
  `first + slot` 으로 인덱스를 다시 만들기 때문에 이것도 맞춰야 한다. 그 뒤 원래
  명령을 그대로 돌린다(사양). 호스트 행이면 그리고 꼬리(`+0x3f4`)로 복귀.
* **`KeyOption` 진입** — UP/DOWN 은 12행 기준으로 호스트가 전부 답한다(창 안이면
  이동, 아니면 한 줄 스크롤, 끝에서 랩 — 게스트 자신의 모양). 그 밖의 키는 게스트
  행이면 `this+0x0f` 에 **게스트 인덱스를 잠시 써 두고** 게스트 `KeyOption` 을
  `vm.call` 로 돌린 뒤 되돌린다(게스트가 커서를 안 움직였을 때만). 호스트 행이면
  LEFT/RIGHT/SELECT 는 호스트, BACK 은 게스트(그 arm 은 인덱스를 안 읽는다).
* **`TouchOption+0x24c`** — "커서 행 두 번째 탭 = idx 8 이면 SELECT" 비교가 표시
  인덱스를 8 과 비교하므로, 데이터 관리가 있는 행(10)으로 고쳐 분기한다.
  `+0x2ac` 클램프는 `first` 4..6 을 통과시키고, `+0x30c` 랩은 (5, 11) 을 쓴다.
* `DrawScroll` 총계 9 → 12.

커스텀(3글자, 33 px)은 31 px 값 상자보다 넓어 **그 행만 상자를 4 px 넓히고 오른
화살표를 같이 민다**.

### 67.2 화면필터 — 후처리는 Scale2x 하나였다

화면 후처리는 §32.10 의 **Scale2x** 뿐이다(계단을 반으로 줄이는 픽셀아트 스케일러 —
사용자가 "도트가 뭉개지고 흐리다"고 느낀 것이 정확히 이것이다; EGL 샘플링은 NEAREST).
행 `화면필터 ON/OFF` 가 `Vm::scale2x` 를 토글하고 `hl5_scale.txt` 에 `scale2x`/`nearest`
를 쓰며, 셸은 틱마다 `Vm::take_scale2x_change` 로 받아 프리젠터의 Scale2x 패스를 켜고
끈다(두 배 버퍼 폐기, `last_fb` 비움, 강제 present; EGL 은 텍스처 크기 변경을 이미
처리한다 §34). **재시작 없이 즉시 적용**된다. 셸이 부팅 때 읽던 파일을 행이 쓰므로
다음 부팅도 그 값이다.

### 67.3 패드키 — 팝업 id 8 의 답을 한 번만 가로챈다

값 기본 ↔ 커스텀. 커스텀으로 가면 §46 의 두 버튼 팝업(id 8)을 `패드키 설정을;진행하시
겠습니까?;아니오를 누르면;기본 상태로 진행됩니다.` 로 띄운다(**네 토막** — 첫 문장
29바이트를 한 줄에 두면 `?` 가 다음 줄로 떨어졌다, 첫 시도의 스크린샷). id 8 의 답은
`StateMainMenu::Process+0x698`(0x124F30) 이 읽어 0 이면 `MC_knlExit` 다. 후크가 그
자리에서 **`padmap.pending` 일 때만** `this+0x15` 를 읽고(0 = 예 → 맵핑 화면 열기,
1 → 기본 유지), `r6 = 1` 로 두고 게스트의 아니오 경로(0x124EAC)로 복귀시켜 팝업을
게스트가 지우게 한다. 게임종료·해상도 팝업은 `pending` 이 아니므로 그대로 게스트다.

**상태**: 바인딩은 그대로 `hl5_gamepad.txt`(파서는 여전히 `gamepad::parse_map_reporting`
하나), 그것이 적용 중인지는 새 파일 **`hl5_padmode.txt`**(`custom`|`default`). 기본을
고르면 파일은 지우지 않고 읽기만 멈춘다 — 실수 한 번에 배치를 잃지 않게. 모드 파일이
없고 `hl5_gamepad.txt` 가 있으면(이 행 이전에 손으로 쓴 사람) 커스텀으로 센다.

### 67.4 맵핑 화면 — 게스트가 그리고 호스트가 배지를 붙인다

`DrawOption` **진입** 후크가 `padmap.active` 면 목록 대신 화면을 그린다: `FillRectAlpha`
(검정 0xb4)로 덮고, 제목은 게스트의 `DrawSubjectBox`, 상자는 **선택된 것만**
`DrawHalfBox(1,1,0xff)`(커서 판), 나머지는 값 상자와 같은 `FillRoundRect`(검정) —
`DrawHalfBox` 의 보통 스타일은 목록 판 위에서만 어둡게 읽히는 반투명 흰 칠이라 검은
시트 위에서는 흰 상자에 흰 글자가 됐다(첫 시도). 라벨은 `BFont::DrawText` EUC-KR.

배치는 `padmap::layout(w, h, portrait)`:

* **가로**: 필드 HUD 그대로 — 여섯 스킬 버튼 `(W − xs[i], H − 6 − ys[i])`
  (xs/ys = `0x159C38` 표의 넓은 쪽 절반, `SetTouchKeypad` 0xE12E4 의 식), 페이지
  `(W−0xbf, H−0x2a)`, 확인 `(W−0x3d, H−0x46)` 60×60 — 585×270 에서 (394,228)·
  (524,200), 프로브가 실제 `StateGame` 에서 읽는 값과 같다. 라벨 `퀵1~5·콤보·페이지·확인`.
  왼쪽 격자(60×16, 68/24 피치)에 취소·대시·지도·상점·저장과 **초기화·완료**.
* **세로**: 벨트 한 줄(`portrait::belt_row` — 다시 쓴 `DrawSkillBelt` 표의 열두
  halfword, (8+32i, 274))과 페이지 `(200,277,27,27)`(프레임에서 잰 아이콘 201..226 ×
  278..302), 확인은 그려지지 않으므로 격자(90×20, 102/36)에 취소·대시·지도·상점·저장과
  함께. 단위시험이 다섯 캔버스에서 상자 겹침 0·캔버스 밖 0 을 센다.

**입력**: 실제 손가락은 `Vm::touch` 머리에서 가로채고(패드의 가짜 손가락 `0x70` 은
제외; 누름을 삼켰으면 그 이동·뗌도 삼키고, 안 삼킨 뗌은 게스트에게), 패드 버튼은
`Vm::pad_button` 머리에서(`padmap::pad_button` — 선택된 상자가 있으면 `ASSIGNABLE`
13개(BACK 포함) 중 하나로 **배정**, 없으면 B/BACK 이 닫기; 세로 키패드 코드는 CLR 만 뒤로), 스틱은
`pad_axes` 에서 버린다, 키는 `KeyOption` 후크의 BACK. **배정 규칙**(`padmap::assign`):
`overrides[K] = Some(action)`, 그리고 그 action 을 내던 다른 모든 K′ 는 `None` — 상자
하나에 버튼 하나, 버튼 하나에 상자 하나. **버튼이 전에 내던 상자는 끊긴 채로 둔다** —
기본 배치의 그림자(십자키 가운데=확인, 패드 BACK=취소)가 그 상자를 이어받으면 배지 패스가
그것을 "저절로 배정된" 것처럼 그렸다(첫 기기 보고). 그래서 `assign` 이 옛 action 을 기본값으로
내던 코드를 쓸어 `None` 으로 두고, 커스텀 표는 십자키 가운데를 처음부터 `None` 으로 둔다
(`custom_baseline`). 저장은 13 버튼 + 십자키 가운데 14줄을 **전부** 쓴다
(`NONE` 포함) — 기본값이 바뀌어도 풀어 둔 버튼이 되살아나지 않게. 방향키·십자키는
배정 대상이 아니다(조종한다, §52.1).

배지는 `pad_paint_badges` 가 `padmap.active` 면 `padmap::paint_badges` 로 — 필드
배지와 같은 `pos_for_action` 역룩업·같은 `draw_badge` 라, 배정 한 번에 필드 배지와
맵핑 화면 배지가 같이 움직인다. 상태가 바뀔 때마다 `gl.forget_last_draw()`.

### 67.5 세로 배지 — 터치 rect 가 아니라 그려진 자리

세로에서 배지가 허공에 뜬 이유: `StateGame+0xe0/+0xec` 의 터치 rect 는 240폭 **호**
((131,242)…(200,67))인데 §56 의 111 워드는 `DrawSkillBelt` 의 그리기 표만 한 줄로
옮겼다(터치는 키패드가 대신하므로). `read_touch_ui` 가 세로면 `portrait::drawn_hud`
— 벨트 한 줄 + 페이지(200,277), 확인은 빈 rect(안 그려진다) — 를 돌려준다.
`out/shot/portrait-field.png`(패드 연결) 에서 여섯 아이콘과 페이지 아이콘 위에 배지.

### 67.6 패드 지연 — 호스트가 한 틱을 더 내고 있었다

`--watch` 로 재 보니(`scratchpad/latency2.txt`, `echo` 를 로그에도 넣어 프레임 표시):
스틱 → 누름 전달 → 게스트 틱 N 에 `TouchPressed`, **N+1 에 게스트 자신의 이동**(중심),
**N+2 에 우리 드래그**(방향) → 걷기. 이유 둘:

1. `onHandleEvent+0x11c` — 게스트는 누름마다 같은 점의 `TouchMove` 를 **하나 더 넣는다**
   (§59 가 본 것). 그것이 이동 칸(`gv+0xF2`)을 차지해 `pad_pump` 의 드래그가 다음
   틱까지 기다렸다.
2. `mainKeyPressed` 는 **한 틱에 누름 칸 또는 이동 칸 하나만** 본다 — 0x9CA5C 에서 `F0`
   이 서 있으면 `F2` 검사를 건너뛴다. 그래서 엄지도 누른 다음 틱에 걷는다(원작 동작).

고침은 1 뿐이다 — `pad_pump` 가 큐 앞이 `Moved` 이고 `F2` 가 **우리 포인터 id(0x70)**
의 이동으로 차 있으면 좌표(`gv+0x104/+0x108`)를 우리 것으로 **덮어쓰고** 큐에서 뺀다
(`BaseState::TouchMove` 0x44DCC 가 쓰는 세 워드). 메일박스는 셸의 입력 채널이라 게스트
동작을 바꾸는 쓰기가 아니다. 결과: 누름 N, 드래그 N+1 **에 `SetMoveXY`** — 한 틱(36~45 ms)
단축, 엄지와 같다. 요약의 `N drag(s) merged into a press`.

**고정 조이스틱 방식에서 처음부터 기울인 점에 누르기**도 시도했다: `TouchPressed` 의 고정
경로(0xE217C)는 각도(`gv+0x278`)만 쓰고 걷기를 시작하는 방향 `KeyPress` 는 **`TouchMoved`
(0xE166C)만** 내므로 틱을 못 번다. 되돌렸다. 남은 1틱 + 게스트 틱 위상(평균 18 ms)은
원작의 것이다.

### 67.7 타격 효과음 — SoundPool 공유 fd 레이스 (Android 11+)

조사(서브에이전트): 게스트는 타격음을 요청하고(`HERO::CheckEffSound_Hit` 0x89448 →
`HERO+0x1FC6` → `Battle::endProcess+0x304` → `HERO::PlayEffSound` 0x8D984 →
`SoundMgr::PlaySound`/`PlayEffSound` → `TimerCb+0x184` `CheckAndPlayEffect` →
`PlayLoadedEffect` → `MC_mdaPlay` → `AndroidService::clipPlaySound`), 호스트는
`AudioEvent::Play{key 26411768}` = `c/snd/eff_01.ogg`(오프셋 13315739, 8899 B, 정상
Ogg) 를 Java 로 넘긴다. 잃는 곳은 Rust 가 아니다. **`Hl5Audio.java` 가 `openFd` 하나를
모든 `SoundPool.load`/`MediaPlayer.setDataSource` 에 넘겼고**, Android 11 의 SoundPool
은 디코더 스레드가 둘(`kDecoderThreads = hardware_concurrency() >= 4 ? 2 : 1`)이며
각 `Sound` 는 fd 를 `dup()` — dup 은 **파일 오프셋을 공유**하므로 동시에 디코드되는 두
클립(`eff_00` 휘두르기·`eff_01` 타격, 부팅 때 연달아 로드돼 첫 쌍)이 서로의 바이트를
읽는다. 원본은 Android 10 S9 에서 단일 스레드라 멀쩡했다. 고침: **로드마다
`AssetFileDescriptor` 를 새로 열고 호출 직후 닫는다**(SoundPool 은 load 안에서 dup,
MediaPlayer 는 setDataSource 복귀 후 닫아도 된다고 문서화). javac -Xlint 통과.
오프라인으로 레이스 자체는 증명 불가 — 기기에서 타격음이 나는지, `logcat -s hl5 SoundPool`
에 `soundID n not READY` 가 없는지 본다. 프로브 `--audio-events <file>` 이 조사 도구로
남았다.

### 67.8 증명

* 단위시험 **261**(options 5, padmap 7, portrait 1). `cargo check -p hl5_android
  --target aarch64-linux-android` 통과.
* `tools_re/scripts/padmap.txt`(`--screen-option --pad --screen 2340x1080`):
  `padmap-list-top/row9/ask/screen/selected/assigned/closed/filter-off/no.png` — 열두
  행, 커스텀 상자, 네 줄 팝업, 맵핑 화면(배지 포함), Y 를 퀵1 에 → 퀵2 배지 사라짐,
  완료 뒤 커스텀, 화면필터 OFF, 아니오 뒤 기본. 요약 `option rows … mapping screen
  opened 1x, 1 binding(s) made`, 파일 `hl5_gamepad.txt` 12줄(`100=1`, `99=NONE`).
* `padmap-portrait.txt`(`--screen-mode p`): 세로 목록·맵핑 화면·R1 을 퀵1 에.
* 세로 배지: `portrait.txt` 에 `pad-connect` 를 넣은 재생의 `portrait-field.png`,
  요약 `field HUD rects buttons [(8,274)…(168,274)] page (200,277) ok (0,0,0,0)`.
* 지연: `latency2.txt` + `--watch TouchPressed/TouchMoved/SetMoveXY` — 누름 N, 이동+
  `SetMoveXY` N+1, `2 drag(s) merged into a press`.
* `bash tools/ab_905.sh padkeys` 1,241 프레임 5장면 동일(`screen_option`·`gamepad`
  는 프로브 기본 꺼짐이라 경로 자체가 없다).
* `option.txt`·`resolution-restart.txt` 는 해상도가 열두 번째 행이 된 만큼 `key down`
  둘과 스크롤 탭을 늘렸다.

### 67.9 남은 것 — 기기 시험

1. 환경설정 열두 행이 스크롤되고 화살표 탭이 끝까지 가는가, 데이터 관리 두 번 탭이 여전히
   SELECT 인가.
2. 화면필터 OFF → 즉시 선명해지는가(재시작 없이), 재부팅 뒤에도 OFF 인가.
3. 패드키 커스텀 → 예 → 화면: 상자 탭 → 버튼 → 배지 이동, 완료 → 필드 배지도 같이
   바뀌는가; 세로에서도. 기본으로 돌리고 다시 커스텀 → 예 → 전 배치가 살아 있는가.
4. 패드 방향키 첫 걸음이 한 틱 빨라졌는가(체감), 그리고 **놓으면 멈추는가**(§52).
5. 타격음 — 가장 중요. S25(Android 16)에서 안 나던 것이 나는가.

### 67.10 둘째 날 — 인게임 메뉴에도, 실제 HUD 위에서, 대시 배지 (2026-10-22)

기기 보고 넷: ① 인게임 메뉴(시스템 > 환경설정)에도 두 행, ② 가로 맵핑 화면은 라벨
상자가 아니라 **실제 버튼 모양**으로(이질감), ③ 세로는 따로 뜨는 상자가 맞다,
④ 가로 필드의 대시 버튼에 배지가 없다.

**인게임 목록 (`hl5_core/src/igoptions.rs`).** `StateInGameMenu::DrawSystemMenu`
(0xEB338) 의 환경설정 페이지는 `DrawOption` 의 사본이다 — 같은 타입표·라벨 id·
`gv[0x84+idx]`·0x17 피치, 행은 여덟(데이터 관리 없음), 커서 바이트가 **반대**
(`this+0x0f` 행, `this+0x10` 인덱스), 그리기 루프 레지스터 r5/sb/sl/r8, 캐시 `first`
는 `[sp+0x54]`, 라벨 배열 `sp+0x58`. 표시 순서 `igoptions::ORDER` 10행(해상도 없음 —
재시작 팝업은 타이틀 것이다). 후크: 루프 첫 검사 **`+0x5c8`**(`cmp r3, sb` — 두
명령 앞의 `ldrsb` 에 걸었더니 아직 `mov sb,#0 / mov r5,r2` 전이라 행 바이트를 슬롯으로
읽고 루프 카운터를 못 세워 **한 줄만 그리고 끝났다**), 꼬리 검사 `+0x77c`, 호스트 행은
꼬리 `+0x764` 로 복귀; `KeySystemMenu` 진입(서브스테이트 **1** — 점프테이블로 2 로
읽었다가 로그로 잡았다, 또 그 함정); `TouchSystemMenu+0x3fc`(**틀렸다 — 그것은 도움말 목록의 화살표다, §68.8**)(아래 화살표 클램프 —
행 수를 **`TextMgr+0x104`** 에서 읽는다, r1 을 10−5 로), `+0x604`/`+0x69c`(위 화살표
랩 둘); `Process+0x9cc`(팝업 id 0x16 의 답 — `+0x7e4` 는 id 0x15 다, 같은 함정 세 번째;
아니오 경로 `+0xf08` 은 `r3 == 1`); `Paint` 진입(맵핑 화면). 팝업은 타이틀로 질문의
id 0x16(`SetPopupState(this, 0x16, msg, 0,0,9, 0)`), `DrawScroll` 총계 8→10 은
`options::scroll_bar_hook` 이 복귀주소로 가른다.

**맵핑 화면은 인게임에서 실제 HUD 다 (`padmap::Scene::InGame`).** `Paint` 자리에서
메뉴의 프롤로그(`CopyFrameBuffer(fb, GetFBOff())` 흐린 필드)·검은 시트 뒤에
`GMenu::DrawSkillBelt(gv+0x15f8, fb, halfW−0x67, H−0x2c, 0,−1,−1)`(필드와 메뉴가
같은 인자), `DrawImageBlending` 로 `DrawTouchUi` 와 같은 호출 — 조이스틱 판·손잡이
(`gv+0x23c/+0x254`, (0x41,H−0x41)), 확인 원판+글리프(`gv+0x17c` f0 α0x5a, `gv+0x194`
f4 α0xff, (W−0x1f,H−0x28), 0x12), 대시 원판+글리프(`gv+0x1f4/+0x20c`, (W,H−0x56),
0x22)와 `CommonUi::DrawTouchCancelUi`(DASH 글자 — **틀렸다, 오른쪽 위 취소 버튼이다, §68.2**), 바 `DrawInfoRect` 셋 + `map shop
save`(`common_text` 0x112..). 그 위에 호스트 배지와 선택 테두리(`padhud::outline`).
히트 사각형은 `pad.ui`(필드가 마지막으로 그린 `StateGame` 값) + 대시 + 바 칸. 상자는
취소·초기화·완료 셋뿐. 세로 인게임은 벨트(게스트 한 줄)만 실물, 나머지 격자.
**타이틀 화면도 실물이다 (셋째 보고 뒤).** HUD 이미지(`gv+0x17c..+0x254`)는 `onStartApp`
이 부팅 때 로드하므로 타이틀에도 있다; 없는 것은 `GMenu`(벨트 칸 그림 `+0x1e0/+0x1f8/+0x210`,
페이지 `+0x228`)와 영웅뿐이다 — `Battle::Init` 이 `new(0x468)` + `GMenu::GMenu()` 로 만들고
생성자가 `LoadGMenuImage` 로 47장을 읽는다. 그래서 호스트가 첫 열림에 같은 생성자로 하나
만들어 두고(`title_gmenu`), `DrawSkillBelt` 가 영웅을 묻기 전에 그리는 빈 칸 여섯·페이지
(0x83854..0x83958 의 `DrawEffect` 셋, 두 배치 공통 코드)를 그 배치의 좌표로 그린다
(`draw_belt_frames`). 확인·대시·조이스틱·바는 인게임과 같은 호출. 제목 상자는 바와 겹쳐 뺐다.

**대시.** 터치 사각형은 `TouchPressed+0x304..` — `x ≥ W−0x23, H−0x65 ≤ y < H−0x47`
= (W−35, H−101, 35, 30) — `StateGame` 필드가 아니라 즉치라 `gamepad::dash_rect` 로
계산하고 `TouchUi.dash` 에 싣는다(`gv[0x1971]` 게이트 — **틀렸다, 그 바이트는 메뉴바의 관문이고 대시는 언제나 그려진다, §68.2**). `padhud::controls` 가
키가 아니라 **action** 을 돌려 대시도 배지를 받는다(기본 START). ★ `gv[0x1971]` 은
**필드-시 플래그**라 메뉴 안에서는 0 — 맵핑 화면은 그 플래그와 무관하게 대시를 넣고
그린다.

인수: `padmap-ingame.txt`(400×240, 좌표가 gamepad.txt 것이라 `--screen` 금지) —
인게임 목록 10행, 팝업, 실제 HUD 화면, Y→퀵1, START→대시, 완료; `padmap-portrait-
ingame.txt`(`--screen-mode p`) — 세로 인게임. 단위시험 266. 1,241 프레임 동일.

**포커스·배지 위치 (넷째 보고).** 가로 벨트는 터치 사각형 `(x, y, 30, 30)` 보다 **6 px 아래**에
그려진다 — `DrawSkillBelt` 가 판을 `(x, y+6)`, 아이콘 중심을 `(x+15, y+21)` 에 놓는다
(0x83880 `add r3, sb, #6`; 0x84240/0x8424c). 테두리와 배지가 터치 사각형을 따라 6 px 떠
있었다. `gamepad::BELT_DRAW_DY` = 6, `drawn_button_rect`. **세로도 같다** — 240폭 분기(0x83748)는
좌표 배열만 채우고 0x837C8 의 공통 그리기로 합류하므로 판은 역시 `(x, y+6)` 이다(처음에
세로 표를 "그려진 자리"로 잘못 두었다가 같은 보고를 받았다). 맵핑 화면 히트도 같은
사각형이라 그림을 누른 대로 잡힌다.

## 68. 배치변경 — 필드 HUD 를 옮기고 키운다 (2026-10-23)

사용자 요구: *"환경설정에서 배치변경도 패드키 기본,커스텀 처럼 … 커스텀으로 바꾸면 팝업뜨고
아니오 누르면 기본으로 돌아가야하는거야. 그리고 커스텀으로 바꾸고 예누르면 실제 게임화면
그대로 뜨고 각 버튼 위치 이동과 크기 조절 할 수있게"* — 크기는 **50~150% 자유롭게**,
**타이틀에도**, **조이스틱도**. hero2/hero3 는 HUD 를 우리가 그려서 숫자만 바꾸면 됐지만
여기는 게임이 그리고 게임이 판정한다(그래서 "얼마나 큰 일인가"부터 따졌다, 이 절의 앞
절반이 그 조사다).

코드: `hl5_core/src/hudlayout.rs`(배치·기하·다시 그리기·터치 번역·행) +
`hl5_core/src/hudedit.rs`(편집 화면). 상태 파일 `<files>/hl5_hudlayout.txt`.

### 68.1 위치는 어디에 박혀 있는가

| 컨트롤 | 그림 | 판정 |
|---|---|---|
| 퀵슬롯 5 + 콤보 | `.rodata` 0x157D1C+0x90.. (넓은 쪽 표) → `DrawSkillBelt` 의 합류점 0x837C8 이후 **41개 그리기 전부**가 이 배열 하나를 따른다(그 뒤 `GetWidth/GetHeight` 0번) | 같은 값의 둘째 표 0x159C38+0x18.. → `SetTouchKeypad` 가 `StateGame+0xe0/+0xec` 에. **`TouchPressed`·`TouchMoved`·`TouchReleased` 가 셋 다 맨 처음 `SetTouchKeypad` 를 부른다** (0xE1BB4, 0xE14F4, 0xE1850) |
| 페이지 | `DrawSkillBelt` 즉치 (W−0xb0, H−0x1b) 가운데 | 같은 함수가 `+0xf8/+0xfa` (W−0xbf, H−0x2a) 30×30 |
| OK | `DrawTouchUi` (W−0x1f, H−0x28) 앵커 0x12, 원판 0x17c + 글리프 0x194 f4 (눌림 0x1ac f7) | `+0xb8` IRect (W−0x3d, H−0x46, 60, 60) |
| 대시 | `DrawTouchUi` (W, H−0x56) **앵커 0x22 = 오른쪽 정렬·세로 가운데**, 0x1f4 + 0x20c f4 (눌림 0x224) | `TouchPressed+0x30c..` 즉치 `x ≥ W−0x23, H−0x65 ≤ y < H−0x47` |
| 취소 | `CommonUi::DrawTouchCancelUi` (W, 0x1e) **앵커 0x21 = 오른쪽·위**, 0x1c4 | **`mainKeyPressed` 0x9CC50** — `x ≥ W−0x24`, `30 ≤ y ≤ 60` → `KeyPress(−16)`, 모든 상태 공통 |
| 조이스틱 | `DrawTouchUi` 다섯 곳: 고정식 (0x41, H−0x41), 부동식은 쥐었을 때만 원점 `gv+0x120/0x122` 에(쉴 때는 **안 그린다**) | `Check_Rect` 두 영역 `(0,30)..(W−210,H)` / `(0,30)..(W−80,H−80)` (포함 경계) |
| 메뉴바 | `DrawTouchUi` 루프 0x93 + 0x32k (+ `hud.rs` 의 dx) | `TouchPressed` 0xE252C.. (`hud.rs` 가 되돌려 준다) |

`Graphic::CalcAnchor`(0x52FE4): 0x10 가로 가운데, 0x20 오른쪽, 그 밖 왼쪽; 0x02 세로
가운데, 0x04 아래, 그 밖 위. **대시와 취소는 화면 밖으로 잘린 원이 아니라 오른쪽에 붙인
반원 그림이다** — 안쪽으로 옮겨도 반원 그대로다.

이미지 크기(`GbmImage+2/+4`, `hudaudit` 실측): OK 55×54, 대시 36×54, 취소 36×32,
조이스틱 판 75×73 / 손잡이 45×44, 페이지 25×21.

### 68.2 기존 기록 정정 둘

1. **`DrawTouchCancelUi` 는 DASH 글자가 아니다** (§67.10 이 그렇게 적었다). 오른쪽 위의
   취소(↻) 버튼이다. DASH 글자는 대시 글리프 이미지의 프레임 4 다. 필드 밖의 모든
   상태는 `mainPaint`(0x9C7A8, 상태 id > 1) 가 같은 버튼을 그린다.
2. **`gv[0x1971]` 은 대시의 관문이 아니라 메뉴바의 관문이다.** `DrawTouchUi` 0xE0764 는 0
   일 때만 바를 그리고 바 루프 끝(0xE0D98)에서 취소·대시 코드 0xE0778 로 **합류한다** —
   취소·대시는 언제나 그려진다. `TouchPressed` 0xE1D7C 도 0 일 때만 바를 먼저 보고
   대시 판정은 언제나 한다. `gamepad::dash_enabled` 가 이 바이트를 "대시 있음"으로 읽어서
   **바가 보이는 평소 필드에서 대시 배지가 빠졌다** — 고쳤다(`TouchUi.dash` 는 가로 필드에서
   언제나 `Some`).

### 68.3 방법 — 게임 자신의 픽셀로 다시 그린다

게스트 코드는 한 바이트도 고치지 않는다. 두 그리기 함수를 진입에서 후킹한다:
`GMenu::DrawSkillBelt`(복귀주소 `Battle::DrawGameUI+0xa9c` 일 때만 — 메뉴 넷이 같은 표로
벨트를 그리지만 그 화면들은 원래 자리 그대로 둔다)와 `StateGame::DrawTouchUi`(호출자는
`StateGame::Paint` 의 꼬리 `b` 하나뿐). **둘 다 순수하다** — 스택 밖 쓰기가 0곳이다
(`GetTouchJoyXY` 는 출력 포인터 둘에만 쓴다). 그래서 같은 프레임에 여러 번 불러도 된다.

* **캡처 버퍼** = 게스트 자신의 `MC_grpCreateOffScreenFrameBuffer(W, H)` 하나(첫 사용 때).
  호출 동안 `Graphic+0`(= `GetFB`) 을 그것으로 바꿔 끼운다. 구조체 `[w, h, bpl, bpp, base]`,
  565 만(`bpp == 16`).
* 검은 바탕에 한 번(`B = αF`), 흰 바탕에 한 번(`B + (1−α)`) — `T = W − B` 가 채널별
  투과율이다. 합성은 `out = B' + T'·D`, `B`·`T` 를 **면적 가중**(박스 필터)으로 리샘플한다
  (사전곱이라 선형 필터가 정확하다). 100% 제자리면 탭이 픽셀 하나라 불투명 픽셀은 원본과
  비트까지 같고, 반투명 픽셀은 565 양자화로 1~2 LSB 다르다(실측 최대 12/255).
* **자르기**: 벨트는 일곱 중심점(아이콘 중심, 페이지 중심) 보로노이 — 판 그림자가 맞닿을 뿐
  겹치지 않는다. 터치 UI 는 영역(그림 크기 + 1 px)으로 자르는데 **OK 원판과 대시 반원은
  오른쪽 끝에서 테두리가 겹친다**(대시 H−113..H−59, OK H−67..H−13). 한 장으로는 못 가르므로
  **두 번 나눠 찍는다** — 대시를 뺀 패스와 OK 를 뺀 패스. 빼는 방법은 `DrawImageBlending`
  공유 디스패처(10페이지 탭과 공유, `Vm::draw_image_blending`)의 팔 하나가 복귀주소
  `DrawTouchUi+0x118/+0x180`(OK) 또는 `+0x210/+0x270`(대시) 을 건너뛰는 것이다(눌림
  글리프도 같은 호출로 합류한다: 0xE074C, 0xE083C).
* `hudaudit`(프로브 명령)이 두 묶음을 캔버스 **전체**로 찍어 영역 밖 픽셀을 센다 — 0 이다.
* **캐시**: HUD 는 그려지는 것보다 훨씬 덜 바뀐다. 검은 바탕 결과가 직전과 같으면 흰 바탕
  패스를 건너뛴다(소유자 맵도 사각형이 같으면 재사용). 실측(400×240 필드, 열두 컨트롤 중
  여덟을 옮김): 프레임당 게스트 명령어 313,763 → 캐시 전 476,825(+52%) → **캐시 후
  364,479(+16%)** — 남은 것이 대시/OK 분리용 추가 패스 하나다.
* 기본 모드(또는 묶음 안에 옮긴 것이 없으면) 후크는 사양한다. 기본 모드에서도 하는 일은
  편집기 배경용 스냅숏뿐이다(아래).

### 68.4 터치 — 옮긴 그림을 누르면 원래 자리로

`Vm::touch` 가 `hud.unshift` 앞에서 `hudlayout::route` 를 부른다(패드의 가짜 손가락
0x70 은 이미 게스트 좌표라 제외). 필드일 때만 — `DrawTouchUi` 의 조기반환 셋과 같은 검사
(`gv+0x27c` 상태 id == 1, `gv+0x148c → +0x79 == 0`, `gv+0x15f0 → +4 == 1`) +
`GetState()`(`[this+0xc]`) == 0 + 정령 스킬창이 아님.

* 누름: 맨 위 컨트롤부터(터치 UI 를 벨트 위로, 각 묶음은 그린 역순) **새 몸체**에 들면
  `guest = 피벗 + (손가락 − 새 피벗)/배율` 을 그 컨트롤의 **게스트 판정 사각형 안으로**
  클램프해서 넘기고, 같은 손가락의 이동·뗌은 같은 변환을 탄다(누름의 클램프 보정 포함).
* 조이스틱: 영역 전체를 이동량만큼 옮긴다. 고정식은 `C + clamp_len((p − C')/s, 60)`
  (각도만 쓰므로 길이를 잘라도 같다), 부동식은 누른 점 − 이동량을 영역 1 안으로 클램프한
  곳이 원점이고 이동은 `/s`. 그려진 판은 그 손가락의 경로로 손가락 밑에 놓인다
  (`stick_user`, `gv+0x124` 가 주인).
* 컨트롤이 **떠난 자리**(옮겨진 컨트롤의 원래 판정 사각형, 옮긴 조이스틱의 원래 영역)를
  누르면 그 손가락은 삼킨다 — 게스트는 그 손가락을 모른다.
* 나머지는 그대로.

오프라인 증명(커스텀 배치, `--watch` 로 게스트 판정 블록):

| 누른 곳 | 도달 |
|---|---|
| 새 OK | `TouchPressed+0x8a0`(OK arm 0xE2328) |
| 옛 OK | 없음 |
| 새 퀵1 | `+0x270`(벨트 arm 0xE2470) |
| 옛 퀵1 | 없음 |
| 새 대시 | `+0x350`(0xE1DD8) |
| 옛 대시 | 없음 |
| 옮긴 조이스틱 | `+0x6b4`(고정식 잡기 0xE2148) |
| 새 취소 | `mainKeyPressed+0x264`(0x9CCB4) |

요약 `5 finger(s) routed, 3 swallowed`.

### 68.5 편집 화면

* 행: 타이틀 `options::ORDER` **13행**(패드키 9, **배치변경 10**, 데이터 관리 11, 해상도
  12), 인게임 `igoptions::ORDER` **11행**(배치변경 10). 값 `기본/커스텀`, 세로나 320 이하
  캔버스는 `-`(설치 안 됨). 팝업은 패드키와 같은 두 버튼(타이틀 id 8 / 인게임 0x16) —
  `버튼 배치를;변경하시겠습니까?;아니오를 누르면;기본 상태로 진행됩니다.` — 답은
  `pending` 일 때만 가로챈다(두 질문 중 어느 것인지로 갈라서).
* **배경**: 인게임은 **흐림 없는 필드**. 메뉴가 열릴 때 게임이 필드를 `BoxBlur565` 로
  흐리므로 선명한 원본이 남지 않는다 — 그래서 필드 프레임마다 터치 후크 진입에서
  캔버스를 복사해 두고, 벨트 영역만은 벨트 후크 진입 때의 것(벨트 그리기 전)으로 갈아
  끼운다: 상태바·미니맵은 있고 조작부는 없는 그림. 타이틀은 패드키처럼 어두운 막.
* 조작부는 `hudlayout::draw_hud` 가 게임 그림으로 편집 중 배치에 그린다(인게임 벨트는 영웅의
  `GMenu` 로 실제 아이콘, 타이틀은 패드키가 만든 `GMenu` 의 빈 칸). 패드키 맵핑 화면도
  같은 함수를 쓴다 — **커스텀 배치가 맵핑 화면에도 그대로 나온다.**
* `mainPaint` 가 그리는 원래 자리의 취소 버튼은 편집기 동안(그리고 취소를 옮긴 채 맵핑
  화면 동안) `DrawTouchCancelUi` 진입 후크가 건너뛴다.
* 손가락 하나로 끌기, 잡은 채 둘째 손가락으로 크기(거리 비, 50~150%, 1% 단위),
  `−`/`+` 는 5% 단위로 다음 배수에. 몸체는 언제나 캔버스 안(`clamp_place`). 초기화 = 전부
  제자리, 완료·BACK·패드 B = 저장하고 커스텀. 테두리는 호스트가 완성 캔버스에(선택은 주황).
* 파일: `mode custom|default` + 열두 줄 `<이름> <dx> <dy> <퍼센트>`(이름 `slot1 slot2 combo
  slot3 slot4 slot5 page ok dash cancel stick bar`). 오프셋은 각 컨트롤이 붙은 모서리
  기준이라 FULL/90%/80% 사이에서 뜻이 유지되고, 부팅 때 캔버스 밖으로 나간 것은 당긴다.
  기본으로 돌려도 파일은 남는다(패드키와 같다).

### 68.6 증명

* 단위시험 **282**(배치 파일 왕복·기하·보로노이 소유·영역 분리·리샘플러 가중 합 1·
  터치 맵·조이스틱 반경·편집 위젯 위치, 목록 13/11행).
* `tools_re/scripts/hudlayout.txt`(타이틀, 585×270): 행 → 팝업 → 아니오(기본) → 예 → 편집
  → OK 끌기 → 두 손가락 150% → 조이스틱 −,−,+ = 95% → 완료(커스텀). 파일
  `ok -254 -80 150`, `stick 0 0 95`.
* `tools_re/scripts/hudlayout-ingame.txt`(400×240): 인게임 행 → 편집(선명한 필드) → OK
  130% → 완료 → 필드: 새 OK 를 누르면 `0xE2328` 에 도달, 옛 자리는 아니다.
* **HEAD 대비 1,241 프레임 5장면 바이트 동일 — `--screen-option` 으로**(후크가 설치된 기본
  모드). 표준 `ab_905.sh`(패치 vs `--no-patch`) 도 동일.
* 10페이지(`--ten-pages`, 공유 디스패처 경유) 1,510 상자 이동 그대로, 세로는 `-` 로 설치 안 됨.
* `option.txt`·`resolution-restart.txt` 는 해상도가 열세 번째 행이 된 만큼 `key down`
  하나·스크롤 탭 하나씩을 더했다(재시작 팝업 4, `MC_knlExit` 1 그대로).

### 68.7 기기에서 볼 것

1. 인게임 시스템 > 환경설정 > 배치변경 → 커스텀 → 예: **선명한 필드** 위에 조작부가 그대로
   나오는가. 끌기, 잡고 둘째 손가락으로 크기, `−`/`+`.
2. 완료 후 필드에서 **옮긴 버튼이 그 자리에서 눌리고, 원래 자리는 아무것도 안 하는가**.
   조이스틱은 고정식·부동식(터치방식 행) 둘 다.
3. 150% 로 키운 버튼과 50% 로 줄인 버튼의 그림 품질(면적 가중 리샘플, 캔버스 해상도).
4. 기본으로 돌리면 원래 배치, 다시 커스텀 → 예 → 전 배치가 살아 있는가.
5. 패드가 붙어 있으면 배지가 옮긴 버튼을 따라가는가(대시 배지는 이제 바가 있어도 나온다).
6. S9 에서 필드 프레임률 — 커스텀 배치의 추가 비용은 캐시 후 프레임당 게스트 명령어 +16%.

### 68.8 둘째 날 — 인게임 목록이 세 번째 스크롤에 맨 위로, 그리고 어디서든 핀치 (2026-10-23)

기기 보고 둘: ① 환경설정에서 아래로 두 번 스크롤하고 세 번째(가운데쯤)에서 맨 위로 튄다,
② 버튼을 골라 둔 뒤 화면 아무 데서나 두 손가락으로 확대·축소하면 크기가 바뀌게.

**① 원인은 §67.10 의 후크 자리다.** `StateInGameMenu::TouchSystemMenu` 는 `GetSubState()`
점프테이블(0xFE4B8) 하나이고 갈래들이 꼬리를 공유한다. 환경설정(서브스테이트 1, 0xFE7B8)의
행 루프는 0xFE554 로 끝나 0xFE564.. 의 화살표 코드로 떨어지고, 거기 아래 화살표가
**0xFE5A4 `cmp r3, #2`** — `first + 1 > 2` 면 0xFE7A8 이 커서를 맨 위로 되돌린다(원래 8행 ·
6줄: 마지막 창 first 2). §67.10 은 `+0x3fc`(0xFE7A0, `TextMgr+0x104` 행 수와 비교)와
`+0x69c`(0xFEA40, 위 화살표 감기)를 걸었는데, 그 둘은 **서브스테이트 2 = 도움말 목록**의
화살표다(도움말을 열고 아래 화살표를 누르면 0xFE7A0 에 닿는 것을 `--watch` 로 확인). 그래서
환경설정은 세 번째 스크롤에서 게스트 원래 규칙대로 맨 위로 갔고, 도움말은 자기 항목 수
대신 10(·11)행 기준으로 잘렸다.

고침: 두 후크를 떼고 `+0x200`(0xFE5A4)에 하나 — `r3`(= `first + 1`)이 게스트의 끝 창
다음(3)부터 우리 끝 창(11행이면 5)까지면 스크롤 경로 `+0x208`(0xFE5AC)로 재개, 나머지는
게스트 몫(끝 창에서 한 번 더 누르면 원래처럼 맨 위로). 위 화살표 감기 `+0x604`(0xFE9A8)는
처음부터 맞는 갈래였다. 재생: 아래 화살표 1~5번이 0xFE5AC(스크롤)에 닿고 여섯 번째만
0xFE7A8(맨 위로); 화면은 다섯 번째에 `배치변경` 이 맨 아래인 마지막 창. 타이틀 목록
(`TouchOption+0x2ac`)은 처음부터 맞았다(`option.txt` 의 터치 스크롤 일곱 번).

**② 핀치는 고른 컨트롤에 건다.** 편집기의 손가락 처리를 Vm 없는 상태기계
(`hudedit::Edit::finger_down / finger_move / finger_up`)로 다시 썼다:

* 첫 손가락은 누르는 순간 그 밑의 컨트롤을 고른다(주황 테두리) — 그러나 그 전에 고른 것을
  기억해 두고, 4 px(`MOVE_SLOP`) 움직이기 전에 둘째 손가락이 내려오면 **그 전 것을 돌려주고
  그것을 핀치한다.** 핀치 손가락이 우연히 다른 버튼 위에 닿아도 고른 버튼이 바뀌지 않는다.
* 4 px 를 넘게 움직인 첫 손가락은 누른 컨트롤을 끈다(누른 점 기준이라 슬롭만큼 튀지 않는다).
  끄는 중에 둘째 손가락이 오면 끌기는 멈추고 핀치가 된다.
* 두 손가락 사이 거리 비 × 처음 크기, 50~150%, 1% 단위. 한 손가락을 떼면 핀치가 끝나고
  남은 손가락은 뗄 때까지 아무것도 안 한다.
* 빈 곳을 **탭**하면(4 px 안에서 떼면) 선택이 풀린다 — 누르는 순간이 아니라 뗄 때라서
  빈 곳에서 시작하는 핀치가 선택을 지우지 않는다.
* 편집기 버튼(초기화·완료·−·+)은 첫 손가락에만 반응한다.

안내 문구: `끌어서 옮기고, 고른 뒤 두 손가락으로 크기 조절`.

증명: 단위시험 **287**(+5: 탭으로 고르기/빈 곳 탭으로 풀기, 빈 곳 핀치 150%·75%·하한 50,
다른 버튼 위에서 시작한 핀치가 고른 것을 지키기, 끌기 후 핀치, 고른 것 없이 핀치는 무시).
`hudlayout.txt` 에 `hudlayout-pinch-anywhere.png` — 조이스틱을 고른 채 화면 가운데 빈 곳에서
80 → 60 px 핀치, 95% → 71%(파일 `stick 0 0 71`).

### 68.9 `데이터 관리` 를 목록에서 뺀다 (2026-10-23)

사용자 지시: *"환경설정에서 데이터 관리라는 항목 삭제해줘"*. 게스트의 아홉째 행(인덱스 8, 값
상자 없는 실행 행)을 `options::ORDER` 에서 뺐다 — 표시 표에 없는 게스트 행은 행 루프 후크가
한 번도 가리키지 않으므로 그려지지 않고, UP/DOWN 은 호스트가 표시 공간에서 답하므로 커서도
닿지 않는다. 다른 입구는 없다(인게임 목록에는 원래 없다). 타이틀 목록은 **12행**:
패드키 9, 배치변경 10, 해상도 11.

`TouchOption+0x24c` 후크는 남는다 — 게스트는 커서 행을 두 번째로 탭하면 **표시 인덱스 8**
을 SELECT 로 읽는데, 표시 8 은 터치방식이다. 이제 그 후크는 언제나 값 화살표로 보낸다.
`option.txt`·`resolution-restart.txt` 는 `key down` 하나·스크롤 탭 하나씩을 도로 뺐다(재시작
팝업 4, `MC_knlExit` 1 그대로). 단위시험이 `display_of(8) == None` 을 지킨다.

### 68.10 메뉴 안에서도 커스텀 배치 — 뒤 배경과 단축키 등록 (2026-10-23)

사용자 보고 둘: *"커스텀 한뒤에 메뉴 열렸을떄 뒤에 배경은 … 기본으로 나오고 있는데 … 커스텀한걸로
나왔으면"*, *"스킬이나 포션 등록시에도 내가 커스텀한 UI내에서 등록되게 해야해."* §68.3 은 벨트를
`Battle::DrawGameUI+0xa9c` 복귀주소일 때만 옮겼고, 메뉴의 벨트는 원래 자리에 두기로 했었다 —
그것을 뒤집는다.

**그림.** `GMenu::DrawSkillBelt` 의 호출자는 셋이다: 필드(`DrawGameUI+0xa98`; 메뉴 생성자가
배경을 만들 때도 이것을 돈다), **메뉴의 모든 페이지**(`StateInGameMenu::Paint+0xb4`, 흐린
벨트, 모드 0), **등록 화면**(`DrawSkillMenu+0x1cb4` 와 가방·특성의 같은 자리, 모드 1).
벨트 후크가 이제 셋 다 같은 배치로 다시 그린다 — 한 표에서 같은 벨트를 그리므로 어디서나
같은 자리여야 한다. 취소(↻)는 `mainPaint+0xc0` 이 상태 id > 1(2·5·6 제외)에서
`CommonUi::DrawTouchCancelUi` 로 그리므로 메뉴에도 있고, 그 복귀주소(0x9C7AC)의 후크가 옮긴
자리에 그린다. 그 판정은 모든 상태 공통인 `mainKeyPressed` 0x9CC50 이라 `route()` 가 필드
밖에서도 취소 탭만은 번역한다 — **그 프레임 화면에 옮긴 취소가 있었을 때만**
(`cancel_drawn` → 재칠 시계로 `cancel_on_screen`).

**등록 판정.** 세 `Touch*Menu` 는 다섯 칸(퀵1 퀵2 퀵3 퀵4 퀵5)의 `x = W − tx`,
`y = H − 6 − ty` 를 `.rodata` 표(0x15A040 / 0x15A138 / 0x159F60, 넓은 화면 표)에서 스택에
만들고 페이지 `(W − 0xbf, H − 0x2a)` 와 함께 `(x, y, 31, 31)` 로 판정한다:

| 페이지 | 등록 서브스테이트 | 후크(스택 배열 완성 직후) | 칸 명중 |
|---|---|---|---|
| 스킬 | 6 / 7 | `TouchSkillMenu+0x328` | 0x102488 — 첫 탭 `this+0x11` = 그 칸, 같은 칸 둘째 탭 `KeyPress(−5)` |
| 가방 | 2 | `TouchBagMenu+0x594` | 0x103928 |
| 특성 | 4 / 5 (점프테이블 case 4·5 → 0x101378) | `TouchPropertyMenu+0x1ec` | 0x1013B4 |

후크는 옮긴 칸의 i16 둘을 `피벗 + d − 15` 로 다시 쓰고 사양한다(크기는 판정 31 px 를 그대로
둔다 — 가운데 맞춤). 옮기지 않은 칸은 원래 값 그대로다.

**등록 표시가 찢어졌다.** 모드 1 의 `DrawSkillBelt` 는 꼬리(0x83D08)에서 커서 칸 `[sp+4]`
의 표 자리를 기준으로 하이라이트(`DrawEffect` x+5, y+11), 20×20 판(`FillRect` x−5, y+1),
올릴 아이템 `[sp+8]` 의 아이콘을 칸 밖 ~10 px 까지 그린다. 커서는 할당 가능한 다섯 칸을
세고 표는 콤보(2)를 건너뛴다(c ≤ 1 → c, 아니면 c+1). §68.3 의 보로노이 자르기는 그 표시를
커서 칸과 이웃 칸(페이지)에 나눠 각자의 오프셋으로 옮겨서 조각이 났다. 고침
(`belt_cursor`): 모드 바이트 `[sp]` 를 0 으로 한 번 더 찍고(함수 안에서 그 바이트의 독자는
0x83D08 하나다) **두 그림이 다른 픽셀을 커서 칸에** 준다. 이웃은 표시 없는 그림에서(그
밑에 가렸던 부분도 온전히), 커서 칸은 표시 있는 그림에서 맨 마지막에(게스트도 마지막에
그린다). 등록 화면에서만 캡처가 한 벌 더 들고, 캐시가 그 절반을 덜어 준다.

**증명** (`hudlayout-menu.txt`, 퀵1 −200,−90 · 퀵5 −40,−40 · 페이지 −170,−60):

* 세 페이지 다 `echo new-slot1`(옮긴 퀵1 의 가운데) 뒤에 그 페이지의 칸 명중 주소가 뜨고,
  `echo old-slot1`(원래 자리) 뒤에는 안 뜬다. 요약 `6 menu belt touch(es) laid out`,
  `96 with a registration mark`.
* 프레임: 메뉴 뒤 벨트가 옮긴 자리(`hudlayout-menu-backdrop.png`), 등록 표시가 옮긴 퀵1 위에
  온전히(`-skill`·`-bag`·`-property`). 같은 장면을 기본 배치로 찍어 그 칸을 탭하면 표시의
  모양(첫 탭 뒤 아이콘 둘이 자리를 바꾸는 것까지)이 같다.
* 새 스크립트 명령 `menu-substate <n>` 은 게스트 자신의 `StateInGameMenu::SetSubState` 를
  화면의 상태에 부른다 — 등록 화면까지 걸어가는 대신이다.
* 단위시험 288(+1: 커서 → 칸 대응, 모드 0·커서 −1·5 는 표시 없음). 두 재생(`hudlayout.txt`,
  `hudlayout-ingame.txt`)은 전과 같고, 1,241 프레임 A/B 동일.

**기기에서 볼 것**: 스킬에서 스킬 → 단축키 등록 → 옮긴 칸 두 번 탭, 가방에서 물약 → 같은 것,
특성 → 같은 것. 등록 중 올리는 아이콘이 옮긴 칸에 붙어 있는가. 메뉴 뒤 벨트·↻ 가 옮긴 자리이고
↻ 를 누르면 메뉴가 닫히는가.

## 69. 패드 입력 순서 — 짧은 십자 탭이 사라지던 두 길 (2026-10-23)

코드 리뷰 지적 둘(P1·P2), 둘 다 사실이었다. 리뷰의 재현 로그는
`out/review-input/trace.txt`, 재생은 `out/review-input/rapid.txt`.

### 69.1 P1 — 우편함 다섯 칸을 게스트는 정해진 순서로 비운다

`mainKeyPressed`(0x9CA18)의 몸통(0x9CA5C~):

```
if gv+0xF0 (누름)  → TouchPressed  … 0x9CA74 로 합류   ← 이동 칸을 건너뛴다
else if gv+0xF2 (이동) → TouchMoved
0x9CA74: if gv+0xF1 (뗌)   → TouchReleased
         if gv+0xE4 (키다운) → KeyPress
         if gv+0xE5 (키업)   → KeyRelease
```

`Vm::pad_pump` 는 큐의 앞 사건을 **그 칸이 비었으면** 넣었다. 칸이 종류마다 하나라
같은 종류의 둘째는 막지만, 다른 종류끼리의 순서는 위 표가 정한다. 그래서 셋이 뒤집힌다:

| 패드가 낸 순서 (한 틱 안) | 게스트가 처리한 순서 | 결과 |
|---|---|---|
| 누름, 이동, 뗌 | 누름 · 뗌 / 다음 틱 이동 | 이동이 주인(`gv+0x124`) 없는 스틱에 닿아 버려진다 — **짧은 탭이 안 걷는다** |
| 뗌(앞 탭), 누름(다음 탭) | 누름 · 뗌 | 같은 포인터 id(0x70)라 옛 뗌이 새 손가락을 놓는다 — **둘째 탭이 사라진다** |
| 키업, 키다운 | 키다운 · 키업 | 누르고 있는 키가 떼진 것으로 읽힌다 |

누름을 받은 틱의 이동 건너뛰기 때문에 이동은 누름 다음 틱에 처리되므로, 두 틱(≈72 ms)보다
짧은 탭이 전부 첫 줄에 걸린다. 리뷰 재생: 같은 프레임의 `pad-stick 1 0` / `pad-stick 0 0`
→ `WorkMoveHero` 관문 통과 0회, 12프레임 유지 → 5회.

**고침 — `PadOut::waits_for`.** 사건마다 "이 칸들에 **패드의** 앞선 사건이 남아 있으면
기다린다"를 둔다: 뗌 ← 누름·이동, 누름 ← 뗌, 키다운 ← 키업. 기다리는 칸이 **다른
포인터**(진짜 손가락)의 것이면 안 기다린다 — 스틱 주인 판정이 포인터로 갈리므로 그
순서는 상관없고, 화면을 끄는 손가락이 패드의 뗌을 굶기는 일을 막는다. 터치 칸의 포인터
id 는 누름 `gv+0x10C` / 뗌 `gv+0x110` / 이동 `gv+0x114`(`mainKeyPressed+0xcc/+0x1d4/+0x210`
이 세 번째 인자로 넘긴다). 키 칸은 포인터가 없어서 차 있으면 패드의 것으로 본다.
`Pad::next_ready_in_order` + `Slot`, 요약 줄 `poll(s) held back for order`.

결과 (같은 재생 + 두 번 연타 추가):

| | 고치기 전 | 고친 뒤 |
|---|---|---|
| 짧은 탭 | 0 | **1** |
| 12프레임 유지 | 5 | 5 |
| 짧은 탭 둘 | 0 | **2** |

`gamepad.txt` 58 · `portrait.txt` 29 는 고치기 전과 같고(그 재생들에서는 대기가 0번),
1,241 프레임 A/B 동일. 단위시험 +5: `Guest` 가 위 표의 순서를 그대로 흉내 내는
모형이고(누름이 게스트 자신의 이동을 같이 넣는 것, `pad_pump` 의 병합까지), 짧은 탭·
두 번 연타·유지·키 재누름·남의 손가락을 잰다. 규칙을 끄면 셋이 실패한다.

### 69.2 P2 — 한 이벤트에 묶인 HAT 누름·중립

안드로이드는 한 프레임의 `ACTION_MOVE` 를 이벤트 하나로 묶고 앞 샘플을 히스토리로 둔다.
`MainActivity.dispatchGenericMotionEvent` 는 최신 값만 넘겼으므로, 십자키를 HAT 축으로
보고하는 패드에서 누름 → 중립이 한 프레임 안이면 중립만 갔다. 이제 히스토리 샘플 중
**HAT 이나 트리거 값이 바뀐 것**을 순서대로 먼저 보내고 마지막에 최신을 보낸다. 스틱
위치는 여전히 최신만 쓴다(그 샘플을 보낼 때의 스틱 값이 같이 가는 것뿐이다). 네이티브
쪽(`push_pad`)은 합치지 않고 순서대로 처리하므로, 누름·중립이 같은 폴에 도착해도 §69.1
의 규칙이 한 걸음을 보장한다.

### 69.3 타격음 — 바꾸지 않았다

리뷰도 새 원인을 확정하지 못했다. 로드마다 `openFd` 하는 §67 고침은 들어가 있다.
비동기 로드 완료 확인·재시도는 없지만 그것이 증상의 원인이라는 근거가 없어 손대지 않았다.

**기기에서 볼 것**: 필드에서 십자키를 아주 짧게 톡톡 칠 때마다 방향을 틀며 한 걸음씩
가는가(특히 HAT 방식 패드), 연타가 빠짐없이 먹는가, 놓으면 멈추는가.

## 70. 세이브 내보내기·불러오기·되돌리기 — 폰을 바꿀 때 (2026-10-25)

사용자 요구: 플레이어가 "세이브 파일 경로"를 물었다. 원하는 것은 다른 폰으로 옮기는
것이다. 그런데 세이브는 앱 내부 저장소에 있고, 릴리스 APK 는 debuggable 이 아니라
`run-as` 가 막혀 있다. targetSdk 35 라 `adb backup` 도 앱 데이터를 빼고(Android 12+),
`-DumpSave`(§43) 는 빼내기만 되지 넣는 길은 없다. 그래서 **루팅 없이 옮기는 길이 0개**였다.
환경설정 맨 밑에(타이틀·인게임 둘 다) 세 행을 넣었다. 사용자 결정 셋: 인게임 내보내기는
게임 저장을 먼저 돌린다 / 패드키 맵핑은 같이 옮긴다(키코드→동작이라 기기와 무관), 배치변경은
안 옮긴다(이 화면의 좌표) / 되돌리기 행을 둔다.

`hl5_core/src/savebundle.rs`(형식·디스크, 순수 함수) + `savexfer.rs`(메뉴·팝업) +
`hl5_android/src/savedoc.rs` + `Hl5Save.java`.

### 70.1 세이브가 무엇인가

* 게스트: `<storage>/data/data/co.kr.eamobile.HeroesLore5/files/` 의 **`kickass` 하나에
  캐릭터 전부**(§18.2 의 MIDAS 컨테이너), 그리고 63 B `LOCAL.sav`. 기기의 실제 경로는
  `/data/data/kr.heroeslore.five/files/files/data/data/co.kr.eamobile.HeroesLore5/files/`
  — `getFilesDir()/files` 가 셸 저장소이고(`lib.rs` `storage`) 그 아래에 게스트가 원본 경로를
  붙인다.
* 컨테이너: `ff 56 30 00` + 20 B `[이름 16][크기 4]` 항목 표(800 B 예약, 나머지 0) +
  몸통. 항목은 `M.sav / Heroslore5 / ET.sav / EX.sav / OP.sav` + 캐릭터마다
  `SL_n.sav / H_n.sav (/ B_n.sav)`. probe 저장소 249개 전수: 마법수 249/249, 캐릭터 수
  0(옵션만, 2,340 B) 155개 / 1명 87개 / 3명 7개 — **`SL_` 항목 수 = 캐릭터 수**.
* 호스트: `<storage>/hl5_warehouse.txt`·`hl5_warehouse_upper.txt`(+ 두 파일 커밋의
  중간 산물 `.new`, §65.12)·`hl5_avatar.txt`. **세이브만 옮기면 공용창고가 사라진다.**
* DES 키는 상수다 — `onStartApp` 이 `MX_desInit` 에 `.rodata` 0x1588B0 의 문자열을
  넘긴다(`add r0, r4, #-0xb5e0`). 그래서 **세이브는 폰에 묶여 있지 않다.** 호스트는
  아무것도 복호화하지 않는다 — 파일을 바이트 그대로 나른다.

### 70.2 왜 다음 실행 때 바꾸는가

게스트는 열린 세이브를 힙에 들고 저장할 때마다 컨테이너 전체를 다시 쓴다. 실행 중에 파일을
바꾸면 다음 저장이 옛 상태로 덮는다. 그래서 메뉴는 **예약만** 하고(`hl5_save_import.h5save`)
게임을 닫으며, 셸이 `android_main` 에서 **어떤 파일도 읽기 전에**(패드 맵핑 포함)
`savebundle::apply_pending` 을 부른다:

1. 예약 파일을 다시 검사(여기서는 무결성만),
2. 바뀔 파일 전부를 보관함에 복사 — **복사 못 하면 적용하지 않는다**,
3. 예약 파일 → `.applying` 이름 바꾸기 = 커밋,
4. 멤버마다 temp + fsync + rename, 번들에 없는 멤버는 지운다(패드 묶음은 번들이 그 묶음을
   주장할 때만),
5. `.applying` 삭제, 보관함 회전(최신 5개).

`.applying` 이 남아 있으면 4 에서 죽은 것이라 한 번 더 마저 한다 — 그 전에 `.rollforward`
로 이름을 바꾸고, `.rollforward` 를 만난 시작은 포기한다(시작할 때마다 죽는 고리 방지).
검사에 실패한 예약 파일은 지우고 아무것도 안 바꾼다.

### 70.3 팝업은 전부 게임 것이다

| | 타이틀 `StateMainMenu` | 인게임 `StateInGameMenu` |
|---|---|---|
| 안내(한 버튼, `common_text 0x38` = OK) | `SetPopupState` id 1 → 0x11B758 팔. `Process` 의 id 1 팔(0x124A84)은 답이 0 이면 지운다. 뒤로가기로 닫아도 목록은 계속 움직인다(실측) | id 1 → +0x1D0 팔. 게임의 `저장되었습니다.` 가 쓰는 그 id(0x10AE60). `Process` 팔(0x10A77C)은 무조건 지운다 |
| 질문(예/아니오) | id 8 — 답은 `options` 의 `Process+0x698` 후크 | id 0x16 — `igoptions` 의 `Process+0x9cc` 후크 |

파일 화면은 비동기라 결과를 **그 메뉴의 다음 `Process` 진입**에서 처리하고(후크 둘, 진입점은
비어 있었다), 팝업을 띄운 뒤 **즉시 반환**한다. `Process` 의 첫 일이 `GetPopupState` 스위치라
계속 가면 방금 띄운 id 를 결과 0 으로 읽어 같은 호출에서 지워 버린다. 게스트도 그 스위치
앞에서는 팝업을 띄우지 않는다. 대가는 메뉴 `Process` 한 틱(파티클·네트워크 폴)이다.
게스트 자신의 팝업 답이 기다리는 틱(`GetPopupState() != 0`)에는 손대지 않는다.
답 후크 안에서 띄운 팝업은 게스트의 아니오 경로가 지우므로, 실패 안내는 다음 진입으로 미룬다.

### 70.4 인게임 내보내기는 게임 저장을 먼저

시스템 › 저장하기가 부르는 것과 같다: `HERO::SaveAll(gv+0x1474, (i8)gv[0x1970])`
(`StateInGameMenu::Process+0x810`). **단 `gv[0x1988]` 이 0 이 아니면 저장하지 않는다** —
`KeySystemMenu+0x7c4` 가 그 바이트를 보고 `common_text 0xf7` = `저장/조합/합성/분해를
할 수 없는 지역입니다.` 를 띄우는 자리라, 내보내기가 그 규칙을 넘으면 안 된다. 그때는 마지막
저장을 내보내고 안내가 `저장할 수 없는 지역이라 / 마지막으로 저장한 세이브를 / 내보냈습니다.`
로 바뀐다. 타이틀에는 열린 세이브가 없으므로 파일이 곧 세이브다.

### 70.5 번들

압축 없는 zip(PC 에서 그대로 열린다) — `manifest.txt`(`hl5 save bundle 1` / `exported
<날짜>` / `groups save[ pad]` / `characters n` / `line main|vanilla`) + 있는 멤버.
읽기는 일부러 엄격하다: 저장 방식(method 0)만, 암호화·데이터 디스크립터 없음, CRC 전부,
목록에 있는 이름만, 중복 없음, 멤버 1 MB · 전체 4 MB 이하, `kickass` 는 컨테이너 모양.
불러오기 정책은 하나 — **캐릭터 0명 번들은 거절**(`캐릭터가 없는 세이브입니다.`): 이 폰의
캐릭터 전부를 아무것도 아닌 것으로 바꾸는 것뿐이라서. 보관함 복원은 이 정책을 안 탄다.
`exported` 는 숫자·`-`·`:`·공백만 남겨(16자) 게스트 팝업에 들어간다.
**패드 묶음**: 원본 폰에 패드 파일이 없으면 번들이 그 묶음을 주장하지 않아 받는 폰의 맵핑이
남는다. 보관함 세대는 언제나 주장한다(되돌리기가 정확히 되돌리도록).

### 70.6 보관함과 되돌리기

`hl5_save_vault/<14자리 unix ms>.h5save`, 같은 형식, 적용마다 하나, 최신 5개. 이름이 시간이라
목록이 곧 연표이고, 시계가 뒤로 가도 최신+1 로 붙는다. 같은 파일 내용이면 새 세대를 안 만든다.
**되돌리기 = 최신 세대를 예약** — 마지막 불러오기/되돌리기 직전 상태다. 적용할 때 또 현재를
보관하므로 **두 번 누르면 다시 돌아온다**. 게임 자신의 저장은 보관함을 안 건드린다(적용은
플레이어 행동이라 드물다 — 그래서 평평한 창으로 충분하다).

### 70.7 안드로이드

`Hl5Save.java`: `ACTION_CREATE_DOCUMENT`(이름 제안 `영웅서기5_세이브_20261025_1430.h5save`,
`application/octet-stream` — zip 이면 공급자가 `.zip` 을 붙인다) / `ACTION_OPEN_DOCUMENT`
(`*/*`, 내용으로 판정). 권한 없음, 공급자 무관(다운로드·구글 드라이브·USB). 복사는 별도
스레드(클라우드 공급자는 `openInputStream` 안에서 내려받는다), 불러오기는 4 MB+1 까지만.
결과는 `nativeDocResult` → `SAVE_DOC_RESULT` + 루프 깨우기 → `App::save_doc_flush` →
`Vm::save_doc_done`. 요청은 같은 함수가 `Vm::take_save_doc_request` 로 가져가 Java 로
넘기고, Java 에 못 닿으면 즉시 실패로 답한다. 날짜는 `TimeZone` 오프셋(셸이 시작 때 한 번).
파일 화면이 열린 동안 앱은 백그라운드라 게스트가 멈추고(§35.5) 스스로 저장한다(`nativePauseApp`)
— 흐름에는 영향이 없다.

### 70.8 오프라인 증명

프로브 `--save-doc <path>` 가 파일 화면 대역이다(내보내기 → 그 경로로 복사, 불러오기 ← 그
경로). 없으면 모든 화면이 `cancelled`. 프로브도 시작할 때 `apply_pending` 을 돈다.

* `savexfer-ingame.txt`(빈 저장소 A, 새 게임 → 필드 → 시스템 › 환경설정 14행): 내보내기 →
  `1 game save(s) first` → `세이브를 내보냈습니다.` / 불러오기 → 질문 → 같은 파일 →
  `2026-10-25 21:04 캐릭터 1명 / 적용하려면 종료해야 합니다. / 지금 종료하시겠습니까?` →
  아니오 / 되돌리기 → `되돌릴 세이브가 없습니다.` 내보낸 파일은 Python `zipfile` 로 열리고
  `testzip()` 통과, 안의 `kickass` 는 저장소의 것과 바이트 동일.
* 다른 저장소 B(옵션만 저장된 상태)에서 `savexfer-title.txt` → 불러오기 예·예 → `1 staged`,
  종료 요청 / `savexfer-slots.txt` → 시작 줄 `the save it replaces is in …vault…` +
  `applied the save exported "2026-10-25 21:04" — 1 character(s)`, **슬롯 1 에 A 의 워리어**
  / `savexfer-restore.txt` → `<날짜> 상태로` 질문 → 예 / 다시 `savexfer-slots.txt` →
  `applied a vault generation`, 슬롯 비어 있음(`kickass` 2,340 B), 보관함 2세대(불러온 것이
  남아 다시 되돌릴 수 있다).
* 단위시험 +14(번들 왕복·거절 여섯 가지·적용·보관함·롤포워드 한 번·회전·시간 표기·팝업 줄
  폭·EUC-KR).

### 70.9 기기에서 볼 것

1. 인게임에서 내보내기 → 저장 위치 화면 → 다운로드에 저장 → `세이브를 내보냈습니다.`
   (취소하면 아무 팝업도 없어야 한다).
2. 그 파일을 **다른 폰**(같은 빌드 이상)으로 옮겨(카톡·드라이브) 타이틀 › 환경설정 › 불러오기 →
   날짜·캐릭터 수가 맞는가 → 예 → 앱이 닫힌다 → 다시 켜면 그 캐릭터·공용창고·아바타·패드키.
3. 되돌리기 → 불러오기 전 상태로, 한 번 더 → 다시 불러온 상태로.
4. 아무 파일(사진 등)을 골랐을 때 `세이브 파일이 아닙니다.` 이고 아무것도 안 바뀌는가.

## 71. 버프 스킬의 시간 — 설명문의 "초"를 실제 초로 (2026-10-27)

사용자 요구: 버프 스킬의 지속시간·재사용·소모 SP 를 전수 조사하고 조정한다. 조사 결과를 본
사용자 결정 셋: **설명문의 초를 실제 초에 맞춘다(일괄)** / **SP 0·재사용 1초 묶음(71.3 의 13종)은
지속시간만 2배** / **나머지 직업 버프는 SP·재사용·지속시간 모두 2배**(71.8).

`hl5_core/src/skilltime.rs`, `Config::skill_time_hz` + `Config::buff_rebalance`, 셸
`hl5_skill_time.txt`(둘을 함께, 기본 on), `-SkillTime`, 프로브 `--skill-time-hz <n>` /
`--buff-rebalance` + 스크립트 명령 `learn`/`belt`/`skillmap`/`buffs`/`sp`/`skilltext`.

### 71.1 데이터는 어디에 있는가

* **`/c/csv/skill_00..03.dat`**(워리어·로그·건슬링어·나이트, 각 43행) + **`skill_05.dat`**(정령
  16행). 평문, `LoadRes`. `HERO::LoadResSkillInfo`(0x8bba4)가 `HERO+0x348` 부터 0x58 B 짜리
  `HeroSkillInfo` 로 푼다(정령은 43번부터). 레코드 = `[u16 len][u8 nameLen][name][48 B][u8 descLen][desc]`,
  48 B 블록의 쓸 만한 칸:

  | 블록 | `info+` | 뜻 |
  |---|---|---|
  | +0x00 | 0x08 | **최대 레벨** (특성 화면 `KeyPropertyMenu+0xcd8` 이 `ldrsb [info,#8]` 와 현재 레벨을 비교) |
  | +0x04 | 0x0c | SP 공식 번호 → `UseSkill` 이 계산해 `+0x48`, 모자라면 거절, `ProcHeroSkill` 이 `IncreaseSP(-x)` |
  | +0x05 i16 | 0x0e | SP 기본값 (공식 변수 v168, 표시 SP 의 ×10) |
  | +0x0a | 0x14 | 재사용 공식 번호 → `+0x4c` → `SetNowCoolTime` |
  | +0x0b i16 | 0x16 | 재사용 기본값 (v170, 틱) |
  | +0x1a | 0x28 | **모드** — `ProcHeroSkill` 0x9a398 점프표: 0·1 대상에 저주, 2·4 `AddBuffSkill`, 3 `AddStanceSkill` |
  | +0x2b | 0x3a | 버프 종류 |
  | +0x2c | 0x3b | 발동 확률 공식 (`Rand(0,99)` 와 비교) |
  | +0x2d / +0x2e | 0x3c / 0x3d | 효과값 / **지속시간** 공식 번호 |

  레벨(v174)은 `HERO+0x24c+행` — 스킬북(`StateInNpcMenu::Process+0x300`)과 특성 화면이 쓴다.
* **`/c/calc/calc_sk.dat`** = 공식 128개. `Formula::dataLoad`(0x7583c)가 `LoadResDecrypt` 로
  읽는다 — 16 B 머리가 **평문의 MD5**, 나머지가 §55 의 midas DES(키 `0EP@KO91`, 0x1588b0 —
  세이브와 같은 키). 파이썬 이식이 MD5 일치로 확인됐다. 레코드 =
  `[u16 len][i32 min][i32 max][u8 n][토큰…]`, 토큰은 숫자(0)·변수(0x0c, `Formula::getValFunc`
  0x758d0 의 jump table)가 5 B, 연산자(0x11..0x16 = `+ - * / % ^`)가 1 B 인 RPN 이고 결과를
  `[min, max]` 로 자른다. `Formula::calc` 의 번호 0..999 가 이 표다(1000.. 은 `calc_en`,
  2000.. 은 `calc_pl`).
* 공식은 **여러 스킬이 나눠 쓴다**. 지속 "300 + 40×Lv" (43번)를 버프 14종이, 100(59번)과
  20×Lv(66번)는 확률·효과값까지 수십 칸이 쓴다 — 공식을 고치면 엉뚱한 것이 같이 바뀐다.
* 벨트 칸에는 스킬 행이 아니라 **직업별 행동 번호**가 들어간다. `<CLASS>::GetRealSkillIndex`
  가 `HERO+0x2002` 표로 행을 찾는다(워리어: 12→13 전투태세, 14→23 방어태세, 16→31 발악,
  17→34 광폭태세, 18→41 무통증, 19→42 광전사). 프로브 `skillmap` 이 그 표를 찍는다.

### 71.2 시간의 단위는 틱이고, 설명문은 그것을 10으로 나눈다

* 재사용: `HeroSkillInfo::DecreaseNowCoolTime`(0xd8d54, −1)의 호출자는 `ProcHeroSkill+0x58`
  하나이고 `ProcHeroSkill` 은 클래스 `Proc` 마다 한 번이다.
* 버프: `HERO::HeroBuffEndProc`·`BATTLER::BattlerBuffEndProc` → `BuffEndProc`/
  `BuffArrayEndProc` → `BATTLER::DecreaseCount`(0x49f68, −1, 0 하한), 클래스 `EndProc` 마다
  한 번. `ingame.txt` 실측 `ProcHeroSkill` 145 : `HeroBuffEndProc` 144.
* 설명문: `CommonUi::GetSkillDiscription`(0x7a7b4)가 `#07`(SP, `info+0xc`)·`#08`(지속,
  `+0x3d`)·`#09`(재사용, `+0x14`)를 **공식값 ÷ 10** 으로 찍는다(`0x66666667` 곱셈 + 나머지를
  소수 한 자리). 즉 원작은 초당 10틱을 가정하고 썼다.
* 이 셸은 초당 24틱 고정이다(§44). 그래서 **"34초" 버프가 14.2초에 끝났다.** 원작 기본
  속도(설계 27.8틱/s, §31.9)에서는 12.2초였다 — 원작부터 있던 불일치다.

### 71.3 버프는 공격해도 안 끝난다 (사용자 질문)

SP 0 · 재사용 1초인 워리어 6종(발악·무통증·광전사·전투/방어/광폭태세)이 "한 번 공격하면
끝나는 버프"인지 물었다. **아니다.** 끝나는 길은 셋뿐이다:

* 시간이 다 됐을 때 — 위의 `DecreaseCount`.
* 죽었을 때 — 일반 버프 세 칸(`+0x11b..0x11d` 종류)과 태세 칸(`+0x285`)을 쓰는 곳은
  `AddBuffArray`/`AddBuff`/`*EndProc` 말고는 `HERO::ClearBuffAll`(호출자 `SetDieMotion` 하나)뿐.
* 밀려났을 때 — `AddBuffArray`(0x4afd4)는 같은 종류면 **덮어쓰고(중첩 아님, 시간만 갱신)**,
  빈 칸이 없으면 가장 오래된 칸을 민다(칸은 셋). 쐐기탄(79)·철갑탄(80)은 서로를 같은 종류로
  본다. 태세·문장은 한 칸이라 다른 태세가 교체한다.

`HERO::SetStandMotion+0xe8`·`ProcHeroSkill+0x132c` 도 태세 칸을 지우지만 **건슬링어 전용**
(`HERO+0x22c == 2`, 자동사격 태세의 SP 부족)이다. 버프 칸을 줄이는 코드가 공격 경로에 없다.
오프라인 실측(71.6): 발악은 341틱 동안 그대로 갔다.

### 71.4 바꾼 것 — 타이머를 거는 순간에 틱으로 환산

데이터를 2.4배 하면 설명문도 2.4배("82초")가 된다. 그래서 공식값은 그대로 두고(그것이
설명문이 말하는 0.1초 단위다) **타이머를 거는 네 곳**에서 `틱 = 값 × hz ÷ 10` 으로 바꾼다.
네 후크 모두 인자만 고치고 사양한다.

| 자리 | 무엇 | 인자 |
|---|---|---|
| `BATTLER::AddBuffSkill`(0x4b198) | 버프(모드 2), 특성이 붙이는 모드 4 | 5번째 `[sp]` |
| `HERO::AddStanceSkill`(0x91d7c) | 태세·문장(모드 3) | `r3` |
| `HERO::AddSpiritSkill`(0x91d1c) | 정령 버프 9종 | `r3` |
| `HeroSkillInfo::SetNowCoolTime`(0xd8d38) | 재사용 — `ProcHeroSkill+0xd88` 복귀, 행 ≤ 42, 모드 2·3 일 때만 | `r1` |

`AddBuffSkill`·`AddStanceSkill` 의 호출자는 `ProcHeroSkill` 하나, `AddSpiritSkill` 은
`TargetEffect::Proc` 의 아홉 자리(정령 버프 아홉)다. 몬스터에 거는 디버프는
`BATTLER::AddCurseSkill` 이라 안 바뀐다. 남겨 둔 것:

* **공격 스킬의 재사용** — 사용자가 말한 것은 버프다. 후크가 모드와 행으로 거른다.
* **정령 재사용** — 정령 스킬은 게이지 1000 을 통째로 쓰고(`IncreaseCP(-1000)`, 사용 검사가
  `HERO+0x24a > 999`) 설명문에 재사용이 없다. `UseSpiritSkill+0x23c` 가 같은 값을 전역
  `SetSpiritCoolTime` 에도 넣으므로 한쪽만 바꾸면 둘이 갈린다.
* **0 과 1** — 긴급회복의 지속 1 은 `AddBuffSkill` 에서 "즉시·효과 그림 없음"(`cmpne r5,#1 /
  ble`)이다.

버프 아이콘 막대는 `현재 ÷ 최대`(`GMenu::DrawBuildupIcon`, `__divsi3`)이고 두 값이 같은 인자에서
오므로 비율이 맞다. 남은 시간을 숫자로 그리는 곳은 없다. `hz` 는 셸의 틱 상한(기본 24), 상한이
꺼져 있으면 28(게스트 설계율).

### 71.5 결과 (24틱/s, 설명문 숫자 = 실제 초)

| (레벨 1 기준) | 출하 | 지금 |
|---|---|---|
| 워리어 버프·태세 6종, 로그 칼날폭풍·급소공격·독바르기·강공태세·호흡집중·질주, 건슬링어 쐐기탄·철갑탄 | 34초 표시, 실제 14.2초 | 34초 |
| 문장 4종, 연사, 빛의구·영혼의회복 | 30초 표시, 실제 12.5초 | 30초 |
| 무적 | 15 / 6.3초 | 15초 |
| 재사용 1초짜리 10종 | 0.42초 | 1초 |
| 무적 재사용 | 80 / 33.3초 | 80초 |

이 표는 71.4 만의 결과다. 71.8 의 2배가 그 위에 얹힌다.

### 71.6 오프라인 증명

`tools_re/scripts/bufftime.txt`(빈 저장소, 새 워리어 → 필드 → `learn`·`belt` 로 발악·전투태세를
'1'·'3' 에 → 시전 → 900틱 관찰 → 돌진).

| `buffs` 줄 | 출하 | `--skill-time-hz 24` |
|---|---|---|
| 발악 칸 (게스트가 +1) | 332/341 | 808/817 |
| 전투태세 칸 | 332/341 | 808/817 |
| 발악 재사용 | 2/10 | 16/24 |
| 900틱 뒤 발악 | 사라짐 | 337/817 |
| 돌진 재사용 (공격 스킬) | 78/80 | **78/80** |

단위시험 +4(`skilltime`: 표의 값이 설명문 초에 떨어지는가, 0·1, i16 한계, 0 Hz).

### 71.7 기기에서 볼 것

1. 버프 하나(워리어면 발악)를 쓰고 스톱워치로 잰다 — 설명문의 초(68초)와 맞아야 한다.
2. 도발: 설명문이 `SP 60 · 재사용 48초 · 무적 4초`(Lv1)이고 실제로 그만큼인가.
3. 공격 스킬(돌진 등)의 재사용은 전처럼 빠른가 — 바뀌면 회귀다.
4. 정령 버프(날개 등)의 아이콘 막대가 끝까지 줄어드는가.

### 71.8 2배 조정 — `skilltime::REBALANCE`

사용자 결정(2026-10-27): 71.3 의 표 13종(워리어 6 SP 0·1초, 로그 4 SP 10·1초, 건슬링어 3 SP 0·
재사용 없음)은 **지속시간만 2배**, 그 밖의 직업 버프 10종(도발·긴급회피·호흡집중·질주·독무·
문장 넷·무적)은 **SP·재사용·지속시간 2배**, 긴급회복은 지속이 "즉시"라 **SP·재사용만 2배**.
같은 날 예외 셋을 더했다: **긴급회피·긴급회복은 재사용을 2배 하지 않고**(실제 초 환산만 —
50초), **무적은 재사용 100초**. 정령 버프는 게이지를 통째로 쓰므로 안 바꾼다.
`REBALANCE` 는 행마다 출하값 → 새 값(SP·재사용 기본값)과 지속 2배 여부를 적는다.

* **SP·재사용 = 데이터.** `skill_00/01/03.dat` 의 기본값(블록 +5 / +0xB, v168 / v170)을 2배로
  한 그림자 파일(`rates::write_override`). 표가 출하값(버프 종류·모드·SP·재사용)을 대조하고
  하나라도 다르면 그 파일을 안 쓴다. 꺼지면 지운다. `UseSkill`·설명문·스킬북 설명이 전부 그
  값에서 계산하고, 특성의 감소(모독 Lv당 −15%, 회피충전 Lv당 −10%)도 그대로 얹힌다.
  (긴급회피 재사용을 2배 했을 때는 공식 53 의 상한 999 에 걸려 99.9초였다 — 예외로 2배를
  빼면서 사라졌다.)
* **지속 = 후크.** 지속 공식은 여럿이 나눠 쓰고(43번을 14종, 59·66번은 수십 칸) `calc_sk.dat`
  는 암호화돼 있어 데이터로 못 고친다. 그래서 71.4 의 `AddBuffSkill`/`AddStanceSkill` 후크가
  환산 전에 ×2 하고, 설명문 `#08` 을 계산하는 두 곳 — `GetSkillDiscription+0x598`(`r6` = info)
  과 `DrawSkillBookDiscription+0x830`(`r8`) — 에서 `bl Formula::calc` 직후 `r0` 을 ×2 한다.
  스킬은 **버프 종류**(`info+0x3a`)로 가린다 — 표의 24종 모두 다섯 파일 전체에서 자기 종류를
  혼자 쓴다(전수 확인, 단위시험이 표 안에서 다시 확인). 몬스터 디버프·특성의 모드 4 버프
  (암습 75)는 표에 없어 ×1.
* 공식 `#08` 을 계산하는 곳 전수(`Formula::calc` 호출 중 `info+0x3d` 를 읽는 곳): 위의 둘,
  `ProcHeroSkill` 의 셋(태세·버프·저주 → 후크/제외), `TargetEffect::Proc` 아홉(정령),
  `ProcTargetEffectSkill`(디버프). `+0xc`·`+0x14` 는 `UseSkill`·두 설명문·`Battle::DrawSpiritCutIn`
  (정령)뿐이다.

오프라인(`bufftime.txt`, 틱, 24틱 = 1초):

| | 출하 | 실제 초만 | 실제 초 + 2배 |
|---|---|---|---|
| 돌진 재사용 (대조군) | 80 | 80 | 80 |
| 발악 지속 | 341 | 817 | **1633** (68초) |
| 발악 재사용 | 10 | 24 | 24 |
| 전투태세 지속 | 341 | 817 | **1633** |
| 도발 소모 SP | 300 | 300 | **600** |
| 도발 재사용 | 240 | 576 | **1152** (48초) |
| 도발 무적 | 21 | 49 | **97** (4초) |

설명문은 프로브 `skilltext` 로 게스트 자신의 `GetSkillDiscription` 이 만든 문장을 받았다: 발악
`34초 → 68초`, 도발 `SP 30·24초·2초 → SP 60·48초·4초`, 그리고 `class` 로 직업을 바꿔 무적
`SP 30·80초·15초 → SP 60·100초·30초`, 긴급회복 `SP 30·50초 → SP 60·50초`, 증오의문장
`→ SP 60·50초·60초`, 긴급회피 `SP 10·50초·6초 → SP 20·50초·12초`, 호흡집중 `→ SP 40·60초·68초`.
그림자 셋을 출하본과 전수 대조하면 바뀐 바이트는 표의 11행의 SP·재사용 칸뿐이다. 단위시험 +5(표의 종류 고유성, 종류별
배율, 예외 셋, 그림자 거절, 레코드 걷기).

인수 A/B: `PROBE_FLAGS="--skill-time-hz 24 --buff-rebalance" bash tools/ab_905.sh rb` — patched vs
`--no-patch` 5장면 **1,241 프레임 전부 동일**(그림자 셋이 쓰이고 후크 여섯이 걸린 상태), 그리고
두 기능을 끈 쪽(`skt0`)과 켠 쪽 대조도 **1,241 전부 동일** — 다섯 장면 어디에서도 버프를 쓰거나
그 스킬의 설명문을 열지 않으므로 그림이 달라질 곳이 없다. 실제 초 환산만 켠 A/B(`skt`)도 같다.

### 71.9 나머지 설명문 — 동작은 그대로, 문구를 실제 초로

사용자 질문 "설명문 업데이트 안 된 것"을 전수 조사했다.

* **버프 쪽은 남은 것이 없었다.** 스킬 설명문을 만드는 곳은 `GetSkillDiscription`(호출자
  `DrawSkillDiscription` ← `DrawAutoSkillBox` ← 스킬·특성·정령·연계 메뉴, 그리고
  `Event_PlayerGetRepay`)과 그 복사본 `DrawSkillBookDiscription`(가방 아이템 정보, NPC 스킬 상점)
  둘뿐이고, 벨트는 쿨타임을 `Graphic::DrawCoolTime` 의 원으로, 버프 아이콘은 막대로만 그린다
  (숫자 없음). 스킬북 아이템 설명(`item_16/17`)·도움말·팁에는 "짧은 시간 동안"처럼 숫자가 없다.
* **버프가 아닌 것은 원작부터 거짓이었다** — 공격 재사용 37종, 디버프 5종(고통·충격포·방어분쇄·
  무장해제·섬광탄), 기절·경직·블록 6종(`#12`/`#13`)이 틱을 ÷10 해서 찍으므로 24틱/s 에서 실제는
  표시의 42%. 사용자 결정: **동작은 그대로 두고 설명문을 실제 초로.**
* 두 포매터의 `#08`·`#09`·`#12`·`#13` 자리마다(`bl Formula::calc` 직후, `TEXT_SITES` 여덟) 후크.
  설명문 커서(`r4` / `r5`, 콜리-세이브)로 자리표시자 뒤가 `초`(`|` 하나 허용)인지 보고, 71.4 가
  실행 중에 실제 초로 바꾸지 않는 값이면 표시값을 `값 × 10 / hz` 로 바꾼다 — `#13` 이 SP 양
  ("SP회복 #13 증가")일 때는 `초` 가 없어 안 건드린다.
* 자리표시자가 못 닿는 고정 문구는 그림자에서 바꾼다(`TEXT_EDITS`, 레코드·설명문 길이 다시 씀):
  포대 "15초 유지"(효과 수명 150틱, `GUNNER::NewSentryBodyEffect` 의 `mov r4,#0x96`) → "6.3초",
  그리고 **원작 오기** 그림자밟기 "축지 재사용 #13초 단축" — 그 값(5·Lv)은 축지 재사용 공식 52 에서
  **퍼센트**다 — 를 같은 공식의 정수 `#10%` 로.
* **독바르기 "중독된 적은 10초간"은 동작을 바꿨다**(사용자 결정): `Monster::AddEffectPoison`
  (호출자 `ApplyAddEffect` 하나)이 `AddBadSkill` 에 넘기는 고정 200틱을 `+0x40` 후크가
  `to_ticks(100)` = 240 으로. 몬스터에게 거는 중독은 이 한 곳뿐이다.
* **안 고친 둘**: 지옥소환 "30초"(화염 지역 수명을 코드에서 못 찾았다)와 유도탄 "0.8초간 경직"
  (`Monster::AddEffectKnockBack(16)` → `SetDecreaseDamagedFrame` 인데 그 카운터의 단위가 초가
  아니다). 원작 문구 그대로다.

게스트 자신의 문장(`skilltext`, 출하 → 지금): 돌진 `8초 → 3.3초`, 내려찍기 `16 → 6.7`, 고통
`4초·22초 → 1.7초·9.2초`, 비겁한공격 `0.5 → 0.2`, 충격포 `10·10 → 4.2·4.2`, 포대 `20·15 →
8.3·6.3`, 경직 `0.4 → 0.2`, 섬광탄 `30 → 12.5`, 액티브블록 `20·1.6 → 8.3·0.7`, 방어분쇄
`20·22 → 8.3·9.2`, 폭발 `3 → 1.3`, 그림자밟기 `0.5초 → 5%`; 버프(발악 `1초`, 날개 `20초`)와
유도탄 `0.8초` 는 그대로. 단위시험 +3(표시값 규칙, 고정 문구·중독 값, 설명문 다시 쓰기).

인수 A/B(`PROBE_FLAGS="--skill-time-hz 24 --buff-rebalance" bash tools/ab_905.sh tx`): patched vs
`--no-patch` **1,241 전부 동일**, 끈 쪽(`skt0`)과 켠 쪽도 **1,241 전부 동일** — 다섯 장면은 어느
스킬 설명문도 열지 않는다.
