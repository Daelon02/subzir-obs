"""Benchmark the release executable with the same 20 images used for the RTX 3080 report."""
import csv
import ctypes
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
from datetime import datetime

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "app"
FRAMES = ROOT / "benchmarks" / "frames20"

def measure(report):
    gpu = "CPU"
    models = ["quickmt"]
    server = APP / "hy-runtime" / "llama-server.exe"
    weights = APP / "models" / "Hy-MT2-7B-Q4_K_M.gguf"
    gpu_models = []
    if weights.is_file():
        gpu_models.append("gguf:" + weights.name)
    if server.with_name("Hy-MT2-1.8B-Q6_K.gguf").is_file():
        gpu_models.append("hy")
    if server.is_file() and gpu_models:
        probe = subprocess.run([server, "--list-devices"], capture_output=True, encoding="utf8", errors="replace")
        devices = (probe.stdout + probe.stderr).splitlines()
        gpu = next((s.strip() for s in devices if s.strip().startswith("CUDA0:")), None)
        if probe.returncode or not gpu:
            raise RuntimeError("CUDA0 недоступна. Перевірте драйвер NVIDIA.")
        models.extend(gpu_models)
    print(gpu, flush=True)
    rows = []
    summaries = [gpu, "20 готових кадрів → OCR → переклад → підтвердження OBS. Мілісекунди, після прогріву."]
    for model in models:
        print("Тест:", model, flush=True)
        env = {**os.environ, "SUB_RUNTIME": str(ROOT / "runtime"), "SUB_MODEL": model,
               "SUB_TARGET": "uk", "SUB_ACK": "1", "SUB_BEAM":"3",
               "SUB_PYTHON": str(APP / "python/python.exe"), "SUB_WORKER": str(APP / "worker.py")}
        with (report / (model.removeprefix("gguf:")+".log")).open("wb") as log:
            run = subprocess.run([APP / "subtitle-rs.exe", "bench", FRAMES], env=env, cwd=APP,
                                 stdout=subprocess.PIPE, stderr=log, encoding="utf8", check=True)
        batch = [dict(model=model, **json.loads(s)) for s in run.stdout.splitlines()]
        if len(batch) != 20:
            raise RuntimeError("Неповний прогін. Докладніше у файлі .log.")
        rows.extend(batch)
        with (report / "lines.csv").open("w", encoding="utf-8-sig", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=rows[0].keys())
            writer.writeheader()
            writer.writerows(rows)
        obs = sorted(r["obs_tick_ms"] for r in batch if r["obs_tick_ms"] is not None)
        summaries.append(f"{model}: OBS підтвердив {len(obs)}/20; OCR дав текст для {sum(bool(r['en']) for r in batch)}/20.")
        if len(obs) == 20:
            summaries.append(f"OBS median {statistics.median(obs):.1f} ms; p95 {obs[math.ceil(.95*len(obs))-1]:.1f} ms.")
        else:
            summaries.append("В OBS додайте скрипт із цього комплекту й відкрийте сцену з видимими субтитрами.")
        (report / "summary.txt").write_text("\n".join(summaries)+"\n", encoding="utf-8-sig")
    print("\n".join(summaries))

def main():
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateMutexW.argtypes = (ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p)
    kernel.CreateMutexW.restype = ctypes.c_void_p
    handle = kernel.CreateMutexW(None, False, r"Local\PS5QuickMTLauncher")
    if not handle or ctypes.get_last_error() == 183:
        raise RuntimeError("Закрийте SubZir та інші копії тесту. Залиште OBS відкритим.")
    try:
        report = ROOT / "benchmarks" / "results" / datetime.now().strftime("%Y%m%d-%H%M%S-%f")
        report.mkdir(parents=True)
        print("Результати:", report, flush=True)
        measure(report)
    finally:
        kernel.CloseHandle(ctypes.c_void_p(handle))

if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
