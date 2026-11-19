// Reference data for hovers, completion and signature help.
// Source: docs/manual-en.md (chapters 9-19, 24). Keep in sync with the manual.
'use strict';

/** name -> [signature, description, chapter] */
const FUNCTIONS = {
  // 11 text output
  print: ['print(a, b, …)', 'Print values separated by spaces, then a new line.', 11],
  write: ['write(a, b, …)', 'Print values without separators and without a new line.', 11],
  at: ['at(row, col)', 'Set the print position (row 0–31, column 0–39).', 11],
  ink: ['ink(c)', 'Text colour 0–15.', 11],
  cls: ['cls()', 'Clear the text layer, cursor to 0,0.', 11],
  border: ['border(c)', 'Set the background colour behind every layer (Next register $4A).', 11],
  // 12 input
  key: ['key(KEY_x) -> bool', 'Is the key held down.', 12],
  kempston: ['kempston() -> byte', 'Kempston joystick bits (`JOY_RIGHT`, `JOY_LEFT`, `JOY_DOWN`, `JOY_UP`, `JOY_FIRE`).', 12],
  any_key: ['any_key() -> bool', 'Is any key held down.', 12],
  wait_key: ['wait_key()', 'Wait until all keys are released, then wait for a key press.', 12],
  // 13 time, random, cpu
  vsync: ['vsync()', 'Wait for the next frame (50 per second).', 13],
  pause: ['pause(n)', 'Wait `n` frames.', 13],
  frames: ['frames() -> word', 'Frame counter since the program started.', 13],
  random: ['random(n) -> word', 'Random number 0 … n-1; `random(0)` gives a full 16-bit value.', 13],
  seed: ['seed(w)', 'Start the random sequence from `w` (same seed → same sequence).', 13],
  cpu_speed: ['cpu_speed(s)', '`MHZ_3`, `MHZ_7`, `MHZ_14`, `MHZ_28` (the program starts at 28 MHz).', 13],
  // 14 layer 2
  layer2_visible: ['layer2_visible(on)', 'Show / hide Layer 2.', 14],
  layer2_clear: ['layer2_clear(c)', 'Fill the screen with colour `c` (zxnDMA, very fast).', 14],
  layer2_flip: ['layer2_flip()', 'Double buffering: show the image drawn so far, draw into the other buffer.', 14],
  plot: ['plot(x, y, c)', 'Draw a pixel.', 14],
  point: ['point(x, y) -> byte', 'Read a pixel.', 14],
  hline: ['hline(x, y, len, c)', 'Horizontal line.', 14],
  vline: ['vline(x, y, len, c)', 'Vertical line.', 14],
  line: ['line(x0, y0, x1, y1, c)', 'Line between two points.', 14],
  rect: ['rect(x, y, w, h, c)', 'Rectangle outline.', 14],
  fill_rect: ['fill_rect(x, y, w, h, c)', 'Filled rectangle.', 14],
  rgb: ['rgb(r, g, b) -> byte', 'Build a colour: r, g 0–7, b 0–3.', 14],
  circle: ['circle(x, y, r, c)', 'Circle outline.', 14],
  fill_circle: ['fill_circle(x, y, r, c)', 'Filled circle.', 14],
  ellipse: ['ellipse(x, y, rx, ry, c)', 'Ellipse outline, radii `rx` and `ry`.', 14],
  fill_ellipse: ['fill_ellipse(x, y, rx, ry, c)', 'Filled ellipse.', 14],
  triangle: ['triangle(x0, y0, x1, y1, x2, y2, c)', 'Triangle outline.', 14],
  fill_triangle: ['fill_triangle(x0, y0, x1, y1, x2, y2, c)', 'Filled triangle (corners in any order).', 14],
  draw_image: ['draw_image(img, x, y)', 'Draw a picture declared with `asset img = image("file.png")`.', 16],
  image_width: ['image_width(img)', 'Width of a picture asset (a compile-time constant).', 16],
  image_height: ['image_height(img)', 'Height of a picture asset (a compile-time constant).', 16],
  image: ['image("file.png")', 'In an `asset` declaration: convert a PNG into a Layer 2 picture at compile time.', 16],
  // 15 palette
  palette: ['palette(layer, index, colour)', '8-bit `RRRGGGBB` colour. `layer`: `PAL_ULA`, `PAL_LAYER2`, `PAL_SPRITES`, `PAL_TEXT`.', 15],
  palette9: ['palette9(layer, index, colour)', '9-bit `RRRGGGBBB` colour.', 15],
  rgb9: ['rgb9(r, g, b) -> word', '9-bit colour: r, g, b 0–7.', 15],
  // 16 sprites
  sprite_load: ['sprite_load(first, asset)', 'Upload the sprite patterns of a PNG asset, starting at pattern slot `first`.', 16],
  sprites_visible: ['sprites_visible(on, over_border)', 'Turn the sprite layer on/off, optionally over the border.', 16],
  // 17 sound
  beep: ['beep(ch, note, frames)', 'Play a note for a number of frames, in the background.', 17],
  tone: ['tone(ch, period)', 'Square wave; frequency = 109375 / period Hz (period 1–4095).', 17],
  noise: ['noise(ch, period)', 'Noise, period 0–31 (one noise generator per chip).', 17],
  volume: ['volume(ch, v)', 'Volume 0–15.', 17],
  sound_off: ['sound_off(ch)', 'Silence a channel and stop its tune.', 17],
  sound_reset: ['sound_reset()', 'Silence everything.', 17],
  sound_mix: ['sound_mix(ch, tone_on, noise_on)', 'Turn the tone and noise of a channel on or off in the AY mixer.', 17],
  sound_tick: ['sound_tick()', 'Advance the music driver by one frame (called from the frame interrupt).', 17],
  play: ['play(ch, "music")', 'Play a PLAY-style tune once, in the background (e.g. `"o4 l8 c e g >c"`).', 17],
  play_loop: ['play_loop(ch, "music")', 'Play a PLAY-style tune in a loop, in the background.', 17],
  playing: ['playing(ch) -> bool', 'Is a beep or tune still playing.', 17],
  note_period: ['note_period(n) -> word', 'AY period of note `n`.', 17],
  ay_write: ['ay_write(chip, reg, value)', 'Write an AY register directly.', 17],
  // 18 copper
  copper_begin: ['copper_begin()', 'Stop the copper and start writing a new list.', 18],
  copper_wait: ['copper_wait(line)', 'Wait until the raster reaches `line`.', 18],
  copper_move: ['copper_move(reg, value)', 'Write Next register `reg` (0–127).', 18],
  copper_start: ['copper_start()', 'Finish the list and run it every frame.', 18],
  copper_stop: ['copper_stop()', 'Stop the copper.', 18],
  // 19 memory
  addr: ['addr(x) -> word', 'Address of a variable, array, element or field.', 19],
  dma_copy: ['dma_copy(src, dst, len)', 'Copy memory with the zxnDMA.', 19],
  dma_fill: ['dma_fill(dst, len, value)', 'Fill memory with the zxnDMA.', 19],
  // 9 strings
  len: ['len(s) -> byte', 'Length of a string.', 9],
  sub: ['sub(s, from, count) -> string', 'Part of a string.', 9],
  find: ['find(s, t) -> byte', 'Position of `t` in `s`, or `NOT_FOUND`.', 9],
  val: ['val(s) -> sword', 'Number from a string.', 9],
  str: ['str(x) -> string', 'Number as a string.', 9],
  asc: ['asc(s, i) -> byte', 'Character code at position `i`.', 9],
  chr: ['chr(c) -> string', 'One-character string.', 9],
  upper: ['upper(s) -> string', 'Upper-case copy.', 9],
  lower: ['lower(s) -> string', 'Lower-case copy.', 9],
  // 10 math
  abs: ['abs(x)', 'Absolute value (any number type).', 10],
  min: ['min(a, b)', 'Smaller of two numbers.', 10],
  max: ['max(a, b)', 'Larger of two numbers.', 10],
  clamp: ['clamp(x, lo, hi)', 'Limit `x` to `lo … hi`.', 10],
  sqrt: ['sqrt(x)', 'Square root: integer → `byte`, `fixed` → `fixed`.', 10],
  sin: ['sin(a) -> fixed', 'Sine of a `byte` angle (256 = full circle).', 10],
  cos: ['cos(a) -> fixed', 'Cosine of a `byte` angle (256 = full circle).', 10],
  atan2: ['atan2(y, x) -> byte', 'Angle of a vector (`sword` or `fixed` arguments) as a `byte` angle.', 10],
  round: ['round(f) -> sword', 'Round a `fixed` value to an integer.', 10],
};

