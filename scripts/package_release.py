"""Package an updated Windows bundle and the two offline CUDA editions."""
import argparse
import hashlib
from pathlib import Path
import shutil
import zipfile

MODEL = "Hy-MT2-7B-Q4_K_M.gguf"
MODEL_SHA = "9f96256500f3fc1ab4d64336b58f52a949a95ad7516b0c229476eef782f9f77b"
PART_BYTES = 2_080_000_000
BLOCK = 8 * 1024 * 1024


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def split(path, output):
    with path.open("rb") as src:
        for number, offset in enumerate(range(0, path.stat().st_size, PART_BYTES), 1):
            count = min(PART_BYTES, path.stat().st_size - offset)
            part = output / f"{path.name}.{number:03}"
            with part.open("wb") as dst:
                while count:
                    block = src.read(min(BLOCK, count))
                    if not block:
                        raise EOFError(path)
                    dst.write(block)
                    count -= len(block)
            yield part


def package(bundle, cuda12, cuda13, output):
    weights = bundle / "app/models" / MODEL
    if digest(weights) != MODEL_SHA:
        raise ValueError("Unexpected Tencent weights")
    full = output / "full"
    full.mkdir(parents=True, exist_ok=True)
    assets = []
    base = output / "PS5-Subtitles-Windows-x64.zip"
    with zipfile.ZipFile(base, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as z:
        for file in sorted(bundle.rglob("*")):
            rel = file.relative_to(bundle)
            if not file.is_file() or rel.parts[0] == "runtime" or rel.parts[:2] in (("app","models"),("app","hy-runtime")):
                continue
            z.write(file, "SubZir/" + rel.as_posix())
        z.mkdir("SubZir/runtime/")
        z.mkdir("SubZir/app/models/")
    assets.append(base)
    guide = output / "SubZir-Player-Guide.zip"
    with zipfile.ZipFile(guide, "w", zipfile.ZIP_DEFLATED) as z:
        for file in sorted((bundle/"docs").rglob("*")):
            if file.is_file():
                z.write(file, file.relative_to(bundle/"docs").as_posix())
    assets.append(guide)
    for edition, runtime, major in (("3080", cuda12, "12"), ("5070-Ti", cuda13, "13")):
        for required in ("llama-server.exe", "ggml-cuda.dll", f"cudart64_{major}.dll", f"cublas64_{major}.dll"):
            if not (runtime/required).is_file():
                raise FileNotFoundError(runtime/required)
        archive = full / f"SubZir-7B-RTX-{edition}.zip"
        shutil.copyfile(base, archive)
        with zipfile.ZipFile(archive, "a", zipfile.ZIP_DEFLATED, compresslevel=1) as z:
            for file in sorted(runtime.iterdir()):
                if file.is_file() and (file.suffix.lower() in {".dll",".exe"} or file.name.startswith("LICENSE")):
                    z.write(file, "SubZir/app/hy-runtime/"+file.name)
            z.write(weights, "SubZir/app/models/"+MODEL, compress_type=zipfile.ZIP_STORED)
        assets.extend(split(archive, output))
        print(archive.name,archive.stat().st_size,flush=True)
    sums = "".join(f"{digest(file)}  {file.name}\n" for file in assets)
    (output/"SHA256SUMS.txt").write_text(sums, encoding="ascii")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("bundle","cuda12","cuda13","output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    package(**vars(args))
