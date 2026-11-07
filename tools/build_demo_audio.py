"""Render Czech announcements with Supertonic 3 and an original two-tone chime."""
import argparse
import hashlib
import io
from pathlib import Path
import wave

import numpy as np
from supertonic import TTS


ROOT = Path(__file__).resolve().parents[1]
DOOR = "Ukončete, prosím, nástup a výstup. Dveře se zavírají."


def trim_speech(signal, rate):
    step = rate // 100
    blocks = signal[:len(signal) // step * step].reshape(-1, step)
    rms = np.sqrt(np.mean(blocks ** 2, axis=1))
    active = np.flatnonzero(rms > max(0.004, rms.max() * 0.035))
    if len(active) == 0:
        raise ValueError("Synthesizer returned silent speech")
    return signal[max(0, (active[0] - 5) * step):(active[-1] + 11) * step].copy()


def wav(tts, voice, phrase, event):
    spoken = (phrase.replace("I. P. Pavlova", "Í Pé Pavlova")
             .replace("Slavia - Nádraží Eden", "Slavia, nádraží Eden"))
    audio, duration = tts.synthesize(spoken, voice_style=voice, lang="cs", total_steps=5, speed=1.03)
    signal = audio.reshape(-1)[:round(tts.sample_rate * float(duration[0]))]
    rate = tts.sample_rate // 2
    signal = signal[:len(signal) // 2 * 2].reshape(-1, 2).mean(axis=1)
    speech = trim_speech(signal, rate)
    speech -= np.mean(speech)
    peak = float(np.abs(speech).max())
    speech *= min(0.83 / max(peak, 0.001), 0.122 / max(float(np.sqrt(np.mean(speech ** 2))), 0.001))
    fade = np.linspace(0, 1, rate // 150, dtype=np.float32)
    speech[:len(fade)] *= fade
    speech[-len(fade):] *= fade[::-1]

    notes = {"next": (660, 880), "door": (920, 920, 920), "part": ()}.get(event, (880, 660))
    parts = []
    for freq in notes:
        time = np.arange(rate // 9, dtype=np.float32) / rate
        env = np.minimum(time / 0.008, 1.0) * np.exp(-31 * time)
        tone = np.sin(2 * np.pi * freq * time) + 0.15 * np.sin(4 * np.pi * freq * time)
        parts.append(0.17 * tone * env)
    parts.extend([np.zeros(rate // 11), speech, np.zeros(rate // 20)])
    pcm = (np.concatenate(parts).clip(-1, 1) * 32767).astype("<i2").tobytes()
    out = io.BytesIO()
    with wave.open(out, "wb") as file:
        file.setparams((1, 2, rate, 0, "NONE", "not compressed"))
        file.writeframes(pcm)
    return out.getvalue()


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--cache", required=True, type=Path, help="Workspace directory for rendered clips")
    cli.add_argument("--model", type=Path, help="Local Supertonic 3 model directory")
    cli.add_argument("--voice", default="M1")
    cli.add_argument("--threads", default=4, type=int)
    args = cli.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    tts = TTS(model="supertonic-3", model_dir=args.model, auto_download=args.model is None,
              intra_op_num_threads=args.threads, inter_op_num_threads=1)
    voice = tts.get_voice_style(args.voice)
    phrases = set((ROOT / "data/phrases.tsv").read_text(encoding="utf-8").splitlines())
    phrases.add(f"door\t{DOOR}")
    ordered = sorted(phrases, key=lambda row: (not row.startswith("door\t"), row))
    audio = ROOT / "data/demo-audio.wavpack"
    index = ROOT / "data/demo-audio.tsv"
    with audio.with_suffix(".wavpack.tmp").open("wb") as data, index.with_suffix(".tsv.tmp").open("w", encoding="utf-8", newline="\n") as manifest:
        for number, row in enumerate(ordered, 1):
            event, phrase = row.split("\t", 1)
            key = hashlib.sha256(f"supertonic3-v1:{args.voice}:{row}".encode()).hexdigest()
            cache = args.cache / (key + ".wav")
            if not cache.exists():
                cache.write_bytes(wav(tts, voice, phrase, event))
            clip = cache.read_bytes()
            manifest.write(f"{event}\t{phrase}\t{data.tell()}\t{len(clip)}\n")
            data.write(clip)
            if number == 1 or number % 20 == 0:
                print(f"{number}/{len(ordered)}", flush=True)
    audio.with_suffix(".wavpack.tmp").replace(audio)
    index.with_suffix(".tsv.tmp").replace(index)
    print(f"{len(ordered)} announcements, {audio.stat().st_size:,} bytes")


if __name__ == "__main__":
    main()
