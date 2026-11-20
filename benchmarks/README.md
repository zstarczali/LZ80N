# LZ80N vs NextBASIC benchmark

The same seven tasks in both languages, both at 28 MHz, timed with the
50 Hz frame counter:

| Task | Work |
|------|------|
| loop | empty `for` loop, 10 000 iterations |
| arith | `s = (s + i * 7) % 1000`, 5 000 iterations |
| sieve | primes below 2000 (sieve of Eratosthenes) |
| print | `"LZ80N speed test"` at a fixed position, 500 times |
| string | build a 50-character string one character at a time, 20 times |
| sqrt | sum of `floor(sqrt(i))`, i = 1 .. 1000 |
| plot | 128 × 64 Layer 2 pixels |

Both programs print a check line at the end:
`arith 500  primes 303  sqrt 20615` (NextBASIC: `sqrt 20607`, see the
notes below).

## Running

* **LZ80N**: `lz80nc benchmarks/benchmark.lz80n`, run `benchmark.nex`.
  Each task runs 10 times and the time for one run is shown, so the
  resolution is 2 ms.
* **NextBASIC**: `benchmark.bas` is a plain-text listing. Tokenise it with
  a txt2bas converter, or type it in, then `RUN`. Each task runs once, so
  the resolution is 20 ms.

## LZ80N results (test emulator)

From the compiler's test emulator (`cargo test --release benchmark --
--nocapture`): Z80N instruction timings at 28 MHz, with **no memory wait
states**. Real hardware will be somewhat slower; the times the program
shows on the Next are the reference.

| Task | first version | now | per item |
|------|-------------:|----:|---------:|
| loop | 44 ms | 21 ms | 59 T-states per iteration |
| arith | 346 ms | 115 ms | 645 T per iteration |
| sieve | 57 ms | 41 ms | |
| print | 95 ms | 49 ms | ~170 T per character incl. `at` |
| string | 121 ms | 28 ms | appended in place |
| sqrt | 103 ms | 34 ms | 16-bit root in 8 rounds |
| plot | 99 ms | 86 ms | 295 T per pixel incl. both loops |

## LZ80N vs NextBASIC (CSpect)

Measured on 2026-11-20 in CSpect 3.1 with NextZXOS, 50 Hz, both at 28 MHz.
NextBASIC: `benchmark.bas` (floating point) and `benchmark_int.bas`
(integer variables `%a`..`%z`, the fastest NextBASIC form, for `loop` and
`arith`).

| Task | NextBASIC | NextBASIC integer | LZ80N | LZ80N is faster |
|------|----------:|------------------:|------:|----------------:|
| loop | 7 240 ms | 4 460 ms | 34 ms | 213x (integer: 131x) |
| arith | 12 880 ms | 4 460 ms | 176 ms | 73x (integer: 25x) |
| sieve | 11 860 ms | | 64 ms | 185x |
| print | 1 480 ms | | 74 ms | 20x |
| string | 3 200 ms | | 42 ms | 76x |
| sqrt | 20 020 ms | | 46 ms | 435x |
| plot | 12 340 ms | | 138 ms | 89x |

CSpect runs LZ80N code about 1.4-1.7x slower than the test emulator above
(probably the Next's memory wait states at 28 MHz). `print` is the closest, probably because
most of the NextBASIC time is in the ROM's printing, which is machine code
too.

For scale: one iteration of the LZ80N empty loop is 59 T-states, about
2 µs at 28 MHz. An interpreter has to fetch and decode the tokens of
`LET %i=%i+1: REPEAT UNTIL %i=10000` and look the variable up on every
iteration, which takes far more than a handful of instructions.

## Notes on fairness

* `benchmark.bas` uses 5-byte floating point variables;
  `benchmark_int.bas` repeats `loop` and `arith` with 16-bit integer
  variables and `REPEAT ... REPEAT UNTIL` (integer variables cannot be
  `FOR` loop counters).
* `sqrt`: NextBASIC computes `INT SQR i` in floating point; LZ80N uses an
  integer square root. The exact sum is 20615 (LZ80N); NextBASIC prints
  20607, because `SQR` of some perfect squares comes out just below the
  integer and `INT` rounds it down.
* `print`: NextBASIC prints to the ULA screen, LZ80N to the tilemap.
  Both write 16 characters at a fixed position 500 times.

## Compiler improvements found by this benchmark

* `for` with step ±1: increment and test in one pass against `end + step`
  (constant end: two `cp`s); no entry check when both bounds are constant.
* Multiplication by a constant: shifts and adds.
* Division and `%` by a constant (unsigned): multiplication by a magic
  reciprocal (`rt_mulhi16`, four `MUL D,E`) and a barrel shift, no loop.
  The multiplier is chosen by the compiler and checked against every
  16-bit dividend. Powers of two use shifts and `and`.
* 16-bit division by a variable: faster loop; the high half is skipped
  for dividends below 256.
* Text output: `print`/`write` of a string writes straight into the
  tilemap instead of one `rt_putc` call per character.
* Stores of constants / plain variables into array elements no longer go
  through the stack; `plot`/`point` load simple arguments directly.
* Integer square root skips the zero top byte of 16-bit values; later a
  separate 16-bit root (bit pairs, 8 rounds).
* `s = s .. x` appends to `s` itself instead of building the result in a
  temporary string and copying it back.

The per-operation measurements (`micro.lz80n`) and what was changed for
them are in `docs/performance-review.md`, section 6.
