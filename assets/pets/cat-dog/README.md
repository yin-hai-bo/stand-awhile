# Cat & Dog desktop pet assets

This directory contains the used frames from the Cat & Dog sprite pack downloaded from OpenGameArt.

- Source: https://opengameart.org/content/cat-dog-free-sprites
- Author: pzUH
- License: CC0
- Download: https://opengameart.org/sites/default/files/CatnDog.zip
- Downloaded: 2026-09-27

The `png/cat` and `png/dog` directories contain only Idle, Walk, and Jump:
10 Idle, 10 Walk, and 8 Jump frames per character, 56 PNGs in total.
Unused Run, Fall, Hurt, Dead, and Slide frames have been removed.
The animation names and frame counts are recorded in `manifest.json`.

The source frames have different pixel dimensions. The loader decodes their
native sizes into premultiplied BGRA buffers. The Pet display surface is scaled
from the original frame size using the current monitor DPI (96 DPI is the
baseline), with bilinear interpolation; DPI changes do not accumulate scale.

`build.rs` embeds every PNG in this directory into the executable, and
`asset.rs` embeds the manifest. Runtime loading uses GDI+ memory streams and
decodes `idle`, `walk`, and `jump` for Cat and Dog. Updating these source assets requires rebuilding the
EXE; distribution does not require an external assets directory.