/** hardware arrays / objects */
const ARRAYS = {
  memory: ['memory[address]', 'Read or write a byte of memory.', 19],
  nextreg: ['nextreg[reg]', 'Read or write a Next register.', 19],
  port: ['port[port]', 'Read or write an I/O port.', 19],
  sprite: ['sprite[n]', 'Hardware sprite 0–127 (`Sprite` object): `x y pattern mirror_x scale visible`, `position(x, y)`, `show(pattern)`, `hide()`, `update()`.', 16],
};

const TYPES = {
  bool: 'Boolean, 1 byte: `true` / `false`.',
  byte: 'Unsigned 8-bit integer, 0 … 255.',
  sbyte: 'Signed 8-bit integer, -128 … 127.',
  word: 'Unsigned 16-bit integer, 0 … 65535.',
  sword: 'Signed 16-bit integer, -32768 … 32767.',
  fixed: '8.8 fixed point number, -128 … 127.996 (literals like `1.25`).',
  string: 'String value, up to 64 characters (65 bytes; literals and constants up to 255). By-value copy, `..` concatenates.',
  Sprite: 'Standard library class of a hardware sprite (`sprite[n]`).',
};

const KEYWORDS = [
  'and', 'or', 'not', 'if', 'then', 'elseif', 'else', 'end', 'while', 'do', 'for',
  'function', 'return', 'break', 'const', 'var', 'true', 'false', 'asm', 'struct',
  'class', 'asset', 'interrupt', 'far', 'self', 'raster', 'frame',
];

