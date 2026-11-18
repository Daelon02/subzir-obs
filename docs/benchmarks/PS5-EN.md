# Live PS5 benchmark in God of War Ragnarök

On November 18, 2026 at 12:04–12:06 UTC, SubZir published 47 nonempty updates and 12 clears during 174 seconds of gameplay. OBS acknowledged all 47 updates. The sample includes successive OCR corrections to the same dialogue.

PS5 → FIFINE capture card → OBS virtual camera source (2560×1440 at 60 Hz) → Windows OCR sampled at 10 Hz → QuickMT EN→UK (CPU, INT8, beam 3) → text source in a second OBS instance on the same PC. Ryzen 7 7800X3D, RTX 3080 10 GB, OBS 33.0.1.

| Stage, ms | Median | p95 | Min | Max |
|---|---:|---:|---:|---:|
| OCR | 3.4 | 5.0 | 1.6 | 6.8 |
| Translation | 30.9 | 47.7 | 14.8 | 75.4 |
| Through file publication | 36.7 | 53.6 | 18.1 | 81.4 |
| Through OBS acknowledgement | 64.5 | 91.6 | 48.8 | 114.1 |

Timing starts when processing begins on the frame that confirms a stable line. The endpoint is the Lua tick after updating a visible OBS text source. Time from first appearance in the game also includes capture, sampling and stabilization. Physical display latency was unmeasured. The p95 is the 45th ordered observation out of 47.

“Sindri’s been helping me.” appeared as “Сіндрі мені допомагає.” Other frames contained OCR errors in names and partial words. These figures apply to the Ukrainian QuickMT path. [All events (CSV)](ps5-capture.csv)

## Replay on the same 20 lines

After worker warm-up, the benchmark sent the same 20 English dialogue and slang images through Windows OCR once for each language. OBS acknowledged all 60 updates. This replay bypasses capture polling and stabilization.

| Language | Translation, median / p95 (ms) | Through OBS, median / p95 (ms) |
|---|---:|---:|
| Ukrainian | 18.4 / 25.9 | 55.5 / 62.7 |
| Polish | 196.1 / 289.9 | 232.5 / 332.9 |
| French | 186.3 / 285.1 | 215.9 / 316.6 |

For “Your ride just arrived at the motel.” the French model gave “Votre voyage vient d’arriver au motel.” Here *ride* means the car. The Ukrainian path includes extra rules for this use of slang. [All 60 translations and timings](subtitle-language-replay.csv)
