"""A small original tune as a ProTracker 3.5 module (.pt3), for
examples/music.lz80n. Run from the repository root:
    python assets/make_music.py

Channel A: lead melody, B: arpeggio chords (ornaments), C: bass and drums.
It uses samples, ornaments, a tone portamento and a buzzer envelope, so it
also exercises the compiler's PT3 importer.
"""
import struct

TEMPO = 4          # frames per line
LINES = 32         # lines per pattern

NOTES = {'C': 0, 'C#': 1, 'D': 2, 'D#': 3, 'E': 4, 'F': 5, 'F#': 6, 'G': 7, 'G#': 8, 'A': 9, 'A#': 10, 'B': 11}


def n(name):
    """'A-4' -> PT3 note number (0 = C-1)"""
    key, octv = name.rsplit('-', 1)
    return (int(octv) - 1) * 12 + NOTES[key]


def line(level, tone=0, tone_on=True, noise_on=False, noise=0, env_ok=False, slide=0):
    b0 = (noise & 31) << 1 | (0 if env_ok else 1)
    if slide:
        b0 |= 0x80 | (0x40 if slide > 0 else 0)
    b1 = (level & 15) | (0 if noise_on else 0x80) | (0 if tone_on else 0x10)
    return bytes([b0, b1]) + struct.pack('<h', tone)


SAMPLES = {
    # lead: a soft attack, then a little vibrato (loops on the last 4 lines)
    1: (6, [line(11), line(14), line(13), line(12), line(11), line(11),
            line(11, 1), line(11, 2), line(11, 1), line(11, 0)]),
    # chords: short and quiet, the ornament does the arpeggio
    2: (5, [line(10), line(9), line(8), line(8), line(7), line(7)]),
    # bass: the envelope may be switched on (buzzer)
    3: (4, [line(15, env_ok=True), line(14, env_ok=True), line(13, env_ok=True), line(12, env_ok=True),
            line(12, env_ok=True)]),
    # kick: a falling tone with a little noise
    4: (7, [line(15, -40, noise_on=True, noise=2), line(14, 40), line(12, 120), line(10, 240),
            line(7, 400), line(4, 600), line(2, 800), line(0, tone_on=False)]),
    # snare / hi-hat: noise only
    5: (6, [line(14, tone_on=False, noise_on=True, noise=4), line(11, tone_on=False, noise_on=True, noise=5),
            line(8, tone_on=False, noise_on=True, noise=6), line(5, tone_on=False, noise_on=True, noise=7),
            line(2, tone_on=False, noise_on=True, noise=8), line(1, tone_on=False, noise_on=True, noise=8),
            line(0, tone_on=False)]),
}
ORNAMENTS = {1: (0, [0, 0, 4, 4, 7, 7]),     # major
             2: (0, [0, 0, 3, 3, 7, 7])}     # minor

# chord roots of the progression Am F C G, eight lines each
CHORDS = [('A-3', 2), ('F-3', 1), ('C-4', 1), ('G-3', 1)]


def cell(note=None, sample=None, orn=None, vol=None, rest=False, env=None, fx=None):
    return dict(note=note, sample=sample, orn=orn, vol=vol, rest=rest, env=env, fx=fx)


def lead(melody):
    """melody: list of (line, note or 'r', [fx])"""
    ch = [None] * LINES
    for item in melody:
        ln, nt = item[0], item[1]
        if nt == 'r':
            ch[ln] = cell(rest=True)
        else:
            ch[ln] = cell(n(nt), sample=1, vol=13, fx=item[2] if len(item) > 2 else None)
    return ch


def chords():
    ch = [None] * LINES
    for k, (root, orn) in enumerate(CHORDS):
        for j in (0, 4):
            ch[k * 8 + j] = cell(n(root), sample=2, orn=orn, vol=11)
    return ch


def rhythm(buzz=False):
    ch = [None] * LINES
    for k, (root, _) in enumerate(CHORDS):
        base = k * 8
        bass = n(root) - 12
        ch[base] = cell(bass - 12, sample=4, vol=15)               # kick
        ch[base + 2] = cell(bass, sample=3, vol=15,
                            env=(10, 24) if buzz else 'off')       # bass (buzzer: envelope 10)
        ch[base + 4] = cell(n('C-5'), sample=5, vol=12, env='off')  # snare
        ch[base + 6] = cell(bass + 12, sample=3, vol=14)
    return ch


