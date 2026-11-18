# Subtitle translation and OBS latency on an RTX 3080

November 19, 2026. Windows, Ryzen 7 7800X3D, GeForce RTX 3080 10 GB, OBS Studio 33.0.1. Each mode received the same 20 English lines in the same order and produced Ukrainian subtitles. Recorded images were replayed into the capture path and a running OBS instance acknowledged each update. The measurements exclude HDMI capture, monitor scanout and the load of running a PC game alongside OBS. [Earlier live PS5 measurements](PS5-EN.md)

## From the first subtitle frame to OBS

Replay ran at 10 Hz. Each line stayed on screen for 800 ms, followed by 600 ms of blank frames. Every line required a second observation before publication. Models were warm.

| Mode | Median | p95 | Shown |
|---|---:|---:|---:|
| QuickMT on CPU | 155.2 ms | 174.8 ms | 20/20 |
| Hy-MT2 on GPU, start after confirmation | 213.0 ms | 232.6 ms | 20/20 |
| Hy-MT2 on GPU, start on first reading | 127.9 ms | 156.0 ms | 20/20 |

Starting translation on the first reading saved 85.1 ms at the median. Both Hy-MT2 modes displayed identical translations for all 20 lines. A single-frame reading inserted ahead of the dialogue never appeared in OBS. Results can vary with input timing and system load. [Per-line replay measurements](rtx3080-live.csv)

## OCR, translation and OBS on individual images

A separate run fed the 20 images directly to Windows OCR. The OBS column includes OCR, translation, file publication and acknowledgement. Each cell gives the median / nearest-rank p95. QuickMT includes SubZir’s existing handling for colloquial phrases. Some return without a model call.

| Model | Translation | Frame to OBS acknowledgement |
|---|---:|---:|
| QuickMT INT8 on CPU | 17.2 / 25.4 ms | 45.4 / 62.0 ms |
| M2M100 418M INT8 on CPU | 201.7 / 326.2 ms | 233.9 / 352.2 ms |
| Hy-MT2 1.8B Q6_K on GPU | 83.8 / 106.4 ms | 110.4 / 138.6 ms |

## Translation quality

QuickMT produced “Гей!” for *Hey!* and “Гей” for *Hey*. Hy-MT2 gave “Гей!” and “Ей!”. The bare word originally went undetected on the sample image and was recovered by enlarging a small isolated text region after an empty OCR result. The reported “Гейр” output did not recur on these frames.

All three models missed the robbery sense of *This is a stick-up!*, although QuickMT and Hy-MT2 captured the meaning of *You always stick up for him.* On the supplied screenshot OCR read *The bombs're away and I'm touchin' down hot.* exactly. QuickMT preserved the landing but lost its urgency. Hy-MT2 introduced swimming. Hy-MT2 stays optional. Its live input includes the previous confirmed line when it falls within eight seconds. These examples are evaluation cases. [OCR, model outputs and timings for every case](rtx3080-frames.csv)

A separate six-line capture replay exposed a second failure. The high-contrast filter split a letter in *Hey!*, and OCR returned “I-dey!”. Checking the original strokes for isolated short words recovered all six English lines in this replay. QuickMT displayed “Гей!” and “Гей”, and Hy-MT2 displayed “Гей!” and “Ей!”. [Recognized text, translation and latency in OBS](rtx3080-examples-live.csv)
