# HRIR data

`sadie_d1_48k.hrir` holds head-related impulse responses taken from the **SADIE II database**
(University of York), subject **D1** (Neumann KU100 dummy head), file
`D1_48K_24bit_256tap_FIR_SOFA.sofa`, licensed under the Apache License 2.0
(`LICENSE-SADIE-II.txt`, copied from the database's Zenodo record).

- Source: https://zenodo.org/records/12092466 (SADIE II Database, V2-2)
- Reference: C. Armstrong, L. Thresh, D. Murphy, G. Kearney, "A Perceptual Evaluation of
  Individual and Non-Individual HRTFs: A Case Study of the SADIE II Database", Applied Sciences
  8(11), 2029 (2018).

**Changes made:** the 8,802 measured directions were reduced to a grid of 72 azimuths (every 5°)
× 7 elevations (−30° to +60°, every 15°), each looked up with sofar's interpolation and loudness
normalisation, and stored as 16-bit samples with one shared scale. 256 taps per ear at 48 kHz;
the interaural delay is part of the impulse responses.

Rebuild it with:

```sh
cargo run -p harp-spatial --release --example make_hrir -- D1_48K_24bit_256tap_FIR_SOFA.sofa crates/harp-spatial/data/sadie_d1_48k.hrir
```

File layout (little endian): `HRIRGRD1`, u32 rate, u32 taps, u32 azimuth count, u32 elevation
count, f32 azimuth step, f32 × elevations, f32 sample scale; then for each elevation, for each
azimuth (0° = front, counter-clockwise, +90° = left): f32 left delay, f32 right delay,
i16 × taps left, i16 × taps right.
