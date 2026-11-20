# Teljesítmény-elemzés: hol lehet gyorsítani?

Állapot: 1.1.0 (2026-11-19). A dokumentum azt nézi végig, hogy a **lefordított
programok** hol lassúak, és mit lehetne tenni ellenük. A fordító saját sebessége
nem gond: a `shooter.lz80n` kb. 0,1 s alatt fordul le, mind a 24 példa kb. 1,7 s
alatt (ennek nagy része az SjASMPlus indítása).

## 1. Módszer

* **Mérés:** `benchmarks/micro.lz80n` 25 tipikus műveletet mér a beépített
  emulátorban. A `bench_start()` és `bench_stop(n)` hívások között eltelt
  T-állapotokat a `bench` teszt írja ki (a `compiler/` mappából):

  ```
  BENCH=../benchmarks/micro.lz80n cargo test --release bench -- --ignored --nocapture
  ```

  Ezek ideális Z80N T-állapotok, várakozási ciklusok nélkül. A számok a frame
  megszakítást is tartalmazzák. **Egy képkocka 28 MHz-en kb. 560 000 T,
  3,5 MHz-en kb. 70 000 T.**
* **Kódátnézés:** a generált assembly (`*.asm`), a `codegen.rs`, a
  `peephole.rs`, a `runtime.rs` és a `prelude.rs` (az LZ80N-ben írt könyvtár)
  átnézése.

## 2. Mért eredmények

| # | Művelet | T-állapot | Megjegyzés |
|---|---------|-----------|------------|
| 1 | üres `for i = 1, 1000` (word) | 59 / iteráció | a ciklusváltozó a memóriában |
| 2 | `b = addb(b, 3)` (2 byte paraméter) | ~120 / hívás | paraméterek statikus memórián át |
| 3 | `grid[y][x] = x + y` (16×16) | 254 / elem | `push`/`pop` az indexelésben |
| 4 | `ps[i].x += ps[i].dx` (+ y, + egy `if`) | 775 / objektum | az elem címe 5-ször számolódik |
| 5 | `w = w / 3 + 40000` | ~290 | konstanssal osztás már szorzással megy |
| 6 | `w = w * 3` | ~60 | jó |
| 7 | `fixed` szorzás | ~340 | előjelkezelés + 4 `MUL` |
| 8 | `long += 12345` | ~213 | `rt_add32` hívás |
| 9 | `at(0,0) write(w)` (5 jegy) | **6 770** | 5 × `rt_divu16` |
| 10 | `plot` (indexszámítással) | ~350 / pixel | 3 szintű hívás |
| 11 | `line` (10 × 201 pixel) | **~860 / pixel** | LZ80N-ben írt Bresenham |
| 12 | `vline` (10 × 190 pixel) | **~415 / pixel** | pixelenként `plot` |
| 13 | `fill_rect` 100 × 100 | 27 / pixel | soronként `LDIR` |
| 14 | `circle` r = 60 | **812 000** (1,45 képkocka) | soronként `sqrt` + osztás |
| 15 | `fill_circle` r = 60 | 637 000 | ugyanaz |
| 16 | `fill_triangle` (~150 sor) | **1 132 000** (2 képkocka) | soronként 2 osztás |
| 17 | `sprite[i].position(x, y)` | **~1 360 / sprite** | `self` mezőnként újratöltve |
| 18 | `layer2_clear` | 200 000 | DMA, jó |
| 19 | `s = s .. "x"` | ~1 550 | az egész string másolódik |
| 20 | hangmeghajtó, 3 dallam szól | **~15 600 / képkocka** | `sound_tick` LZ80N-ben |
| 21 | `sqrt(word)` | **~2 010** | 24 bites gyök 16 bitre is |
| 22 | `key(KEY_SPACE)` | ~53 | jó |
| 23 | `hline` teljes szélesség | 23 / pixel | `LDIR` |
| 24 | `while w < 1000 do w += 1 end` | ~103 / iteráció | |
| 25 | `sin` + `cos` | ~262 | jó (táblázat) |