PATTERNS = [
    # 0: intro, no melody
    [[None] * LINES, chords(), rhythm()],
    # 1: theme
    [lead([(0, 'E-5'), (4, 'C-5'), (6, 'D-5'), (8, 'C-5'), (12, 'A-4'), (14, 'C-5'),
           (16, 'G-4'), (20, 'C-5'), (22, 'E-5'), (24, 'D-5'), (28, 'B-4'), (30, 'r')]), chords(), rhythm()],
    # 2: answer, with a slide up to the last note
    [lead([(0, 'A-5'), (4, 'G-5'), (6, 'E-5'), (8, 'F-5'), (12, 'E-5'), (14, 'C-5'),
           (16, 'E-5'), (20, 'G-5'), (24, 'D-5'), (27, 'G-5', ('port', 2, 12)), (30, 'r')]), chords(), rhythm(True)],
]
POSITIONS = [0, 1, 2, 1, 2]
LOOP = 1


def channel_bytes(cells):
    out = bytearray()
    state = dict(sample=None, orn=0, vol=None, skip=1, env=None)
    k = 0
    while k < LINES:
        c = cells[k] or {}
        run = 1
        while k + run < LINES and cells[k + run] is None:
            run += 1
        if run != state['skip']:
            out += bytes([0xB1, run])
            state['skip'] = run
        env = c.get('env')
        fx = c.get('fx')
        if fx:
            out.append(2)                                  # effect 2: tone portamento
        if isinstance(env, tuple):
            ty, per = env
            out += bytes([0x10 + ty]) + struct.pack('>H', per) + bytes([c['sample'] * 2])
            state['sample'] = c['sample']
            state['env'] = env
        elif env == 'off' and state['env'] is not None:
            out.append(0xB0)
            state['env'] = None
        if c.get('sample') is not None and c['sample'] != state['sample']:
            out.append(0xD0 + c['sample'])
            state['sample'] = c['sample']
        if c.get('orn') is not None and c['orn'] != state['orn']:
            out.append(0x40 + c['orn'])
            state['orn'] = c['orn']
        if c.get('vol') is not None and c['vol'] != state['vol']:
            out.append(0xC0 + c['vol'])
            state['vol'] = c['vol']
        if c.get('rest'):
            out.append(0xC0)
        elif c.get('note') is not None:
            out.append(0x50 + c['note'])
        else:
            out.append(0xD0)
        if fx:
            _, delay, step = fx
            out += bytes([delay]) + struct.pack('<h', 0) + struct.pack('<h', step)
        k += run
    out.append(0)
    return bytes(out)


def build():
    head = bytearray(201)
    head[0:14] = b'ProTracker 3.5'
    head[14:30] = b' compilation of '
    head[30:62] = b'LZ80N demo tune'.ljust(32)
    head[62:66] = b' by '
    head[66:98] = b'LZ80N examples'.ljust(32)
    head[98] = 0x20
    head[99] = 0                       # ProTracker frequency table
    head[100] = TEMPO
    head[101] = len(POSITIONS)
    head[102] = LOOP
    body = bytearray(bytes(p * 3 for p in POSITIONS) + b'\xff')
    pos = len(head) + len(body)

    pattern_table_at = pos
    pos += 6 * len(PATTERNS)
    chan_data, chan_offsets = bytearray(), []
    for pat in PATTERNS:
        offs = []
        for cells in pat:
            offs.append(pos + len(chan_data))
            chan_data += channel_bytes(cells)
        chan_offsets.append(offs)
    pos += len(chan_data)

    blobs = bytearray()
    samp_offs = [0] * 32
    for i, (lp, lines) in SAMPLES.items():
        samp_offs[i] = pos + len(blobs)
        blobs += bytes([lp, len(lines)]) + b''.join(lines)
    orn_offs = [0] * 16
    orn_offs[0] = pos + len(blobs)     # ornament 0: no change
    blobs += bytes([0, 1, 0])
    for i, (lp, offs) in ORNAMENTS.items():
        orn_offs[i] = pos + len(blobs)
        blobs += bytes([lp, len(offs)]) + bytes(o & 0xFF for o in offs)

    struct.pack_into('<H', head, 103, pattern_table_at)
    struct.pack_into('<32H', head, 105, *samp_offs)
    struct.pack_into('<16H', head, 169, *orn_offs)
    table = b''.join(struct.pack('<3H', *o) for o in chan_offsets)
    return bytes(head + body + table + chan_data + blobs)


if __name__ == '__main__':
    data = build()
    with open('examples/assets/tune.pt3', 'wb') as f:
        f.write(data)
    print(f'examples/assets/tune.pt3: {len(data)} bytes')
