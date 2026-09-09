#!/usr/bin/env python3
"""Render assets/demo-30s.mp4 + .gif — exact on-screen copy, 30 seconds, 16:9."""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-q", "pillow"])
    from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "assets"
W, H = 1280, 720
FPS = 12
SECS = 30
FRAMES = FPS * SECS
BG = (11, 0, 20)
MG = (255, 43, 214)
CY = (62, 240, 255)
GOLD = (255, 210, 74)
FG = (244, 244, 248)
DIM = (160, 160, 176)
PANEL = (20, 0, 31)


def font(size: int, mono: bool = False) -> ImageFont.FreeTypeFont:
    path = (
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
        if mono
        else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
    )
    bold = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
    if not mono and Path(bold).is_file() and size >= 28:
        path = bold
    return ImageFont.truetype(path, size)


def ease(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def fade(t: float, start: float, dur: float = 0.4) -> int:
    if t < start:
        return 0
    return int(255 * ease((t - start) / dur))


def frame(t: float) -> Image.Image:
    im = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(im)
    d.rectangle((0, 0, W, 6), fill=MG)

    def ink(rgba_a: int, color=FG):
        return (*color, max(0, min(255, rgba_a)))

    # Use RGBA overlay for fades
    ov = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    od = ImageDraw.Draw(ov)

    a1 = fade(t, 0.0)
    od.text((64, 48), "CuNi", font=font(54), fill=ink(a1, CY))
    od.text((250, 62), "119 languages", font=font(32), fill=ink(a1, MG))
    od.text(
        (64, 118),
        "One program. Same stdout. Or the compiler refuses.",
        font=font(22),
        fill=ink(a1, DIM),
    )

    a2 = fade(t, 3.5)
    src = [
        "def add(a: int, b: int) -> int do",
        "    ret a + b",
        "end",
        "",
        "say(add(2, 40))",
        'say("cuni")',
    ]
    od.rounded_rectangle((56, 170, 620, 430), 12, fill=(20, 0, 31, a2))
    y = 188
    for line in src:
        od.text((80, y), line, font=font(20, mono=True), fill=ink(a2, FG))
        y += 36

    seats = [
        ("Python", 11.0),
        ("Go", 12.0),
        ("JavaScript", 13.0),
        ("C", 14.0),
        ("C++", 15.0),
        ("Rust", 16.0),
    ]
    x0, y0 = 660, 170
    for i, (name, start) in enumerate(seats):
        col = i % 2
        row = i // 2
        x = x0 + col * 270
        y = y0 + row * 90
        a = fade(t, start, 0.25)
        od.rounded_rectangle((x, y, x + 250, y + 78), 10, outline=ink(a, MG), width=2)
        od.text((x + 14, y + 8), name, font=font(16), fill=ink(a, CY))
        od.text((x + 14, y + 34), "42", font=font(18, mono=True), fill=ink(a, GOLD))
        od.text((x + 14, y + 54), "cuni", font=font(18, mono=True), fill=ink(a, GOLD))

    a4 = fade(t, 20.0)
    od.text(
        (64, 470),
        "exactness: PASS",
        font=font(40),
        fill=ink(a4, GOLD),
    )
    od.text(
        (64, 530),
        "6 native seats   ·   119-language gate",
        font=font(22),
        fill=ink(a4, FG),
    )

    a5 = fade(t, 25.0)
    od.text(
        (64, 640),
        "cuni-studio.fly.dev",
        font=font(22),
        fill=ink(a5, CY),
    )
    im = Image.alpha_composite(im.convert("RGBA"), ov).convert("RGB")
    return im


def main() -> None:
    tmp = Path(tempfile.mkdtemp(prefix="cuni_demo30_"))
    print(f"rendering {FRAMES} frames → {tmp}", flush=True)
    for i in range(FRAMES):
        t = i / FPS
        frame(t).save(tmp / f"f{i:04d}.png")
        if i % 24 == 0:
            print(f"  {i}/{FRAMES}", flush=True)

    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        subprocess.check_call(["sudo", "apt-get", "install", "-y", "-qq", "ffmpeg"])
        ffmpeg = shutil.which("ffmpeg") or "ffmpeg"

    mp4 = OUT_DIR / "demo-30s.mp4"
    gif = OUT_DIR / "demo-30s.gif"
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    subprocess.check_call(
        [
            ffmpeg,
            "-y",
            "-framerate",
            str(FPS),
            "-i",
            str(tmp / "f%04d.png"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "18",
            str(mp4),
        ]
    )
    subprocess.check_call(
        [
            ffmpeg,
            "-y",
            "-i",
            str(mp4),
            "-vf",
            "fps=8,scale=640:-1:flags=lanczos",
            str(gif),
        ]
    )
    shutil.rmtree(tmp, ignore_errors=True)
    print(f"wrote {mp4}")
    print(f"wrote {gif}")


if __name__ == "__main__":
    main()
