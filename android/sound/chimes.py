"""The app's two notification chimes, written to res/raw: a rising pair of notes for
something that needs the owner (a permission request, a question) and a falling pair for
something that finished (a turn, a session). Run from the repo root:
python3 android/sound/chimes.py"""

import math
import struct
import wave

RATE = 22050
OUT = "android/app/src/main/res/raw"


def note(freq, length, gain=0.35):
    """A soft bell-like note: the tone and its octave, quick attack, exponential decay."""
    n = int(RATE * length)
    out = []
    for i in range(n):
        t = i / RATE
        env = min(1.0, t / 0.008) * math.exp(-t * 6.0)
        s = math.sin(2 * math.pi * freq * t) + 0.3 * math.sin(4 * math.pi * freq * t)
        out.append(gain * env * s / 1.3)
    return out


def chime(freqs, gap=0.14, length=0.55):
    total = int(RATE * (gap * (len(freqs) - 1) + length))
    mix = [0.0] * total
    for k, f in enumerate(freqs):
        start = int(RATE * gap * k)
        for i, s in enumerate(note(f, length)):
            mix[start + i] += s
    return mix


def write(name, samples):
    with wave.open(f"{OUT}/{name}.wav", "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in samples))


write("chime_needs_you", chime([659.25, 987.77]))  # E5 then B5
write("chime_finished", chime([987.77, 659.25]))  # B5 then E5