const COLOURS = ['BLACK', 'BLUE', 'RED', 'MAGENTA', 'GREEN', 'CYAN', 'YELLOW', 'WHITE'];
const CONSTANTS = {};
COLOURS.forEach((c, i) => {
  CONSTANTS[c] = `Colour ${i}.`;
  CONSTANTS['BRIGHT_' + c] = `Colour ${i + 8}.`;
});
for (const c of 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789') CONSTANTS['KEY_' + c] = 'Key code for `key()`.';
for (const k of ['SPACE', 'ENTER', 'CAPS', 'SYM']) CONSTANTS['KEY_' + k] = 'Key code for `key()`.';
for (const j of ['RIGHT', 'LEFT', 'DOWN', 'UP', 'FIRE']) CONSTANTS['JOY_' + j] = 'Kempston bit, test with `kempston() & JOY_' + j + '`.';
[3, 7, 14, 28].forEach((m, i) => (CONSTANTS['MHZ_' + m] = `CPU speed ${m} MHz (= ${i}) for \`cpu_speed()\`.`));
CONSTANTS.NOT_FOUND = '255, returned by `find()`.';
CONSTANTS.TRANSPARENT = '`$E3`, the transparent colour of Layer 2 and sprites.';
for (const p of ['ULA', 'LAYER2', 'SPRITES', 'TEXT']) CONSTANTS['PAL_' + p] = 'Palette selector for `palette()` / `palette9()`.';

module.exports = { FUNCTIONS, ARRAYS, TYPES, KEYWORDS, CONSTANTS };
