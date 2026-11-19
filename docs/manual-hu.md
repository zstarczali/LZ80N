# LZ80N programozói kézikönyv

> A Lua egyszerűsége. A Z80N ereje.

Az LZ80N statikusan típusos, Lua ihlette programozási nyelv a **ZX Spectrum
Next** számára. Az `lz80nc` fordító az LZ80N forráskódot natív Z80N gépi kódra
fordítja, és `.nex` fájlba csomagolja, amely közvetlenül fut a Nexten vagy
emulátorban. Nincs interpreter, nincs virtuális gép és nincs szemétgyűjtő: amit
leírsz, az Z80N utasításokká válik.

Ez a kézikönyv a nyelvet és a könyvtárait az 1.0.0-s verzió állapota szerint
írja le (lásd [`CHANGELOG.md`](../CHANGELOG.md)). Az angol változat: [`manual-en.md`](manual-en.md). A tömör műszaki
specifikáció: [`spec.md`](spec.md).

---

## Tartalom

1. [Első lépések](#1-első-lépések)
2. [Az első program](#2-az-első-program)
3. [A forráskód alapjai](#3-a-forráskód-alapjai)
4. [Típusok, változók és konstansok](#4-típusok-változók-és-konstansok)
5. [Kifejezések és operátorok](#5-kifejezések-és-operátorok)
6. [Vezérlési szerkezetek](#6-vezérlési-szerkezetek)
7. [Függvények](#7-függvények)
8. [Struktúrák és osztályok](#8-struktúrák-és-osztályok)
9. [Stringek](#9-stringek)
10. [Fixpontos számok és matematika](#10-fixpontos-számok-és-matematika)
11. [Szövegkiírás](#11-szövegkiírás)
12. [Bevitel: billentyűzet és joystick](#12-bevitel-billentyűzet-és-joystick)
13. [Idő, véletlenszámok, processzorsebesség](#13-idő-véletlenszámok-processzorsebesség)
14. [Layer 2 grafika](#14-layer-2-grafika)
15. [Paletták](#15-paletták)
16. [Hardveres sprite-ok és erőforrások](#16-hardveres-sprite-ok-és-erőforrások)
17. [Hang és zene](#17-hang-és-zene)
18. [Megszakítások és a copper](#18-megszakítások-és-a-copper)
19. [Memória, portok, Next-regiszterek és DMA](#19-memória-portok-next-regiszterek-és-dma)
20. [Beépített assembly](#20-beépített-assembly)
21. [Gyors kód írása](#21-gyors-kód-írása)
22. [Hogyan fut egy program](#22-hogyan-fut-egy-program)
23. [Korlátok és tervek](#23-korlátok-és-tervek)
24. [Referenciatáblák](#24-referenciatáblák)

---

## 1. Első lépések

### 1.1 Amire szükséged van

* Az **lz80nc** fordító és az **SjASMPlus 1.24.0** assembler, amely a
  generált kódból `.nex` fájlt készít. A Windows, macOS és Linux release
  csomagok mindkettőt tartalmazzák; a fordító a saját mappájában megtalálja
  az assemblert. (Forrásból fordítás: lásd `README.md`.)
* Egy **ZX Spectrum Next** (3.0-s vagy újabb core) vagy a **CSpect** emulátor.

Az LZ80N kizárólag a Nextet célozza. Nincs 48K-s vagy 128K-s kompatibilitási
mód. A program 28 MHz-re kapcsolja a processzort, és közvetlenül használja a
Next hardverét (tilemap, Layer 2, sprite-ok, zxnDMA, copper).

### 1.2 Egy program fordítása

```powershell
lz80nc examples/hello.lz80n            # hello.asm és hello.nex készül
lz80nc game.lz80n -o build/game.nex    # kimeneti fájl megadása
lz80nc game.lz80n -S                   # megáll az assemblynél (.asm)
lz80nc game.lz80n -O0                  # peephole optimalizálás nélkül
lz80nc game.lz80n --bank-all           # minden függvény külön kódlapon (22.5)
lz80nc game.lz80n --sjasmplus C:\tools\sjasmplus.exe
```

A hibák formája `fájl:sor:oszlop: error: üzenet`. A fordító egy futásban annyi
hibát jelez, amennyit csak talál.

### 1.3 Futtatás

* **CSpect**: `CSpect.exe -zxnext -w3 hello.nex`
* **Next**: másold a `.nex` fájlt az SD-kártyára, és nyisd meg a Browserben.

### 1.4 A projekt felépítése

```
compiler/      a Rust fordító (lexer, parser, kódgenerátor, runtime, tesztek)
docs/          ez a kézikönyv, a specifikáció, a koncepciódokumentum
examples/      futtatható példaprogramok (.lz80n) és erőforrásaik
benchmarks/    ugyanazok a feladatok LZ80N-ben és NextBASIC-ben
tools/         SjASMPlus
```

---

## 2. Az első program

```lua
-- hello.lz80n
function square(byte x) -> word
    return x * x
end

ink(BRIGHT_YELLOW)
print("Hello, Next!")
ink(WHITE)
for i = 1, 10 do
    print(i, square(i))
end
```

Néhány megfigyelés:

* Nincs `main` függvény. A legfelső szinten álló utasítások, a forrás
  sorrendjében, alkotják a főprogramot. Függvényt bárhol lehet deklarálni a
  fájlban, a használata előtt vagy után is.
* Minden változónak van típusa (`byte`, `word` …). Az `i` ciklusváltozó a
  határokból kapja a típusát.
* Az `x * x` `word`-ként számolódik, mert a függvény `word`-öt ad vissza. Az
  eredmény nem csonkul 8 bitre.
* A `print` a Next szövegrétegére ír (40 × 32 karakter).

Amikor a főprogram véget ér, a gép várakozik, és a képernyő változatlan marad.

---

## 3. A forráskód alapjai

### 3.1 Megjegyzések

```lua
-- egysoros megjegyzés
--[[ blokkmegjegyzés,
     több soron át ]]
```

### 3.2 Tördelés

Az újsorok és a behúzás nem számítanak. A blokkok `end`-del zárulnak. Az
utasítások közé nem kell elválasztó:

```lua
if x > 10 then x = 0 end
```

### 3.3 Literálok

| Literál | Példák | Típus |
|---------|--------|-------|
| decimális egész | `42`, `65_535` | egész konstans |
| hexadecimális | `0x7F`, `$7F` | egész konstans |
| bináris | `0b1010_0001` | egész konstans |
| tizedes tört | `1.5`, `0.25`, `-0.1` | `fixed` |
| karakter | `'A'`, `'\n'`, `'\''` | egész konstans (ASCII-kód) |
| string | `"Hello\n"` | string |
| logikai | `true`, `false` | `bool` |

A `_` jel tagolhatja a számjegyeket. A stringekben használható
escape-szekvenciák: `\n \t \\ \" \0`. A karakterliterálokban: `\n \t \\ \' \0`.
Egy string literál legfeljebb 255 karakter lehet.

### 3.4 Nevek

A nevek betűkből, számjegyekből és `_` jelből állnak, és nem kezdődhetnek
számjeggyel. A típusnevek (struktúrák és osztályok) nagybetűvel kezdődnek. A
kis- és nagybetű különbözik: a `score` és a `Score` két külön név.

### 3.5 Foglalt szavak

Az alábbi nevek nem használhatók változó, konstans, függvény, paraméter,
ciklusváltozó, típus vagy erőforrás (asset) neveként. A fordító ilyenkor ezt
jelzi: `` `név` is a reserved name ``.

**Kulcsszavak**

`and` `or` `not` `if` `then` `elseif` `else` `end` `while` `do` `for`
`function` `return` `break` `const` `var` `true` `false` `asm` `struct`
`class` `asset` `interrupt` `far` `include`

**Típusnevek és a `self`**

`bool` `byte` `sbyte` `word` `sword` `fixed` `long` `fixed16` `string`, valamint a `self` (az
objektum a metóduson belül).

**Beépített függvények** (a fordító valósítja meg őket)

`print` `write` `at` `ink` `cls` `border` `vsync` `pause` `frames` `key` `kempston` `any_key` `wait_key` `cpu_speed` `random` `seed` `memory` `nextreg` `port` `addr` `dma_copy` `dma_fill` `plot` `point` `hline` `layer2_clear` `layer2_flip` `sprite_load` `ay_write` `note_period` `abs` `min` `max` `clamp` `sqrt` `sin` `cos` `atan2` `round` `len` `sub` `find` `val` `asc` `chr` `str` `upper` `lower` `draw_image` `image_width` `image_height` `layer2_mode` `layer2_width` `layer2_height` `sample_play` `sample_loop` `sample_stop` `sample_playing`

**A szabványos könyvtár nevei** (LZ80N-ben írt függvények, típusok és
konstansok). **Minden** programban foglaltak, akkor is, ha a program nem
használja az adott modult, így egy név jelentése sosem változik meg attól,
hogy a program elkezdi használni a modult:

| Modul | Nevek |
|-------|-------|
| sprite | `Sprite` `sprite` `sprites_visible` |
| Layer 2 | `layer2_visible` `rgb` `vline` `line` `rect` `fill_rect` `circle` `fill_circle` `ellipse` `fill_ellipse` `triangle` `fill_triangle` `TRANSPARENT` `L2_256` `L2_320` `L2_640` |
| paletta | `palette` `palette9` `palette_select` `rgb9` `PAL_ULA` `PAL_LAYER2` `PAL_SPRITES` `PAL_TEXT` `PAL_ULA_2` `PAL_LAYER2_2` `PAL_SPRITES_2` `PAL_TEXT_2` |
| copper | `copper_begin` `copper_wait` `copper_move` `copper_start` `copper_stop` |
| hang | `sound_reset` `sound_mix` `tone` `noise` `volume` `sound_off` `beep` `play` `play_loop` `playing` `sound_tick` |

A **`snd_`** kezdetű nevek a hangmodul belső változóinak és függvényeinek,
az **`l2_`** kezdetűek a Layer 2 modul segédfüggvényeinek vannak fenntartva.

**Beépített konstansok** (már léteznek, ezért újra deklarálni hiba):
`BLACK` … `WHITE`, `BRIGHT_BLACK` … `BRIGHT_WHITE`, `KEY_…`, `JOY_…`,
`MHZ_3` `MHZ_7` `MHZ_14` `MHZ_28`, `NOT_FOUND`.

**Egy névtér.** A függvények, a típusok és a globális változók nevei közösek:
egy változó nem kaphatja egy függvény vagy típus nevét.

**Struktúrán és osztályon belül szabad.** A mező- és metódusnevek a saját
típusukhoz tartoznak, így a kulcsszavakon kívül bármi lehet a nevük: egy
osztálynak lehet `line` mezője vagy `update()` metódusa.

### 3.6 Több forrásfájl: `include`

A program több fájlra bontható. Az `include "fájl"` egy másik forrásfájlt
illeszt be a beillesztő fájl legfelső szintjén:

```lua
-- game.lz80n
include "lib/screen.lz80n"     -- title(), status()
include "lib/enemies.lz80n"    -- az Enemy osztály

title("My game")
```

* Az útvonal az `include`-ot tartalmazó fájlhoz képest relatív (a `lib/`
  mappában lévő fájl a szomszédjait így éri el: `include "other.lz80n"`).
* Minden fájl **egyszer** kerül be: ugyanannak a fájlnak a második
  `include`-ja kimarad. A fájlok így maguk is beilleszthetik, amire
  szükségük van, és akár kölcsönösen is hivatkozhatnak egymásra.
* Az `include` csak a fájl legfelső szintjén állhat, függvényen, osztályon
  vagy blokkon belül nem. A beillesztett fájlban bármi lehet, ami egy
  programban: függvények, osztályok, globális változók, konstansok,
  erőforrások és legfelső szintű utasítások (ezek ott futnak le, ahol az
  `include` áll).
* Az erőforrások útvonala (`asset x = "..."`) az erőforrást deklaráló
  fájlhoz képest relatív.
* Minden egyetlen programmá áll össze, egyetlen névtérrel: az egyik fájlban
  deklarált név az utána következő összes fájlban látszik, és ugyanaz a név
  két fájlban hiba. A hibaüzenet megnevezi a fájlt:
  `lib/enemies.lz80n:12:5: error: ...`
* A `modules.lz80n` példa a `lib/screen.lz80n` és `lib/ball.lz80n` fájlokat
  használja.

---

## 4. Típusok, változók és konstansok

### 4.1 Alaptípusok

| Típus | Méret | Tartomány | Megjegyzés |
|-------|-------|-----------|------------|
| `bool` | 1 bájt | `false` / `true` | számokkal nem keverhető |
| `byte` | 1 | 0 … 255 | előjel nélküli |
| `sbyte` | 1 | -128 … 127 | előjeles |
| `word` | 2 | 0 … 65535 | előjel nélküli |
| `sword` | 2 | -32768 … 32767 | előjeles |
| `fixed` | 2 | -128 … 127,996 | 8.8 fixpontos, lépésköz 1/256 |
| `long` | 4 | -2 147 483 648 … 2 147 483 647 | előjeles 32 bites (10.4) |
| `fixed16` | 4 | -32768 … 32767,99998 | 16.16 fixpontos, lépésköz 1/65536 (10.4) |
| `string` | 65 | 0 … 64 karakter | szöveg |

A legkisebb, még elegendő típust válaszd. A Z80-on a 8 bites műveletek a
leggyorsabbak.

### 4.2 Változók

```lua
byte lives = 3
word score            -- kezdőérték nélkül: 0
sword dx = -2
bool alive = true
var x = 1000          -- a típus az értékből: word
```

* A függvényeken kívül deklarált változók **globálisak**, és mindenhonnan
  láthatók.
* A blokkon belül (függvényben, `if`-ben, ciklusban …) deklarált változók
  **lokálisak** az adott blokkra.
* A kezdőérték nélküli skalár változók 0-ról indulnak.

### 4.3 Konstansok

```lua
const SPEED = 2
const GRAVITY = 0.25        -- fixed konstans
const TITLE = "SPACE RACE"  -- string konstans
```

A konstansok fordítási idejű értékek. Nem foglalnak memóriát, és a csak
konstansokból álló kifejezéseket a fordító számolja ki. Egy egész konstans
annak a helynek a típusát veszi fel, ahol használod, és bele kell férnie.

### 4.4 Tömbök

```lua
byte map[256]
word scores[10]
string names[4]
Enemy enemies[8]      -- struktúrák tömbje (lásd a 8. fejezetet)

map[5] = 1
scores[i] += 10
```

* A méret konstans kifejezés. Az indexelés 0-tól indul.
* A kezdőérték nélküli tömbök nullákkal teltek (a stringek üresek).
* Az indexeket futás közben **nem ellenőrzi** a program, így a kód gyors marad.
  A konstans indexeket a fordító ellenőrzi.
* Egy teljes tömböt nem lehet egyben értékül adni. Használj ciklust vagy
  `dma_copy`-t.

A **többdimenziós tömböknek** dimenziónként egy méretük van (legfeljebb 4):

```lua
byte grid[24][32]                 -- 24 sor, soronként 32 elem
grid[y][x] = 1
grid[y][x + 1] += grid[y][x]
word table[4][3] = {{1, 2, 3}, {4, 5, 6}}   -- a sorok beágyazott listák
byte flat[2][3] = {1, 2, 3, 4, 5, 6}        -- vagy egy lapos lista, soronként
string names[][2] = {{"Ann", "Bob"}, {"Cid"}}   -- [] : a sorok száma a listából
Enemy wave[3][8]                  -- struktúrából is
struct Map
    byte cells[16][16]            -- és struktúramezőként is
end
```

* Az elemek soronként követik egymást (az utolsó index változik a
  leggyorsabban), így a `grid[y][x]` a `y * 32 + x` helyen van.
* Önmagában a `grid[y]` egy sor, amit csak tovább lehet indexelni.
* Csak az első méret lehet `[]`. A beágyazott lista rövidebb sorait nullák
  egészítik ki.

### 4.5 Tömb-kezdőértékek

```lua
byte heights[] = {20, 45, 80, 120}        -- [] : a méret a listából adódik
word scores[10] = {500, 400, 300}         -- a többi elem 0
sbyte dx[] = {-1, 0, 1}
fixed speed[] = {0.5, 1, -1.25}
bool solid[] = {true, false, true}
byte keys[] = {'Q', 'A', 'O', 'P'}
string menu[] = {"Start", "Options", "Quit"}
const N = 3
word table[] = {N * 100, N * 200}         -- konstans kifejezés is lehet
```

* Az értékeknek az elemtípusnak megfelelő **fordítási idejű konstansoknak**
  kell lenniük (számok, karakterek, nevesített konstansok, konstans
  kifejezések; `string` tömbnél string literálok és string konstansok).
* Ha kevesebb az érték, mint az elem, a többi 0 (illetve `""`). Ha több, az
  hiba.
* **Globális** tömböknél az értékek a programfájl részei. A kezdőérték
  beállítása nem kerül sem kódba, sem időbe.
* **Lokális** tömböknél (függvényen belül) az értékek minden alkalommal
  bemásolódnak (egy `LDIR`-rel), amikor a deklaráció lefut, így minden hívás
  az eredeti táblával indul.
* Struktúratömbnek még nem lehet kezdőértéke.

### 4.6 Konverziók (típuskényszerítés)

Egy érték mindig **bővíthető** automatikusan: `byte` → `word`, `sbyte` →
`sword`, bármely egész → `long`, `fixed` → `fixed16`. A **szűkítést** ki kell írni:

```lua
word w = 1000
byte b = byte(w)        -- 232: az alsó 8 bit marad
sword s = sword(w)
fixed f = fixed(b)      -- egész -> fixed
sword n = sword(f)      -- fixed -> egész (lefelé kerekít)
```

Az azonos szélességű előjelváltás (`word` ↔ `sword`, `byte` ↔ `sbyte`)
automatikus.

---

## 5. Kifejezések és operátorok

### 5.1 Operátorok

| Erősség (gyenge → erős) | Operátorok |
|---|---|
| 1 | `or` |
| 2 | `and` |
| 3 | `==  ~=  !=  <  <=  >  >=` |
| 4 | `\|` (bitenkénti vagy) |
| 5 | `~` (bitenkénti kizáró vagy) |
| 6 | `&` (bitenkénti és) |
| 7 | `<<  >>` |
| 8 | `..` (stringösszefűzés) |
| 9 | `+  -` |
| 10 | `*  /  %` |
| 11 | egyoperandusú `-  not  ~` |

A `~=` és a `!=` egyaránt „nem egyenlő”. Az egyoperandusú `~` bitenkénti
negálás, a kétoperandusú `~` kizáró vagy (mint a Lua 5.3-ban).

Összetett értékadások: `+=  -=  *=  /=  %=  &=  |=  <<=  >>=`.
Az `a[i] /= 2` jelentése `a[i] = a[i] / 2`.

### 5.2 Hogyan számol az aritmetika

* **Az egész aritmetika körbefordul.** `byte b = 250  b += 10` eredménye 4.
  Túlcsordulási hiba nincs.
* **A cél határozza meg a szélességet.** Egy kifejezés annak a változónak,
  paraméternek vagy visszatérési értéknek a szélességén számolódik, amely
  megkapja:

  ```lua
  byte a = 200
  byte b = 100
  word sum = a + b        -- 300: 16 biten számol
  byte low = a + b        -- 44: 8 biten számol
  ```

* **Előjel**: a művelet előjeles, ha bármelyik operandus vagy a cél előjeles.
* **Osztás**: nulla felé csonkít; a `%` eredménye az osztandó előjelét veszi
  fel. Konstans 0-val osztás fordítási hiba. Futás közben `x / 0` eredménye
  65535.
* Eltolás: a `>>` előjeles típusnál aritmetikai (megtartja az előjelet),
  előjel nélkülinél logikai.

### 5.3 A feltétel `bool`

A feltételnek `bool` értéknek kell lennie. A Lua és a C „a 0 hamis”
szabályát az LZ80N szándékosan nem veszi át:

```lua
if count ~= 0 then ... end      -- helyes
if count then ... end           -- hiba: a feltételnek bool-nak kell lennie
```

Az `and` és az `or` rövidzáras: a jobb oldal csak akkor értékelődik ki, ha
szükség van rá.

---

## 6. Vezérlési szerkezetek

```lua
if hp == 0 then
    game_over()
elseif hp < 10 then
    ink(RED)
else
    ink(GREEN)
end

while not key(KEY_SPACE) do
    vsync()
end

for i = 0, 9 do ... end          -- 0, 1, ..., 9 (mindkét határ benne van)
for i = 10, 0, -2 do ... end     -- 10, 8, ..., 0
for x = left, right do ... end   -- változó határok

while true do
    if done then break end
end
```

A `for` ciklus:

* A ciklusváltozó új lokális változó, típusa a határokból adódik.
* A végértéket a program **egyszer** számolja ki, a ciklus indulása előtt.
* A lépésköz nem nulla konstans.
* A ciklus soha nem „fut túl”: `byte` változóval a `for i = 0, 255` pontosan
  256-szor fut, a `for i = 3, 2` egyszer sem.
* A `break` a legbelső ciklusból lép ki.

---

## 7. Függvények

```lua
function add(byte a, byte b) -> word
    return a + b
end

function clear_line(byte row)
    at(row, 0)
    write("                                        ")
end

word total = add(200, 100)       -- 300
clear_line(5)
```

* A paramétereket a típusukkal együtt kell megadni. A visszatérési típus a
  `->` után áll. A `->` nélküli függvény nem ad vissza értéket.
* A számok és a `bool` értékek **érték szerint** adódnak át.
* **A struktúrák, osztályok és stringek hivatkozás szerint adódnak át**: a
  függvény a hívó változóján dolgozik, és meg is változtathatja (mint a
  táblák a Luában).
* Függvény visszaadhat `string`-et (`-> string`) és struktúrát (`-> Vec`, 8.3).

### 7.1 Rekurzió

Egy függvény meghívhatja önmagát, közvetlenül vagy más függvényeken
keresztül is:

```lua
function factorial(word n) -> word
    if n <= 1 then return 1 end
    return n * factorial(n - 1)
end

function hanoi(byte n, byte from, byte to, byte via)
    if n == 0 then return end
    hanoi(n - 1, from, via, to)
    hanoi(n - 1, via, to, from)
end
```

Hogyan működik, és mibe kerül:

* A gyors hívások érdekében minden függvény paraméterei és lokális változói
  **rögzített memóriacímen** vannak (statikus keretek). Ha egy hívás
  visszatérhet a hívóhoz (a hívott maga a hívó, vagy újra meghívhatja), a
  hívó a hívás előtt a veremre másolja a saját keretét, utána pedig
  visszaállítja. A közönséges hívások ezért semmivel sem lassabbak.
* Egy rekurzív hívás ára a hívó keretével nő: a paraméterek és lokális
  változók bájtjaira kb. 10 T-ciklus bájtonként. Nagy lokális tömböt ne
  tegyél rekurzív függvénybe (legyen globális).
* A veremnek a program alatt kb. 1–2 KB hely jut, a program méretétől
  függően. A túl mély rekurzió "stack overflow (recursion too deep)"
  üzenettel megállítja a programot, és nem írja felül a memóriát.
* Egy függvényt nem használhat egyszerre a főprogram és egy
  megszakításkezelő (18. fejezet): a megszakítás akkor is jöhet, amikor a
  főprogram éppen benne jár. A fordító ezt hibaként jelzi.

A `recursion.lz80n` példa fraktálfát rajzol, és megoldja a Hanoi tornyait.

---

## 8. Struktúrák és osztályok

### 8.1 Struktúrák

```lua
struct Vec2
    sbyte x
    sbyte y
end

struct Player
    word x
    word y
    Vec2 speed           -- beágyazott struktúra
    byte health = 100    -- alapérték
    byte items[8]        -- tömb mező
end

Player p
p.x = 120
p.speed.x = -1
```

* A mezők **tömörítve**, a deklaráció sorrendjében, kitöltés nélkül kerülnek
  a memóriába.
* Egy struktúraváltozónak saját memóriája van. Az `a = b` a teljes
  struktúrát másolja.
* Az alapértékeknek (`= 100`) konstansnak kell lenniük. Minden alkalommal
  beíródnak, amikor a deklaráció lefut: globálisnál a főprogram adott
  pontján, lokálisnál a blokkba lépéskor.

### 8.2 Osztályok

Az osztály függvényekkel (metódusokkal) ellátott struktúra:

```lua
class Ball
    byte x = 16
    byte y = 12
    Vec2 vel

    function move() -> bool
        self.x += self.vel.x
        self.y += self.vel.y
        return self.x > 38
    end

    function draw()
        at(self.y, self.x)
        write("o")
    end
end

Ball balls[3]
balls[0].vel.x = 1
if balls[0].move() then balls[0].vel.x = -1 end
balls[0].draw()
```

* A metóduson belül a `self` maga az objektum. A mezőkre mindig
  `self.mező` alakban kell hivatkozni.
* Az `obj.method(...)` hívás közvetlen `CALL`: nincs virtuális függvény és
  nincs többletköltség.

### 8.3 Struktúraértékek: listák, konstruktorok, visszatérési értékek

Egy struktúraérték megadható a mezői `{...}` listájával, deklarációs
sorrendben, vagy **konstruktorral**, amikor a típus nevét függvényként
hívjuk:

```lua
struct Vec
    sword x
    sword y
end

Vec a = {3, 4}                  -- mezők sorrendben; ami hiányzik: alapérték / 0
Vec b = Vec(10, 20)             -- konstruktor
a = {a.y, a.x}                  -- az értékek tetszőleges kifejezések lehetnek
Vec pts[] = {{1, 2}, {3, 4}, {5}}   -- struktúratömb: elemenként egy lista

function add(Vec p, Vec q) -> Vec    -- függvény visszaadhat struktúrát
    return Vec(p.x + q.x, p.y + q.y)
end

Vec c = add(a, {1, 1})          -- lista és konstruktor argumentumként is jó
var d = add(c, c)               -- a `var` az értékből veszi a típust
```

* Ha az osztálynak van `init` metódusa, az építi fel: a `Ship(10, 20)`
  először beírja a mezők alapértékeit, aztán meghívja az új objektumon az
  `init(10, 20)`-at. `init` nélkül az argumentumok sorban a mezőkbe kerülnek.
* Beágyazott struktúrához és tömbmezőhöz beágyazott lista kell:
  `Pix p = {{1, 2}, rgb(7, 0, 0)}`.
* A csak konstansokból álló lista betöltéskor semmibe sem kerül
  (globális), illetve egy `LDIR` (lokális); a többi lista és a konstruktor
  mezőnként épül fel.
* A struktúrát visszaadó függvény egy statikus pufferbe ír, amit a hívó
  azonnal átmásol, így az eredmény önálló érték.
* Struktúraérték tárolható, visszaadható és átadható; kifejezésben
  közvetlenül (`add(a, b).x`) előbb változóba kell tenni.

### 8.4 Öröklődés

```lua
class Shape
    Vec pos
    byte colour = 255
    function init(sword x, sword y)
        self.pos = Vec(x, y)
    end
    function move() ... end
    function draw() ... end
end

class Ball extends Shape
    byte radius = 6                 -- az új mezők a szülőéi után jönnek
    function init(sword x, sword y, byte r)
        super.init(x, y)            -- a szülő változata
        self.radius = r
    end
    function draw()                 -- felülírja a Shape.draw-t
        fill_circle(self.pos.x, self.pos.y, self.radius, self.colour)
    end
end

Ball b = Ball(100, 50, 8)
b.move()                            -- örökölt
b.draw()                            -- a Ball saját metódusa
```

* A leszármazott osztályban elöl, ugyanazokon az eltolásokon vannak a
  szülő mezői, utánuk a sajátok; a metódusokat örökli, hacsak nem írja
  felül őket.
* A `super.m(...)` a szülő metódusát hívja a `self`-en; a
  `Shape.m(obj, ...)` egy adott osztály metódusát hívja kifejezetten.
* `Ball` átadható ott, ahol `Shape` kell, és értékül adható `Shape`
  változónak (csak a `Shape` rész másolódik).
* A metódusok fordításkor kötődnek (**nincs virtuális hívás**): a `Shape`-et
  váró függvény a `Shape` metódusait hívja akkor is, ha `Ball`-t kap. A
  `Ball` saját metódusaiban a `self.draw()` a `Ball.draw`. Így minden hívás
  közvetlen `CALL` marad.
* A szülőt a leszármazott előtt kell deklarálni.

Az `inherit.lz80n` példa mindezt bemutatja.

---

## 9. Stringek

```lua
string name = "Next"
string s = "Hello, " .. name .. "! Score: " .. 42
print(s)                          -- Hello, Next! Score: 42
if find(s, "Next") ~= NOT_FOUND then print(upper(name)) end
```

### 9.1 A stringek tárolása

* Egy `string` változó **legfeljebb 64 karaktert** tárol: egy hosszbájtot és
  utána a szöveget (összesen 65 bájt).
* Nincs heap és nincs szemétgyűjtő. Minden stringváltozónak saját,
  rögzített helye van.
* A hosszabb eredmény **64 karakternél levágódik**; ez nem hiba.
* Az értékadás lemásolja a szöveget. Az `s = "x" .. s` biztonságos.
* A string **literálok és konstansok** 255 karakteresek is lehetnek, így
  hosszú szöveget vagy dallamot közvetlenül ki lehet írni vagy le lehet
  játszani. Ha egy ilyen változóba kerül, az első 64 karaktere marad meg.

### 9.2 Műveletek

* A `..` összefűz. A számok, `fixed` és `bool` értékek automatikusan szöveggé
  alakulnak: `"x=" .. x`.
* Az `==  ~=  <  <=  >  >=` szótári (bájt-) sorrend szerint hasonlít.
* A pozíciók **0-tól** számolódnak.

| Függvény | Eredmény | Leírás |
|----------|----------|--------|
| `len(s)` | byte | a karakterek száma |
| `sub(s, from, count)` | string | `s` egy része; `s` végénél levágódik |
| `find(s, t)` | byte | `t` első előfordulása `s`-ben, vagy `NOT_FOUND` (255) |
| `val(s)` | sword | az `s` elején álló szám: `"-45"` → -45, `"7up"` → 7, `""` → 0 |
| `str(x)` | string | szám, `fixed` vagy `bool` érték szövegként |
| `asc(s, i)` | byte | az `i`. karakter kódja (0 a stringen kívül) |
| `chr(c)` | string | egykarakteres string |
| `upper(s)`, `lower(s)` | string | nagy- / kisbetűsítés (ASCII betűk) |

### 9.3 Stringfüggvények

```lua
function greet(string who) -> string
    return "Hi " .. who .. "!"
end

function shout(string s)       -- hivatkozás szerint: a hívó stringjét változtatja
    s = upper(s)
end

string n = "bob"
shout(n)
print(greet(n))                -- Hi BOB!
```

---

## 10. Fixpontos számok és matematika

### 10.1 Miért `fixed` és nem lebegőpontos

A Z80N-nek nincs lebegőpontos egysége. Egy szoftveres lebegőpontos művelet
több száz vagy több ezer órajelciklusba kerül. Ezért az LZ80N egy 16 bites,
**8.8 fixpontos** típust ad: 8 bit egészrész és 8 bit törtrész. A Next-játékok
is ezt használják sima mozgáshoz, sebességhez és gravitációhoz.

```lua
fixed y = 10
fixed vy = 0
const G = 0.2

vy = vy + G        -- az esési sebesség nő
y = y + vy
plot(100, byte(y), 255)
```

### 10.2 Szabályok

* A tizedesponttal írt szám `fixed`: `1.5`, `-0.25`. Az értékek 1/256-ra
  kerekednek: a `0.1` tárolt értéke 26/256 = 0,1016.
* Az egész **konstansok** automatikusan illeszkednek: `f + 1`, `f > 2`,
  `fixed v = 0`.
* Az egész **változókat** át kell alakítani: `f + fixed(n)`. Kivétel, mert
  pontos és olcsó: `fixed * egész`, `egész * fixed` és `fixed / egész`.
* A `fixed * fixed` és `fixed / fixed` gyors runtime rutint hív. Az eredmény
  nulla felé csonkul.
* Konverziók: `fixed(n)`, `sword(f)` / `byte(f)` (lefelé kerekít),
  `round(f)` → `sword` (a fél felfelé kerekedik).
* A `print` legfeljebb két tizedesjegyet ír ki: `1.5`, `-0.38`, `3`.

### 10.3 Matematikai függvények

| Függvény | Típusok | Leírás |
|----------|---------|--------|
| `abs(x)` | bármely szám | abszolút érték |
| `min(a, b)`, `max(a, b)` | bármely számok | kisebbik / nagyobbik |
| `clamp(x, lo, hi)` | bármely számok | `x` korlátozása `lo … hi` közé |
| `sqrt(x)` | egész → `byte`, `fixed` → `fixed` | négyzetgyök (lefelé kerekítve) |
| `sin(a)`, `cos(a)` | `byte` szög → `fixed` | szinusz / koszinusz táblázatból |
| `atan2(y, x)` | `sword` vagy `fixed` → `byte` szög | az (x, y) vektor iránya |
| `round(f)` | `fixed` → `sword` | kerekítés |

**A szögek bájtok**: a teljes kör 256 egység (64 = 90°, 128 = 180°). A szög
így ingyen körbefordul: `angle += 3`. Az `atan2` a +x irányra 0-t, a +y
irányra 64-et ad.

```lua
byte a = 0
while true do
    vsync()
    byte x = 128 + byte(sword(cos(a) * 60))
    byte y = 96 + byte(sword(sin(a) * 60))
    plot(x, y, rgb(7, 7, 0))
    a += 1
end
```

A szinusz- és arkusztangens-táblázat (kb. 0,6 KB) csak akkor kerül a
programba, ha a `sin`, `cos` vagy `atan2` szerepel benne.

### 10.4 32 bites számok: `long` és `fixed16`

Ha a 16 bit nem elég:

| Típus | Méret | Tartomány | Mire |
|-------|-------|-----------|------|
| `long` | 4 bájt | -2 147 483 648 … 2 147 483 647 | nagy pontszámok, számlálók, szorzatok |
| `fixed16` | 4 bájt | -32768 … 32767,99998, lépésköz 1/65536 | pontos pozíciók és fizika |

```lua
long score = 0
score += long(level) * 25000         -- szorzás előtt bővíts
long f = 1
for i = 2, 12 do
    f *= i                           -- 479001600
end

fixed16 x = 4
fixed16 vy = -3.5
vy += 0.05                           -- a kis lépések pontosan összeadódnak
plot(byte(x), byte(y), c)
```

* Az egészek (`byte` … `sword`) automatikusan `long`-gá bővülnek, a `fixed`
  `fixed16`-tá. A többi irányhoz konverzió kell: `long(f)`, `fixed16(n)`,
  `word(l)`, `byte(l)`, `fixed(f16)`. Fixpontosból egészbe lefelé kerekít.
* `long`-on minden operátor működik (`+ - * / % & | ~ << >>`,
  összehasonlítások; a `>>` megtartja az előjelet). A `fixed16`-nak
  `+ - * / %` és összehasonlítás van; a `fixed16 * egész` és
  `fixed16 / egész` pontos.
* A `print`, `write`, `str()` és `..` a `long`-ot teljes egészében, a
  `fixed16`-ot legfeljebb négy tizedessel mutatja. Az `abs`, `min`, `max`
  és `clamp` is működik.
* `long` és `fixed16` lehet változó, tömbelem (inicializálással is),
  struktúramező, paraméter és visszatérési érték. Még nem: `far` tömb
  belőlük, és `long`-gal számoló `for` ciklus (helyette `while`).
* Sebesség: az összeadás és az összehasonlítás néhány tucat T-ciklus, a
  szorzás kb. 700, az osztás kb. 3000 (futtatórendszer-rutinok, a szorzás
  a Z80N `MUL` utasítását használja). Ahol elég, használj 16 bites típust.
* A tizedesponttal írt szám `fixed`, ha elfér 8.8-ban, különben `fixed16`.
  `fixed16` változóba írva a teljes pontosságát megtartja.

A `bignum.lz80n` példa mindkét típust használja.

---

## 11. Szövegkiírás

A szövegréteg a Next **tilemapjét** használja: 40 oszlop × 32 sor, 8 × 8
pixeles karakterekkel, amelyek a teljes 320 × 256-os képet lefedik, a keretet
is beleértve. A 256 × 192-es „papír” terület a 4–35. oszlop és a 4–27. sor. A
betűkészlet a fordítóba van beépítve (32–127. karakter).

| Hívás | Leírás |
|-------|--------|
| `print(a, b, …)` | értékek kiírása szóközzel elválasztva, majd új sor |
| `write(a, b, …)` | értékek kiírása elválasztó és új sor nélkül |
| `at(row, col)` | kiírási pozíció (sor 0–31, oszlop 0–39) |
| `ink(c)` | szövegszín 0–15 |
| `cls()` | a szövegréteg törlése, kurzor a 0,0 pozícióra |

```lua
cls()
at(2, 4)
ink(BRIGHT_CYAN)
write("SCORE ")
ink(BRIGHT_WHITE)
write(score)
print("")                 -- új sor
print("x =", x, "fixed:", 1.5, "ok:", true)
```

* A `print` és a `write` számot, `fixed` és `bool` értéket, valamint stringet
  is elfogad.
* A 40 karakternél hosszabb sor a következő sorban folytatódik. A képernyő
  alján a szöveg felfelé gördül.
* Színek: `BLACK BLUE RED MAGENTA GREEN CYAN YELLOW WHITE` (0–7) és
  `BRIGHT_BLACK` … `BRIGHT_WHITE` (8–15).
* A szöveg háttere átlátszó: a Layer 2, a sprite-ok és a háttérszín
  átlátszanak rajta.
* A `border(c)` az összes réteg mögötti háttérszínt állítja. Ez a keret, és
  a papír is ott, ahol semmi más nincs rajzolva.
* A `print` minden argumentumot a saját típusán számol ki: `byte m = 20`
  esetén a `print(m * 13)` 4-et ír ki (8 bites körbefordulás). 260-hoz írd
  így: `word(m) * 13`.

---

## 12. Bevitel: billentyűzet és joystick

```lua
if key(KEY_O) then x -= 1 end
if key(KEY_P) then x += 1 end
if key(KEY_SPACE) then fire() end

byte j = kempston()
if j & JOY_FIRE ~= 0 then fire() end
if j & JOY_LEFT ~= 0 then x -= 1 end
```

| Hívás | Leírás |
|-------|--------|
| `key(KEY_x)` → bool | le van-e nyomva a billentyű |
| `kempston()` → byte | a Kempston joystick bitjei (`JOY_RIGHT`, `JOY_LEFT`, `JOY_DOWN`, `JOY_UP`, `JOY_FIRE`) |
| `any_key()` → bool | le van-e nyomva bármelyik billentyű |
| `wait_key()` | megvárja, hogy minden billentyű felengedjen, majd egy lenyomást |

Billentyűkonstansok: `KEY_A` … `KEY_Z`, `KEY_0` … `KEY_9`, `KEY_SPACE`,
`KEY_ENTER`, `KEY_CAPS` (Caps Shift), `KEY_SYM` (Symbol Shift).

---

## 13. Idő, véletlenszámok, processzorsebesség

| Hívás | Leírás |
|-------|--------|
| `vsync()` | várakozás a következő képkockára (másodpercenként 50) |
| `pause(n)` | `n` képkocka várakozás |
| `frames()` → word | képkockaszámláló a program indulása óta |
| `random(n)` → word | véletlenszám 0 … n-1; a `random(0)` teljes 16 bites értéket ad |
| `seed(w)` | a véletlensorozat indítása `w`-ből (azonos mag → azonos sorozat) |
| `cpu_speed(s)` | `MHZ_3`, `MHZ_7`, `MHZ_14`, `MHZ_28` (a program 28 MHz-en indul) |

Tipikus játékciklus:

```lua
while true do
    vsync()          -- képkockánként egyszer, a kép alján
    read_input()
    move_things()
    draw()
end
```

Időmérés:

```lua
word t = frames()
do_work()
print("took", (frames() - t) * 20, "ms")
```

---

## 14. Layer 2 grafika

A Layer 2 a Next 256 × 192-es bittérképes rétege, pixelenként egy bájttal. A
szín 8 bites `RRRGGGBB` érték. A `TRANSPARENT` (`$E3`) átengedi az alatta lévő
rétegeket.

| Hívás | Leírás |
|-------|--------|
| `layer2_visible(on)` | a Layer 2 megjelenítése / elrejtése |
| `layer2_clear(c)` | a képernyő kitöltése `c` színnel (zxnDMA, nagyon gyors) |
| `plot(x, y, c)` | pixel rajzolása |
| `point(x, y)` → byte | pixel kiolvasása |
| `hline(x, y, len, c)` | vízszintes vonal |
| `vline(x, y, len, c)` | függőleges vonal |
| `line(x0, y0, x1, y1, c)` | szakasz két pont között |
| `rect(x, y, w, h, c)` | téglalap körvonala |
| `fill_rect(x, y, w, h, c)` | kitöltött téglalap |
| `rgb(r, g, b)` → byte | szín összeállítása: r, g 0–7, b 0–3 |
| `circle(x, y, r, c)` | kör körvonala |
| `fill_circle(x, y, r, c)` | kitöltött kör |
| `ellipse(x, y, rx, ry, c)` | ellipszis körvonala, `rx` és `ry` sugárral |
| `fill_ellipse(x, y, rx, ry, c)` | kitöltött ellipszis |
| `triangle(x0, y0, x1, y1, x2, y2, c)` | háromszög körvonala |
| `fill_triangle(x0, y0, x1, y1, x2, y2, c)` | kitöltött háromszög (a csúcsok sorrendje tetszőleges) |
| `draw_image(img, x, y)` | kép kirajzolása (lásd 16.1) |
| `layer2_flip()` | dupla pufferelés: megjeleníti az eddig rajzolt képet, a rajzolás a másik pufferben folytatódik |
| `layer2_mode(m)` | `L2_256`, `L2_320` vagy `L2_640` (14.1) |
| `layer2_width()` / `layer2_height()` → word | a pillanatnyi mód mérete |

A kör, az ellipszis és a kitöltött háromszög `sword` koordinátákat kap, és
**vágja** az alakzatot: az bármelyik szélen kilóghat a képernyőről. A sugár
0–255.

```lua
layer2_visible(true)
layer2_clear(rgb(0, 0, 1))
for x = 0, 255 do
    vline(byte(x), 0, 192, byte(x))     -- színsávok
end
line(0, 0, 255, 191, rgb(7, 7, 3))
fill_rect(100, 80, 56, 32, rgb(7, 0, 0))
```

**Villogásmentes dupla pufferelés:** ha egy kép megrajzolása egy képkockánál
tovább tart, rajzolj a rejtett pufferbe, majd cserélj:

```lua
while true do
    layer2_clear(0)
    draw_scene()
    layer2_flip()       -- egyszerre megjelenik, a rajzolás a másik pufferben folytatódik
end
```

A rétegek sorrendje **S-U-L**: felül a sprite-ok, alattuk a szöveg, legalul a
Layer 2.

A `plot` a képernyőn kívüli pixeleket (256 × 192-ben a 192. és az alatti sorokat) figyelmen kívül hagyja. A pixelek az
MMU 0. slotján (`$0000`–`$1FFF`) keresztül érhetők el. A `draw_image` másolás
közben az 1. slotot (`$2000`–`$3FFF`) is használja, utána visszateszi oda a
ROM-ot. Ha a program már
rajzolt a Layer 2-re, a `memory[0]` … `memory[$1FFF]` az utoljára használt
Layer 2 lapot mutatja, nem a ROM-ot.

### 14.1 320 × 256 és 640 × 256

```lua
layer2_mode(L2_320)        -- 320 x 256, 256 szín: a teljes kép, a kerettel együtt
layer2_mode(L2_640)        -- 640 x 256, 16 szín (palettaindex 0..15)
layer2_mode(L2_256)        -- vissza 256 x 192-re (ez az alapértelmezés)
word w = layer2_width()    -- 256, 320 vagy 640
word h = layer2_height()   -- 192 vagy 256
```

* Minden rajzoló hívás minden módban működik: x legfeljebb 639, y
  legfeljebb 255 lehet. Az alakzatokat a pillanatnyi méretre vágja; a
  `layer2_clear`, a `layer2_flip` és a `draw_image` is követi a módot.
* 640 × 256-ban egy pixel 0–15 palettaindex, és fele olyan széles, mint
  amilyen magas: kerek körhöz vízszintesen dupla sugár kell
  (`fill_ellipse(x, y, 2 * r, r, c)`). A 16 színt a
  `palette(PAL_LAYER2, i, szín)` tölti be.
* A széles módok oszloponként tárolják a pixeleket: a függőleges vonal és a
  `hline`/`fill_rect` 320 × 256-ban gyors, 640 × 256-ban pixelenként rajzol.
* A két puffer ekkor a 8–17. 16K-s bankot foglalja (a 8–13. helyett), a képek
  és a `far` tömbök fölöttük kezdődnek. Ezt a fordító intézi, ha a program
  hívja a `layer2_mode`-ot.

A `hires.lz80n` példa a két mód között vált.

---

## 15. Paletták

Minden rétegnek saját palettája van, így a színek újrarajzolás nélkül
változtathatók. Ez hasznos színciklushoz, áttűnéshez és villantáshoz.

| Hívás | Leírás |
|-------|--------|
| `palette(layer, index, colour)` | 8 bites `RRRGGGBB` szín |
| `palette9(layer, index, colour)` | 9 bites `RRRGGGBBB` szín |
| `rgb9(r, g, b)` → word | 9 bites szín: r, g, b 0–7 |
| `palette_select(layer, second)` | az első (`false`) vagy a második (`true`) paletta látszik |

`layer`: `PAL_ULA`, `PAL_LAYER2`, `PAL_SPRITES`, `PAL_TEXT`.
A `k` szövegszín a `PAL_TEXT` paletta `2k+1`. bejegyzése.

Minden rétegnek van **második palettája** is: `PAL_ULA_2`, `PAL_LAYER2_2`,
`PAL_SPRITES_2`, `PAL_TEXT_2`. A `palette_select(layer, second)` választja ki,
melyiket mutassa a réteg, így egy előre elkészített színkészlet (éjszakai
változat, villanás, áttűnés egy lépése) egyszerre jelenik meg:

```lua
for i = 0, 255 do
    palette(PAL_LAYER2_2, byte(i), dim(byte(i)))   -- a második paletta előkészítése
end
palette_select(PAL_LAYER2, true)                   -- megjelenítés
palette_select(PAL_LAYER2, false)                  -- és vissza
```

```lua
-- színciklus: minden képkockában 16 Layer 2 palettabejegyzés újraírása
byte phase = 0
while true do
    vsync()
    for i = 0, 15 do
        palette(PAL_LAYER2, 16 + i, rgb((i + phase) & 7, 0, 3))
    end
    phase += 1
end
```

Lásd: `examples/palette.lz80n`.

---

## 16. Hardveres sprite-ok és erőforrások

### 16.1 Erőforrások (asset)

```lua
asset sheet = "assets/sprites.png"
```

Az `asset`-et a fordító fordításkor alakítja át és építi be a programba. Az
útvonal a forrásfájlhoz képest relatív.

* `.png`: a képet (szélessége és magassága 16 többszöröse) 16 × 16-os
  sprite-mintákra vágja, balról jobbra és fentről lefelé, legfeljebb 64
  mintát. A színeket a Next `RRRGGGBB` formátumára alakítja. Az átlátszó
  pixelekből (alfa < 128) a sprite-átlátszósági szín (`$E3`) lesz.
* `.spr`: nyers 8 bites sprite-minták (egyenként 256 bájt).
* `.wav`: hangminta (17.3).

**Képek a Layer 2-re**

```lua
asset title = image("assets/title.png")       -- legfeljebb 256 x 255 pixel
asset logo = image("assets/logo.png")

draw_image(title, 0, 0)                       -- teljes képernyős kép
draw_image(logo, x, y)                        -- x, y: sword, lehet a képen kívül is
const LOGO_W = image_width(logo)              -- a méret fordításkor ismert
const LOGO_H = image_height(logo)
```

* A PNG-t a fordító `RRRGGGBB` formátumra alakítja. A pixelek **saját
  memórialapokra** kerülnek (a 14. 16K-s banktól felfelé), nem a 31 KB-os
  programterületre, így egy teljes 256 × 192-es kép (48 KB) a programban
  csak 6 bájtot foglal. A képek és a `far` tömbök (19.1) együtt kb. 780
  KB-ot használhatnak.
* A `draw_image` a képet a Layer 2-re másolja (abba a pufferbe, amelyikbe
  éppen rajzolunk), minden szélen vágva. A sorokat `LDIR` másolja: egy teljes
  képernyő 28 MHz-en kb. 2 képkocka.
* A PNG átlátszó pixeleit (alfa < 128) **kihagyja**, ott a képernyő
  tartalma látszik, mint a sprite-oknál. Átlátszó pixel nélküli képnél a
  gyorsabb, sima másolás fut.
* Az `image_width` és az `image_height` konstans.

### 16.2 Sprite-ok

A Nextnek 128 hardveres sprite-ja van: `sprite[0]` … `sprite[127]`.

```lua
sprite_load(0, sheet)              -- a minták feltöltése a 0. helytől
sprites_visible(true, false)       -- sprite-ok be, a kereten nem

sprite[0].position(152, 200)
sprite[0].show(0)                  -- 0. minta, látható

sprite[1].x = 60
sprite[1].y = 80
sprite[1].pattern = 2
sprite[1].mirror_x = true
sprite[1].scale = 1                -- 0..3: x1, x2, x4, x8
sprite[1].visible = true
sprite[1].update()                 -- a módosított mezők elküldése a hardvernek
```

| Mező | Leírás |
|------|--------|
| `x`, `y` | pozíció (word). A 256 × 192-es terület a 32, 32 pontnál kezdődik |
| `pattern` | minta sorszáma |
| `palette` | palettaeltolás 0–15 |
| `visible`, `mirror_x`, `mirror_y`, `rotate` | bool |
| `scale` | 0–3 |
| `id` | a sprite sorszáma (csak olvasható) |

| Metódus | Leírás |
|---------|--------|
| `position(x, y)` | mozgatás és frissítés |
| `show(pattern)` | minta beállítása és megjelenítés |
| `hide()` | elrejtés |
| `update()` | az összes mező elküldése a hardvernek |

Lásd: `examples/sprites.lz80n`, ahol egy osztály vezérli a sprite-okat.

---

## 17. Hang és zene

A Nextben három AY-3-8912 hangchip (TurboSound) van, ez **9 csatornát** ad:
csatorna = chip × 3 + hang, azaz a 0–2. csatorna a 0. chipen, a 3–5. az 1.-en,
a 6–8. a 2.-on szól.

### 17.1 Hangeffektek

```lua
beep(3, 60, 12)            -- 60-as hang (C5) a 3. csatornán 12 képkockáig; nem vár
tone(6, 300)               -- négyszögjel, AY-periódus 300
volume(6, 12)              -- 0..15
noise(7, 18)               -- zaj a 7. csatornán
sound_off(6)
```

| Hívás | Leírás |
|-------|--------|
| `beep(ch, note, frames)` | hang lejátszása adott számú képkockáig, a háttérben |
| `tone(ch, period)` | négyszögjel; frekvencia = 109375 / periódus Hz (periódus 1–4095) |
| `noise(ch, period)` | zaj, periódus 0–31 (chipenként egy zajgenerátor) |
| `volume(ch, v)` | hangerő 0–15 |
| `sound_off(ch)` | csatorna elnémítása és dallamának leállítása |
| `sound_reset()` | minden elnémítása |
| `note_period(n)` → word | az `n` hang AY-periódusa |
| `playing(ch)` → bool | szól-e még beep vagy dallam |
| `ay_write(chip, reg, value)` | AY-regiszter közvetlen írása |

A hangok sorszáma `n = oktáv × 12 + félhang`: C4 = 48, A4 = 57 (440 Hz),
C5 = 60.

Ereszkedő „lézer” hang, képkockánként frissítve:

```lua
byte laser = 15
word period = 40
while laser > 0 do
    vsync()
    tone(6, period)
    volume(6, laser)
    period += 25
    laser -= 1
end
sound_off(6)
```

### 17.2 Zene

```lua
play_loop(0, "o5 l12 e d c d e e e24 d d d24 e g g24")
play_loop(1, "o3 l24 c g c g c g < g > c")
play(2, "o4 l8 c e g >c")              -- egyszer
```

A dallamok jelölése a 128K / NextBASIC `PLAY` parancsához hasonló:

| Jel | Jelentés |
|-----|----------|
| `c d e f g a b` | hang. `#` vagy `+` = félhanggal fel, `-` = le. Utána szám = hossz képkockában: `c8`, `f#16` |
| `r` | szünet (ugyanazok a hosszszabályok) |
| `o4` | oktáv 0–8 (alapérték 4) |
| `>` `<` | egy oktávval fel / le |
| `l10` | alapértelmezett hanghossz képkockában (alapérték 10; 50 képkocka = 1 másodperc) |
| `v12` | hangerő 0–15 (alapérték 12) |

A szóközöket és az egyéb karaktereket a lejátszó figyelmen kívül hagyja. Minden
hang utolsó képkockája csendes, így az ismételt hangok külön hallatszanak.

A `play` és a `beep` **nem állítja meg a programot**. Egy LZ80N-ben írt
hangmeghajtó fut a képkocka-megszakításban, és lépteti az összes csatornát. A
játék közben tovább fut, miközben a zene szól.

Lásd: `examples/sound.lz80n`.

### 17.3 Minták (digitális hang)

```lua
asset boom = "assets/boom.wav"
asset engine = "assets/engine.wav"

sample_play(boom)            -- egyszer
sample_loop(engine)          -- újra meg újra
if sample_playing() then ... end
sample_stop()
```

* A 8 vagy 16 bites PCM `.wav` fájlokat (mono vagy sztereó) a fordító 8 bites
  monóvá alakítja. A 4 kHz és 31,25 kHz közötti mintavételezés megmarad, a
  többit 15,6 kHz-re alakítja. A minták a képekhez hasonlóan saját
  memórialapokon vannak, nem a 31 KB-os programterületen: egy másodperc
  15,6 kHz-en kb. 15 KB.
* A lejátszás **a háttérben** fut: a zxnDMA a minta ütemében küldi a
  bájtokat a DAC-ra (`$DF` port), egy `$7000`–`$7FFF`-en lévő 4 KB-os
  pufferen át, amelyet a képkocka-megszakítás tölt újra. A program tovább
  fut, és az AY hang (17.1, 17.2) is működik.
* Egyszerre egy minta szól; egy új `sample_play` lecseréli az előzőt.
* Amíg minta szól, a DMA foglalt: a `dma_copy`, a `dma_fill` és a
  `layer2_clear` ilyenkor a CPU-val dolgozik (helyesen, de lassabban: a
  Layer 2 törlése kb. 2 képkocka).

Lásd: `examples/samples.lz80n`.

---

## 18. Megszakítások és a copper

### 18.1 Megszakításkezelők

```lua
word ticks = 0

interrupt frame             -- minden képkockában egyszer (másodpercenként 50-szer)
    ticks += 1
end

interrupt raster(96)        -- amikor a kép a 96. sorhoz ér
    nextreg[0x4A] = 0x1C    -- háttérszín váltása a kép közepén
end

interrupt raster(160)
    nextreg[0x4A] = 0
end
```

* Sorszámozás: 0–191 a kép, 192-től az alsó keret és a függőleges
  visszafutás, a felső keret kb. 280–311 (50 Hz).
* Minden sornak egy kezelője lehet. A sorszám 0–311 közötti konstans.
* A kezelők legyenek **rövidek**. Egy rastersor 28 MHz-en kb. 1800
  órajelciklus.
* Konstans regiszterrel használd a `nextreg[…] = …` formát.
* Kezelőből ne írj szöveget és ne rajzolj a Layer 2-re: a kurzor és a
  memórialeképezés közös a főprogrammal.
* A kezelőből hívott függvényeket a főprogram nem hívhatja (statikus keretek,
  lásd 7.1).

Ha vannak raster-kezelők, a `vsync()` a kép alján (192. sor) tér vissza, ez a
kép frissítésének legjobb pillanata.

### 18.2 Copper

A copper egy kis társprocesszor, amely a megadott rasterpozíciókban
Next-regisztereket ír át, a processzor igénybevétele nélkül.

| Hívás | Leírás |
|-------|--------|
| `copper_begin()` | a copper leállítása és új lista írásának kezdete |
| `copper_wait(line)` | várakozás, amíg a raster eléri a `line` sort |
| `copper_move(reg, value)` | a `reg` Next-regiszter írása (0–127) |
| `copper_start()` | a lista lezárása és futtatása minden képkockában |
| `copper_stop()` | a copper leállítása |

```lua
-- szivárvány háttér: 8 soronként új szín
copper_begin()
for i = 0, 23 do
    copper_wait(word(i) * 8)
    copper_move(0x4A, rgb(byte(i) & 7, byte(i >> 1) & 7, 2))
end
copper_start()
```

A tépődés elkerülésére a listát közvetlenül a `vsync()` után építsd újra. Lásd:
`examples/raster.lz80n`.

---

## 19. Memória, portok, Next-regiszterek és DMA

| Szerkezet | Leírás |
|-----------|--------|
| `memory[addr]` | bájt olvasása / írása 16 bites címen |
| `port[n]` | 16 bites I/O port olvasása / írása |
| `nextreg[n]` | Next-regiszter olvasása / írása |
| `addr(x)` → word | változó, tömb, elem vagy mező címe |
| `dma_copy(src, dst, len)` | memóriamásolás a zxnDMA-val |
| `dma_fill(dst, len, value)` | memória kitöltése a zxnDMA-val |

```lua
nextreg[0x07] = 3                     -- közvetlen NEXTREG utasítás
byte machine = nextreg[0x00]
memory[0x5800] = 0x47
dma_copy(addr(level), addr(backup[0]), 256)
dma_fill(addr(map[0]), 256, 0)
```

A DMA-átvitel sokkal gyorsabb, mint a ciklusos másolás. A processzor megvárja,
amíg az átvitel befejeződik. A címek processzorcímek, tehát az aktuális
memórialeképezés érvényes rájuk.

---

### 19.1 `far` tömbök: adatok memóriabankokban

A programterület (kód és közönséges változók) kb. 31 KB. A nagy adatok a
Next többi memóriájában is lakhatnak:

```lua
far byte level[22][720]                 -- 15840 bájt, a programterületen kívül
far word scores[10000]
far fixed curve[] = {0.5, 1, 1.5, 2}    -- kezdőérték is lehet
far byte map[64][64] = {{1, 1, 1}, {1, 0, 1}}

level[y][x] = 3
level[y][x + 1] += level[y][x]
scores[i] = scores[i - 1] + 10
```

* Elemtípus: `byte`, `sbyte`, `word`, `sword`, `fixed`, `bool`; egy vagy
  több dimenzió; tömbönként legfeljebb 64 KB. Csak a legfelső szinten
  deklarálható.
* Az adatok 8K-s lapokon vannak a 14. 16K-s banktól felfelé, a képekkel
  (`image(...)`) közös területen: együtt kb. 780 KB. A kezdőérték a `.nex`
  fájl része; a kezdőérték nélküli tömböket induláskor nullázza a program.
* Egy elem ugyanúgy olvasható és írható, mint bármely tömbelem. Hozzáféréskor
  a megfelelő lap az MMU 1. slotjába (`$2000`–`$3FFF`) kerül, ezért kicsit
  drágább egy közönséges tömbnél (kb. 60–80 T-ciklus, ha a lap már be van
  lapozva). Megszakításkezelő is használhat `far` tömböt: a lapozást a
  kezelő előtt elmenti, utána visszaállítja.
* `far` elem nem adható át referenciaként és nem használható `addr()`-ral;
  egy teljes sor (`level[y]`) önmagában nem használható.

## 20. Beépített assembly

```lua
function fast_border(byte c)
    asm
        ld a,(l_fast_border_c_1)   ; a paraméter címkéje: lásd az .asm fájlt
        out ($FE),a
    end
end
```

* Az `asm` és `end` közötti sorok változatlanul kerülnek a kimenetbe
  (SjASMPlus szintaxis).
* A fordító az utasítások között nem tart értéket regiszterben, így egy
  `asm` blokk szabadon használhatja az `AF BC DE HL IX IY` regisztereket és az
  árnyékregisztereket.
* Az `SP`-t és a verem egyensúlyát meg kell őriznie, és nem változtathatja
  meg a megszakítási módot és az `I` regisztert.
* Használj lokális címkéket (`.loop`) vagy egyedi neveket.
* A peephole optimalizáló az `asm` blokkokat soha nem módosítja.

Egy változó címkéjét úgy találod meg, hogy `-S` kapcsolóval fordítasz, és
megnézed az `.asm` fájlt. A globálisok neve `g_<név>`, a lokálisoké
`l_<függvény>_<név>_<szám>`.

---

## 21. Gyors kód írása

Az LZ80N natív kódra fordít, így a legtöbb program külön odafigyelés nélkül
is gyors. Ezek a szabályok akkor segítenek, ha minden ciklus számít:

1. **Használd a legkisebb típust.** A `byte` műveletek gyakran feleannyiba
   kerülnek, mint a `word` műveletek.
2. **A konstans szorzó és osztó olcsó.** Az `x * 10` eltolásokká és
   összeadásokká alakul. Az `x / 10` és `x % 1000` (előjel nélküli)
   reciprokkal való szorzás lesz, osztóciklus nélkül. Kettő hatványai
   eltolások és `&` műveletek.
3. **A változóval való osztás a legdrágább alapművelet** (16 lépéses ciklus).
   Belső ciklusban kerüld.
4. **A ±1 lépésközű `for` ciklus a leggyorsabb ciklus.** Egy üres kör `word`
   számlálóval 59 órajelciklus.
5. **Lebegőpontos utánzat helyett `fixed`.** A `fixed + fixed` egyetlen 16
   bites összeadás.
6. **Használd a hardvert**: a `layer2_clear`, `dma_copy` és `dma_fill` DMA-t
   használ; a `hline` `LDIR`-t; a sprite-ok és a copper nem visznek
   processzoridőt.
7. **Belső ciklusban kerüld a hosszú stringeket.** Minden stringértékadás
   másolja a szöveget.
8. **Nézd meg a generált kódot.** Az `lz80nc -S` megírja az `.asm` fájlt,
   benne megjegyzésként a forrássorokkal.

### Sebességmérés

A `benchmarks/` mappában ugyanaz a hét feladat van LZ80N-ben és NextBASIC-ben
(ciklus, aritmetika, prímszita, kiírás, stringek, négyzetgyök,
pixelrajzolás). A `benchmark_int.bas` a ciklust és az aritmetikát NextBASIC
egész változókkal ismétli meg. A részletek és az eredmények a
`benchmarks/README.md` fájlban vannak.

---

## 22. Hogyan fut egy program

### 22.1 Indulás

1. Megszakítások ki, verem beállítása, **processzor 28 MHz-en**.
2. Az ULA képernyő kikapcsol. Rétegsorrend S-U-L. A Layer 2 előkészítése
   (256 × 192).
3. A tilemap szövegréteg és a betűkészlet beállítása.
4. Saját **IM2 megszakításkezelő**: képkockaszámláló, `interrupt` kezelők,
   hangmeghajtó.
5. A program által használt könyvtármodulok inicializálása (hang,
   sprite-ok …).
6. Megszakítások be, majd fut a főprogram.

A program nem használja a ROM-ot és annak rendszerváltozóit. Amikor a
főprogram véget ér, a processzor `EI : HALT` ciklusban várakozik.

### 22.2 Memóriatérkép

```
$0000..$1FFF  MMU 0. slot: Layer 2 rajzolóablak (rajzolás után)
$4000..$5AFF  ULA képernyő (nem használt, törölve)
$6000..$69FF  tilemap (szövegréteg)
$6C00..$6FFF  betűkészlet
$2000..$3FFF  MMU 1. slot: far adat- és képlapok, hozzáféréskor lapozva
$4000..$5FFF  MMU 2. slot: bankolt kódlapok (bankolt függvény futása alatt)
8-10. bank    Layer 2 1. puffer, 11-13. bank: 2. puffer (induláskor beállítva)
14. banktól   képek és far tömbök
$8000..$FCFF  programkód és adatok (fordításkor ellenőrizve)
$FDFC..       verem (lefelé nő)
$FDFD..$FDFF  JP a megszakításkezelőre
$FE00..$FF00  IM2 vektortábla
```

### 22.3 Szabványos modulok

Néhány könyvtár magában LZ80N-ben készült (sprite-ok, Layer 2 alakzatok,
paletták, copper, hang). Ezek csak akkor kerülnek a programba, ha a program
használja őket, így egy nem használt könyvtár nem foglal memóriát.

### 22.4 A fordító

`lexer → parser → AST → típusellenőrzés és Z80N kód → peephole optimalizáló →
SjASMPlus → .nex`.

A tesztkészlet (`cargo test`) kis programokat fordít le, és egy beépített Z80N
emulátorban futtatja őket (órajelciklus-számlálással, raster-időzítéssel, IM2
megszakításokkal, copperrel, DMA-val és AY-regiszterekkel). Ellenőrzi a
kimenetet, a képernyőt és a hardver állapotát.

### 22.5 Kódbankolás

A program nagyobb lehet a 31 KB-os programterületnél. Ha a rezidens rész
nem férne el, a fordító a függvényeket 8K-s **kódlapokra** teszi,
automatikusan, a legnagyobbakkal kezdve. A forrásban semmit nem kell
változtatni.

* A bankolt függvény az MMU 2. slotjában fut (`$4000`–`$5FFF`, ami szabad,
  mert az ULA képernyő ki van kapcsolva). A hívás egy 8 bájtos csonkon
  megy át, ami belapozza a függvény lapját, meghívja, majd visszalapozza a
  hívó lapját, így a bankolt függvények lapokon át is hívhatják egymást,
  adhatnak vissza értéket, és minden más is a szokásos módon működik. Egy
  hívás kb. 150 T-ciklussal drágább.
* A kódlapok a képek és a `far` tömbök után következnek (a 14. 16K-s
  banktól felfelé). A megszakításkezelők rezidensek maradnak; az általuk
  hívott függvények lehetnek bankoltak (a lapozást a kezelő elmenti és
  visszaállítja).
* A főprogram is lehet nagy. Ha még az összes függvénye bankolásával sem
  fér el, a fordító a nagy utasításait (ciklusokat, `if` blokkokat, hosszú
  utasításokat, a legbelsőkkel kezdve) rejtett függvényekbe emeli ki, és
  ezeket ugyanígy bankolja. Ha egy `break` kilép az utasításból, vagy
  `asm` van benne, az utasítás a helyén marad.
* Ami mindig rezidens marad: a globális változók, a stringek és a többi
  adat, a futtatórendszer és a főprogram kis utasításai. Ha ezek önmagukban
  túl nagyok, az assembler jelzi; a nagy táblákat tedd `far` tömbökbe (19.1).
* Az `lz80nc --bank-all game.lz80n` minden függvényt és a főprogram
  nagyobb utasításait kódlapokra teszi, így kis programmal is kipróbálható
  a bankolás (például az `examples/banks.lz80n`-nel).

---

## 23. Korlátok és tervek

Még nem támogatott:

* kb. 31 KB-nál több globális változó és adat a programterületen (a nagy
  táblák helye a `far` tömb, 19.1)
* PT3 zene importálása

Tervezett sorrend: sebességmérés a NextBASIC ellen valódi gépen →
`long`/`fixed16`/Layer 2 módok → konstruktorok és öröklődés → memóriabankok →
mintalejátszás és zeneimport → köztes reprezentáció és regiszterallokáció.

---

## 24. Referenciatáblák

### 24.1 Beépített függvények

| Függvény | Fejezet |
|----------|---------|
| `print`, `write`, `at`, `ink`, `cls`, `border` | 11 |
| `key`, `kempston`, `any_key`, `wait_key` | 12 |
| `vsync`, `pause`, `frames`, `random`, `seed`, `cpu_speed` | 13 |
| `layer2_visible`, `layer2_clear`, `layer2_flip`, `plot`, `point`, `hline`, `vline`, `line`, `rect`, `fill_rect`, `circle`, `fill_circle`, `ellipse`, `fill_ellipse`, `triangle`, `fill_triangle`, `rgb` | 14 |
| `draw_image`, `image_width`, `image_height` | 14, 16 |
| `palette`, `palette9`, `rgb9` | 15 |
| `sprite_load`, `sprites_visible`, `sprite[n]` | 16 |
| `beep`, `tone`, `noise`, `volume`, `sound_off`, `sound_reset`, `play`, `play_loop`, `playing`, `note_period`, `ay_write` | 17 |
| `copper_begin`, `copper_wait`, `copper_move`, `copper_start`, `copper_stop` | 18 |
| `memory[]`, `port[]`, `nextreg[]`, `addr`, `dma_copy`, `dma_fill`, `far` tömbök | 19 |
| `len`, `sub`, `find`, `val`, `str`, `asc`, `chr`, `upper`, `lower` | 9 |
| `abs`, `min`, `max`, `clamp`, `sqrt`, `sin`, `cos`, `atan2`, `round` | 10 |

### 24.2 Beépített konstansok

| Konstansok | Értékek |
|------------|---------|
| `BLACK BLUE RED MAGENTA GREEN CYAN YELLOW WHITE` | 0–7 |
| `BRIGHT_BLACK` … `BRIGHT_WHITE` | 8–15 |
| `KEY_A` … `KEY_Z`, `KEY_0` … `KEY_9`, `KEY_SPACE`, `KEY_ENTER`, `KEY_CAPS`, `KEY_SYM` | billentyűkódok |
| `JOY_RIGHT JOY_LEFT JOY_DOWN JOY_UP JOY_FIRE` | Kempston bitek |
| `MHZ_3 MHZ_7 MHZ_14 MHZ_28` | 0–3 |
| `NOT_FOUND` | 255 |
| `TRANSPARENT` | `$E3` |
| `PAL_ULA PAL_LAYER2 PAL_SPRITES PAL_TEXT` | palettaválasztók (első paletták) |
| `PAL_ULA_2 PAL_LAYER2_2 PAL_SPRITES_2 PAL_TEXT_2` | második paletták |
| `L2_256 L2_320 L2_640` | Layer 2 módok |

### 24.3 Példaprogramok

| Fájl | Mit mutat |
|------|-----------|
| `hello.lz80n` | számok, függvények, `for`, színek |
| `move.lz80n` | billentyűzet és Kempston joystick, `at` |
| `classes.lz80n` | struktúrák, osztályok, metódusok, objektumtömbök |
| `inherit.lz80n` | konstruktorok, öröklődés, struktúra visszaadása, struktúralisták |
| `text.lz80n` | a szövegréteg, 16 szín |
| `layer2.lz80n` | Layer 2 rajzolás, rétegezés szöveggel és sprite-okkal |
| `math.lz80n` | `fixed`, `sin`/`cos`/`atan2`/`sqrt`, dupla pufferelés |
| `palette.lz80n` | színciklus, `dma_copy`, `addr` |
| `hires.lz80n` | Layer 2 320 × 256 és 640 × 256, második paletták |
| `raster.lz80n` | copper, raster- és képkocka-megszakítás, `random` |
| `strings.lz80n` | stringek és stringfüggvények, futószöveg |
| `tables.lz80n` | tömb-kezdőértékek: menü, oszlopdiagram és színtáblák, `/=` `%=` |
| `life.lz80n` | életjáték kétdimenziós tömbökkel, 2D kezdőértékek |
| `shapes.lz80n` | körök, ellipszisek, vágás, dupla pufferelés |
| `images.lz80n` | PNG képek a Layer 2-n, átlátszóság |
| `banks.lz80n` | `far` tömbök: 16 KB-os barlangtérkép memóriabankokban |
| `bignum.lz80n` | `long` és `fixed16`: nagy pontszámok, 12!, ágyúgolyó pályája |
| `recursion.lz80n` | rekurzió: fraktálfa, Hanoi tornyai, kölcsönös rekurzió |
| `modules.lz80n` | program három fájlban: `include`, `lib/screen.lz80n`, `lib/ball.lz80n` |
| `shooter.lz80n` | kis űrlövölde: képből kivágott játékos, ötféle 16x16-os pixel art ellenség, csillagok, hang |
| `sound.lz80n` | háttérzene, `beep`, hangeffektek |
| `samples.lz80n` | `.wav` minták a DAC-on, a háttérben |
| `sprites.lz80n` | hardveres sprite-ok, PNG erőforrások |
| `benchmarks/benchmark.lz80n` | sebességmérés |