**Mit jelent ez egy játékban?** 64 sprite mozgatása (#17) 87 000 T, ez egy
28 MHz-es képkocka 16%-a. A hangmeghajtó 28 MHz-en a képkocka 2,8%-át viszi el,
3,5 MHz-en (`cpu_speed(MHZ_3)`) már 22%-át. Egy 60-as sugarú kör (#14) másfél
képkockáig tart.

## 3. Javaslatok, várható nyereség szerint

A becslések nagyságrendiek. Minden változtatást a meglévő tesztek és a
`micro.lz80n` mérései igazolnának.

### A. A könyvtár forró részei assemblyben (nagy nyereség, kis kockázat)

A `prelude.rs` grafikai és hangkönyvtára LZ80N-ben van írva. Ez olvasható, de
pixelenként vagy csatornánként sok általános célú kód fut. Ezeket érdemes kézzel
írt runtime rutinokra cserélni, változatlan LZ80N-felülettel:

| Rutin | Most | Javaslat | Várható |
|-------|------|----------|---------|
| `line` | ~860 T/px | asm Bresenham. 256-os módban a cím közvetlenül léptethető (x: `inc l`, y: `inc h`, laphatárnál újra kell lapozni); a széles módokban oszlopos a tárolás. | **10–15×** |
| `vline` | ~415 T/px | 256-os módban `inc h` soronként, laponként 32 sor. **A 320/640-es módokban egy oszlop folytonos memória: `LDIR` vagy DMA fill.** | **10×**, széles módokban 20×+ |
| `circle` / `ellipse` | 812 000 T | középpontos (midpoint) algoritmus, `sqrt` és osztás nélkül, 8-as (ellipszisnél 4-es) szimmetriával | **10–20×** |
| `fill_circle` / `fill_ellipse` | 637 000 T | midpoint algoritmus spanokkal; a sorvégek összeadással léptethetők | **2–3×** (a kitöltés `LDIR`-je marad) |
| `fill_triangle` | 1 132 000 T | DDA: a két él x-e 8.8 vagy 16.16 fixpontos lépéssel halad, soronként osztás nélkül | **3–4×** |
| `Sprite.update` | ~1 360 T | asm rutin IX = `self`-fel, mezőnként `ld a,(ix+n)`. A pozícióhoz elég a 0/1/2. attribútum. Még jobb: port `$57` és `$303B`, 5 × `out` (a select után a NR `$34`–`$39` írása helyett). | **5–8×** |
| `sound_tick` | ~15 600 T/kocka | asm meghajtó, csatornánként közvetlen állapotrekordokkal. Ha nincs aktív csatorna, azonnali `ret` (egy jelzőbájt alapján). | **5×**; 3,5 MHz-en ez a legfontosabb |

**Sorrend:** `Sprite.update` és `sound_tick` (minden játékot érint, minden
képkockában), aztán `line`/`vline`, végül a körök és háromszögek.

### B. Runtime rutinok (közepes nyereség, kis kockázat)

1. **Számkiírás (`rt_print_u16`)** – most 5 × `rt_divu16` (kb. 600 T jegyenként).
   10-hatványokkal való ismételt kivonás (10000, 1000, 100, 10) jegyenként
   átlagosan kb. 5 × 26 T, plusz a vezető nullák kezelése. A `write(score)`
   minden képkockában gyakori: **~4–5× gyorsabb**. Ugyanez kell az
   `rt_print_s16`-nak és (32 bitre) az `rt_print_u32`-nek.
2. **`sqrt(word)`** – most a 24 bites `rt_isqrt24` fut 16 bites argumentumra is
   (~2 000 T). Egy külön 16 bites bitpár-módszer (8 lépés, kigöngyölítve) kb.
   **400–500 T**. A `fill_circle` is használja, amíg az A pont el nem készül.
3. **`hline` / `fill_rect` hosszú szakaszai DMA-val** – `LDIR` 21 T/bájt. A
   zxnDMA fill 28 MHz-en ennek töredéke. Egy DMA-program beállítása kb.
   150–200 T, ezért csak kb. 32 pixelnél hosszabb szakaszokra érdemes (rövidebbnél
   marad az `LDIR`). A 100 × 100-as `fill_rect` (#13): **~3×**. Ha minta szól
   (`rt_smp_on`), a mostani CPU-s út marad.
4. **`hline` 640-es módban** – most pixelenként rajzol (`rt_hline_hi`). Mivel a
   tárolás oszlopos, egy vízszintes vonal soronként egy bájt minden oszlopban:
   egy `add hl,256`-os ciklus egyszerre két pixelt ír (nibble pár), csak a két
   szélén kell maszkolni. **~4×**.
5. **`fixed` szorzás** – most mindkét operandust abszolút értékre hozza, majd
   negál. Előjeles 8.8 × 8.8 előjelkorrekcióval (`MUL` után a negatív operandus
   kivonása a felső bájtból) a három `rt_neghl` hívás elhagyható. **~30%**.
6. **`long` műveletek konstanssal** – `big += 12345` most `rt_add32` hívás
   (~213 T). Inline `ld hl,(x) / ld de,lo / add hl,de / ld (x),hl / ld hl,(x+2) /
   ld de,hi / adc hl,de / ld (x+2),hl` kb. 110 T. **~2×**. Ugyanígy a `-=`, és
   a 0-val vagy konstanssal való összehasonlítás.
7. **String hozzáfűzés helyben** – `s = s .. t` esetén, ha a cél a bal operandus,
   elég `t`-t a végére másolni és a hosszt növelni (most az egész string
   ideiglenes pufferbe, majd vissza másolódik). Hosszabb stringeknél **~3–10×**.

### C. Kódgenerálás (a legnagyobb általános nyereség, közepes munka)

Ezek minden programot gyorsítanak, nem csak a könyvtárat.

1. **`a op= b` a cím egyszeri kiszámításával.** A parser most `a = a op b`-re
   bontja, ezért `ps[i].x += d` az elem címét kétszer számolja (és ha `d` is
   `ps[i]` mezője, háromszor). Javaslat: összetett célnál a cím egyszer
   `HL`-be kerül (`push hl`), betöltés, művelet, `pop hl`, tárolás. A #4 mérés
   alapján az ilyen sorok **~2×** gyorsabbak lesznek. Ez a leggyakoribb minta
   játékokban (`obj.x += obj.dx`).
2. **`or` / `not` rövidzáras feltételként.** A `branch_false` csak az `and`-et
   kezeli ugrásokkal. `if k == 100 or k > 150` most `0/1` értéket állít elő,
   majd `or a` / `jp z`. Kell egy `branch_true` párja is; ezzel az `or` és a
   `not` közvetlen ugrás lesz. **Feltételenként 15–30 T, és rövidebb kód.**
3. **Összehasonlítás konstanssal.**
   * `x > c` → `x >= c+1`: egy feltételes ugrás kettő helyett (`jp c` + `jp z`).
     Ugyanez a `<=`-re.
   * Előjeles `x < 0` / `x >= 0` → `bit 7,h` (most `xor $80`, `ld de,32768`,
     `sbc hl,de`).
   * `x == 0` szónál → `ld a,h / or l`.
4. **Szűkítő betöltések.** `byte(self.x)` most az egész szót betölti
   (`ld a,(hl) / inc hl / ld h,(hl) / ld l,a / ld a,l`), elég az alsó bájt.
   `byte(x >> 8)` most `bsrl de,b`, elég a felső bájt (`inc hl / ld a,(hl)`).
   Ez a `Sprite.update`-ben is előfordul.
5. **Tömbindexelés `ADD HL,A`-val.** `grid[y][x]` most:
   `ld l,a / ld h,0 / push hl / ... / pop de / add hl,de`. A Z80N
   `add hl,a` utasítással a byte-index közvetlenül hozzáadható, `push`/`pop`
   nélkül. Nem kettő-hatvány sorhossznál a `mul d,e` már használatban van.
   **2D tömbelérés ~40%-kal gyorsabb.**
6. **`self` IX-ben a metódusokban.** Most minden mezőelérés
   `ld hl,(p_..._self) / add hl,n / ld a,(hl)` (39 T), IX-szel `ld a,(ix+n)`
   (19 T). A metódus belépéskor `push ix / ld ix,(self)`, kilépéskor `pop ix`.
   Ha a metódus nem hív mást, a mentés el is hagyható. **Metódusokban a
   mezőelérés ~2×.** Ugyanez a minta használható egy ciklus testében, ha
   ugyanazt a `ps[i]` elemet sokszor eléri (a C.1 általánosítása).
7. **Ismert regisztertartalom követése** (peephole, alapblokkon belül).
   Gyakori minta: `ld a,(x)` kétszer egymás után, vagy `ld (x),a` után `ld a,(x)`
   egy másik ág után. Egy egyszerű követés („A = (x), HL = (y)", amit címke,
   hívás vagy memóriaírás érvénytelenít) a felesleges újratöltések nagy részét
   eltünteti. **5–15% általános nyereség.**
8. **Peephole-szabályok (olcsó, biztonságos):**
   * `jp cc,L1 / jp L2 / L1:` → `jp !cc,L2` (feltétel megfordítása),
   * ugrás ugrásra → közvetlenül a végső célra,
   * `ld hl,(x) / ex de,hl / ld hl,(x) / add hl,de` (`err + err`) → `ld hl,(x) / add hl,hl`,
   * `ld a,(x) / ld l,a / ld h,0 / push hl / … / pop de / add hl,de` → `add hl,a` (lásd C.5),
   * konstans végű `for` ciklusnál a soha nem olvasott `$end` slot tárolása elhagyható.
9. **Byte `for` ciklus `DJNZ`-vel.** Ha a ciklusváltozót a test nem olvassa,
   és a ciklus 1-től n-ig megy (`for i = 1, 8 do ... end`), a számláló B-ben
   lehet (`djnz`, 13 T). Most egy memóriaváltozót tölt, növel, tárol és
   összehasonlít (~40 T). Ehhez a testnek B-t meg kell őriznie, vagy B-t
   `push bc`/`pop bc` védi a test körül (akkor is olcsóbb). **Szűk ciklusokban
   ~2×.**
10. **Ciklusváltozó regiszterben** (nagyobb munka): a legbelső, hívásmentes
    ciklusoknál a változó a ciklus alatt regiszterben maradhatna, és csak
    kilépéskor íródna vissza. Ehhez kell egy egyszerű regiszter-hozzárendelés
    a kifejezés-kiértékelésben. Ez a legnagyobb, de a legkockázatosabb
    változtatás. Csak a C.1–C.9 után érdemes.
11. **Függvényhívás paraméterei regiszterben.** Egyparaméteres, nem rekurzív
    függvénynél az első paraméter A-ban vagy HL-ben érkezhetne. A hívó nem
    írná a memóriába, a hívott csak akkor, ha a paramétert a hívása után még
    használja. `addb`-szerű kis függvényeknél **~30%**.

### D. Megszakítás

* Az ISR minden képkockában meghívja a `sound_tick`-et, ha a hangmodul
  használatban van, akkor is, ha egy csatorna sem szól. Egy `snd_active`
  jelzőbájt alapján a hívás elhagyható (lásd A.).
* `uses_long` / `uses_recursion` esetén az ISR a teljes árnyék-regiszterkészletet
  menti (kb. 80 T). Ez csak akkor kell, ha maga a kezelő (vagy amit hív)
  használ árnyékregisztert; ezt a fordító a hívási gráfból el tudja dönteni.

## 4. Javasolt sorrend

1. **Peephole-szabályok (C.8) és `or`/`not` ugrásként (C.2), konstans
   összehasonlítások (C.3)** – kis munka, minden programra hat.
2. **`a op= b` címe egyszer (C.1) és `self` IX-ben (C.6)** – a játékok tipikus
   kódját közvetlenül gyorsítja.
3. **`Sprite.update` és `sound_tick` assemblyben (A)** – minden képkockában fut.
4. **Számkiírás (B.1), `sqrt` (B.2), `long` inline (B.6).**
5. **`line`, `vline`, kör/ellipszis, háromszög assemblyben (A).**
6. **DMA-s `hline`/`fill_rect`, 640-es `hline` (B.3, B.4).**
7. **Regiszter-hozzárendelés (C.9–C.11)**, ha az előzők után még kell.

Minden lépés után: a teljes tesztkészlet, a `micro.lz80n` újramérése, és a
`benchmarks/README.md` táblázatának frissítése.

## 6. Eredmények (elkészült)

`micro.lz80n`, T-állapot, a tesztemulátorban (a 2026-11-19-i állapothoz képest):

| # | Mérés | előtte | most | gyorsulás |
|---|-------|-------:|-----:|----------:|
| 3 | 2D tömb feltöltése 16×16 | 65 172 | 48 020 | 1,36× |
| 4 | 16 objektum mozgatása (`ps[i].x += ps[i].dx`) | 12 404 | 7 380 | 1,68× |
| 8 | `long += konstans` ×1000 | 272 187 | 185 183 | 1,47× |
| 9 | `write(word)` ×100 | 677 291 | 249 647 | 2,71× |
| 11 | 10 vonal, ~190 pixel | 1 720 256 | 1 373 744 | 1,25× |
| 12 | `vline` 10 × 190 | 788 552 | 101 574 | 7,76× |
| 13 | `fill_rect` 100 × 100 | 272 033 | 146 333 | 1,86× |
| 14 | `circle` r = 60 | 812 431 | 417 286 | 1,95× |
| 15 | `fill_circle` r = 60 | 637 495 | 387 678 | 1,64× |
| 16 | `fill_triangle` ~150 sor | 1 132 047 | 692 820 | 1,63× |
| 17 | 64 sprite pozíciója | 87 252 | 44 884 | 1,94× |
| 19 | string építése, 50 karakter | 77 281 | 12 087 | 6,39× |
| 21 | `sqrt(word)` ×1000 | 2 013 199 | 873 643 | 2,30× |
| 23 | `hline` teljes szélesség ×192 | 1 131 054 | 380 358 | 2,97× |
| 24 | `while w < 1000` | 103 102 | 93 112 | 1,11× |
| 27 | `hline` 320×256, ×256 | 5 167 161 | 2 933 328 | 1,76× |
| 28 | `hline` 640×256, ×256 | 87 268 480 | 2 978 640 | 29,3× |

A hangmeghajtó (`sound_tick`) három szóló dallammal képkockánként kb.
13 900 T helyett 9 600 T, és ha semmi sem szól, azonnal visszatér.

Ami elkészült:

* **A:** `Sprite.update`, a hangmeghajtó, a `line`, `vline` és a vágott
  vízszintes szakasz assemblyben. A `fill_triangle` soronként osztás nélkül
  lépteti az éleket (ez egy `word`-túlcsordulást is javít a nagy 320/640-es
  háromszögeknél). Az `ellipse` újrahasználja a következő sor félszélességét.
  Mindegyiket pixelpontos teszt veti össze az 1.1.0-s rutinnal.
* **B.1–B.4, B.6, B.7:** számkiírás 10-hatványok kivonásával; 16 bites
  `sqrt`; DMA-s `hline` 32 pixeltől (256-os mód; minta közben CPU); 320-as
  és 640-es `hline` lapomként egy szűk ciklussal; `long`/`fixed16` ± konstans
  inline; `s = s .. x` helyben.
* **C.1–C.5, C.8:** lásd a 2026-11-19-i kört (rövidzáras feltételek,
  konstans-összehasonlítások, `op=` helyben, `add hl,a` indexelés,
  ugrás-peephole-ok). `plot(x, y + k, c)` verem nélkül.
* **D:** az ISR csak felhasználói kezelő esetén menti az árnyékregisztereket.

Ami kimaradt, és miért:

* **B.5** (előjeles `fixed` szorzás): a javasolt előjelkorrekció lefelé
  kerekít, a mostani rutin nulla felé csonkol; negatív szorzatoknál más
  eredményt adna.
* **C.7** (regisztertartalom-követés): a 24 példa generált kódjában
  51 582 utasításból csak 93 felesleges `ld a,(x)` maradt, a nyereség 0,2%.
* **C.6** (`self` IX-ben), **C.9–C.11**: a példákban kevés a metóduson
  belüli mezőelérés (~200), a `djnz`-s ciklus kb. 10 T-t nyerne, de a
  `break` miatt kockázatos. Akkor érdemes, ha egy valódi játék mérése ezt
  mutatja.

## 5. Ami már jó

* Konstanssal való osztás és maradék szorzással (`udiv_magic`), kettő-hatvány
  szorzás és osztás eltolással.
* `mul d,e` (Z80N) a 16 bites szorzásban és az indexelésben.
* `layer2_clear` és `dma_copy`/`dma_fill` zxnDMA-val; a mintalejátszás alatt
  automatikus CPU-s út.
* A szöveg kiírása (`rt_pp_fast`) és a `key()` gyors.
* A `sin`/`cos` táblázatos.
* A PT3 zene előre kiszámolt regiszterképekből szól, ezért szinte nem kerül
  CPU-időbe.
