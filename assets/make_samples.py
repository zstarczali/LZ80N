"""Sound samples for examples/samples.lz80n: 8-bit mono .wav at 15625 Hz
(the DMA rate 875 kHz / 56). Run from the repository root:
    python assets/make_samples.py
"""
import math
import random
import struct

RATE = 15625


def save(name, values):
    data = bytes(max(0, min(255, int(round(v * 127 + 128)))) for v in values)
    with open(f"examples/assets/{name}.wav", "wb") as f:
        f.write(b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt ")
        f.write(struct.pack("<IHHIIHH", 16, 1, 1, RATE, RATE, 1, 8))
        f.write(b"data" + struct.pack("<I", len(data)) + data)


def boom():
    random.seed(7)
    n = int(RATE * 0.7)
    out, low = [], 0.0
    for i in range(n):
        t = i / n
        low += (random.uniform(-1, 1) - low) * (0.35 - 0.3 * t)   # darker as it fades
        out.append(low * 2.2 * (1 - t) ** 2)
    return out


def zap():
    n = int(RATE * 0.25)
    out, ph = [], 0.0
    for i in range(n):
        t = i / n
        ph += (2200 - 1900 * t) / RATE
        out.append((1 if ph % 1 < 0.5 else -1) * 0.6 * (1 - t))
    return out


def hum():
    # exactly 8 periods of 97.66 Hz: loops without a click
    n = 1280
    out = []
    for i in range(n):
        a = 2 * math.pi * 8 * i / n
        out.append(0.45 * math.sin(a) + 0.2 * math.sin(3 * a) + 0.1 * math.sin(5 * a))
    return out


save("boom", boom())
save("zap", zap())
save("hum", hum())
print("examples/assets/boom.wav, zap.wav, hum.wav")
