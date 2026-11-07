"""Add offline Czech stop-name clips to the existing Supertonic announcement bank."""
import argparse
import hashlib
import io
from pathlib import Path
import wave

import numpy as np
from supertonic import TTS
from build_demo_audio import ROOT, wav


PHRASES = (
    "Příští zastávka.", "Zastávka na znamení.", "Konečná zastávka, prosíme vystupte.",
    "Přestup na metro A.", "Přestup na metro B.", "Přestup na metro C.",
    "Přestup na linky S a další vlakové spoje.",
)


def pack(tts, voice, name):
    clip = wav(tts, voice, name.rstrip(".") + ".", "part")
    with wave.open(io.BytesIO(clip)) as audio:
        signal = np.frombuffer(audio.readframes(audio.getnframes()), dtype="<i2").astype(np.float32) / 32768
    companded = np.sign(signal) * np.log1p(255.0 * np.abs(signal)) / np.log(256.0)
    return (np.rint(companded * 127.0) + 128).astype(np.uint8).tobytes()


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--model", type=Path)
    cli.add_argument("--cache", type=Path, required=True)
    args = cli.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    tts = TTS(model="supertonic-3", model_dir=args.model, auto_download=args.model is None,
              intra_op_num_threads=4, inter_op_num_threads=1)
    voice = tts.get_voice_style("M1")
    data = ROOT / "data"
    names = set(PHRASES)
    for row in (data / "prague.tsv").read_text(encoding="utf-8").splitlines():
        fields = row.split("\t")
        if fields[0] == "stop":
            names.add(fields[2])
    audio = data / "demo-audio.wavpack"
    index = data / "demo-audio.tsv"
    temp_audio = data / "city-audio.tmp"
    temp_index = data / "city-index.tmp"
    old_bank = audio.read_bytes()
    with temp_audio.open("wb") as target, temp_index.open("w", encoding="utf-8", newline="\n") as manifest:
        for row in index.read_text(encoding="utf-8").splitlines():
            event, phrase, offset, length = row.split("\t")
            if event != "part":
                offset, length = int(offset), int(length)
                manifest.write(f"{event}\t{phrase}\t{target.tell()}\t{length}\n")
                target.write(old_bank[offset:offset+length])
        for i, name in enumerate(sorted(names), 1):
            cached = args.cache / (hashlib.sha256(name.encode()).hexdigest() + ".ulaw")
            if not cached.exists():
                cached.write_bytes(pack(tts, voice, name))
            clip = cached.read_bytes()
            manifest.write(f"part\t{name}\t{target.tell()}\t{len(clip)}\n")
            target.write(clip)
            if i % 100 == 0 or i == len(names):
                print(f"{i}/{len(names)}", flush=True)
    temp_audio.replace(audio)
    temp_index.replace(index)


if __name__ == "__main__":
    main()
