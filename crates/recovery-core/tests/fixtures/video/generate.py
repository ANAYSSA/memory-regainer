#!/usr/bin/env python3
"""Original synthetic playable fixtures, licensed GPL-3.0-or-later with this project.
Requires local ffmpeg/ffprobe; neither application runtime nor Rust tests invoke them.
The generated moving test pattern and sine tone contain no third-party media.
Encoder versions can change bytes; the manifest records this generation's hashes.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess

HERE = Path(__file__).resolve().parent
FFMPEG = os.environ.get("FFMPEG") or shutil.which("ffmpeg")
FFPROBE = os.environ.get("FFPROBE") or shutil.which("ffprobe")
if not FFMPEG or not FFPROBE:
    raise SystemExit("Set FFMPEG and FFPROBE or place both on PATH")


def run(args):
    return subprocess.run(args, check=True, text=True, capture_output=True)


def atoms(data):
    at = 0
    while at < len(data):
        size, kind = struct.unpack_from(">I4s", data, at)
        assert size >= 8 and at + size <= len(data)
        yield kind, data[at + 8:at + size]
        at += size
    assert at == len(data)


def box(kind, body):
    return struct.pack(">I4s", len(body) + 8, kind) + body


def widen_offsets(data):
    """Independent ISO-BMFF fixture transform: tail moov permits growth in place.
    mdat gains an eight-byte extended header, so all file-relative chunks shift +8.
    Convert 32-bit stco to 64-bit co64 and update containing atom lengths.
    """
    result = bytearray()
    for kind, body in atoms(data):
        if kind in (b"moov", b"trak", b"mdia", b"minf", b"stbl"):
            body = widen_offsets(body)
        elif kind == b"stco":
            count = struct.unpack_from(">I", body, 4)[0]
            assert len(body) == 8 + 4 * count
            offsets = struct.unpack_from(">" + "I" * count, body, 8)
            body = body[:8] + b"".join(struct.pack(">Q", x + 8) for x in offsets)
            kind = b"co64"
        result += box(kind, body)
    return bytes(result)


commands = []
for ext, audio in (("mp4", "aac"), ("mov", "pcm_s16le")):
    for layout in ("tail", "faststart"):
        target = HERE / f"original-{layout}.{ext}"
        args = [FFMPEG, "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                "-f", "lavfi", "-i", "testsrc2=size=64x48:rate=5:duration=0.6",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=8000:duration=0.6",
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "libx264", "-threads", "1",
                "-preset", "veryslow", "-crf", "28", "-pix_fmt", "yuv420p", "-c:a", audio,
                "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact",
                "-map_metadata", "-1", "-metadata", "creation_time=2026-01-01T00:00:00Z"]
        if layout == "faststart":
            args += ["-movflags", "+faststart"]
        args += [str(target)]
        run(args)
        commands.append(["ffmpeg" if x == FFMPEG else target.name if x == str(target) else x for x in args])

target = HERE / "original-hevc.mp4"
args = [FFMPEG, "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
        "-f", "lavfi", "-i", "testsrc2=size=64x48:rate=5:duration=0.6",
        "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=8000:duration=0.6",
        "-map", "0:v:0", "-map", "1:a:0", "-c:v", "libx265", "-threads", "1",
        "-x265-params", "pools=1:frame-threads=1:log-level=error", "-preset", "fast",
        "-crf", "28", "-tag:v", "hvc1", "-pix_fmt", "yuv420p", "-c:a", "aac",
        "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact",
        "-map_metadata", "-1", "-metadata", "creation_time=2026-01-01T00:00:00Z",
        str(target)]
run(args)
commands.append(["ffmpeg" if x == FFMPEG else target.name if x == str(target) else x for x in args])

source = (HERE / "original-tail.mp4").read_bytes()
converted = bytearray()
for kind, body in atoms(source):
    if kind == b"mdat":
        converted += struct.pack(">I4sQ", 1, kind, len(body) + 16) + body
    elif kind == b"moov":
        converted += box(kind, widen_offsets(body))
    else:
        converted += box(kind, body)
(HERE / "original-co64-extended.mp4").write_bytes(converted)

files = []
for path in sorted(HERE.glob("original-*.*")):
    if path.suffix not in (".mp4", ".mov"):
        continue
    data = path.read_bytes()
    probe = json.loads(run([FFPROBE, "-v", "error", "-count_frames", "-show_entries",
                           "stream=codec_name,codec_type,width,height,nb_read_frames:format=duration,size",
                           "-of", "json", str(path)]).stdout)
    decode = [FFMPEG, "-hide_banner", "-v", "error", "-xerror", "-err_detect", "explode", "-nostdin", "-i", str(path), "-map", "0", "-f", "null", "-"]
    run(decode)
    files.append({"file": path.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                  "ffprobe": probe, "full_decode_exit_code": 0})
manifest = {"schema_version": 1, "fixture_origin": "Original lavfi test pattern and sine tone; GPL-3.0-or-later",
            "ffmpeg_version": run([FFMPEG, "-version"]).stdout.splitlines()[0],
            "ffprobe_version": run([FFPROBE, "-version"]).stdout.splitlines()[0],
            "generation_commands": commands,
            "co64_transform": "widen_offsets in this script; mdat header extended by 8 bytes; chunk offsets increased by 8; tail moov ancestors resized",
            "decode_command": ["ffmpeg", "-hide_banner", "-v", "error", "-xerror", "-err_detect", "explode", "-nostdin", "-i", "FILE", "-map", "0", "-f", "null", "-"],
            "files": files}
(HERE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps([{k: f[k] for k in ("file", "bytes", "sha256", "full_decode_exit_code")} for f in files], indent=2))
