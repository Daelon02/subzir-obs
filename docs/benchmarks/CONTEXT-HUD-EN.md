# Dialogue context and HUD capture on RTX 3080

English-to-Ukrainian, Windows, RTX 3080 10 GB, Hy-MT2 7B Q4_K_M. Each warmed run processed 162 frames in 54 three-line scenes through Windows OCR, the translation worker and a visible OBS text source at 60 fps. Times below cover the 54 target lines, starting with the prepared frame and ending at the OBS script acknowledgement. The ranges show separate runs. [Per-line measurements](context-hud-dialogue-ab.csv)

| Dialogue mode | Median to OBS | P95 to OBS |
|---|---:|---:|
| Standard (A) | 198–201 ms | 300–316 ms |
| Concise (D) | 184–189 ms | 278–284 ms |

In a separate 10 Hz live replay, all nine lines reached OBS. The median from the first supplied frame to acknowledgement was 209 ms with A and 173 ms with D. Maximums were 309 and 312 ms. On the 54 target lines a single blinded review preferred D six times and A seven times. The other 41 were equivalent or identical. D distinguished settling a debt from making up after a fight, but introduced a grammatical error on “rip-off”. Standard dialogue is the default. Select **Concise dialogue** in SubZir to try D with Ukrainian and the 7B Q4_K_M model. [Live timings](context-dialogue-live.csv) · [Line-by-line review](context-dialogue-review.csv)

A final integrated check (ShipA/ShipD in the CSV) confirmed all 162 rows in each mode. Target medians were 211.5 and 195.2 ms, with P95 at 311.8 and 290.9 ms respectively.

Both capture paths read all three regions on the same nine-second 720p God of War Ragnarök recording and produced identical translations. In a separate set of ten synthetic 1080p clips with three regions each, exact first OCR readings rose from 13/30 to 24/30 when cropping before downscaling. The number of regions that produced any confirmed text rose from 27 to 30. Median FFmpeg preparation time for the three regions went from 0.95 to 0.15 ms/frame at 1080p and from 3.23 to 0.41 ms/frame at 4K (six alternating runs of 200 frames). The 4K test measured processing cost. Physical HDMI/display latency and RTX 5070 Ti performance were outside these measurements. [OCR results](hud-native-1080p.csv) · [All six dialogue variants and test details in Ukrainian](CONTEXT-HUD-UK.md)
