# Cat & Dog desktop pet assets

This directory contains the Cat & Dog sprite pack downloaded from OpenGameArt.

- Source: https://opengameart.org/content/cat-dog-free-sprites
- Author: pzUH
- License: CC0
- Download: https://opengameart.org/sites/default/files/CatnDog.zip
- Downloaded: 2026-09-27

The `png/cat` and `png/dog` directories contain separate PNG frames. The
animation names and frame counts are recorded in `manifest.json`.

The source frames have different pixel dimensions. The loader decodes their
native sizes into premultiplied BGRA buffers. The Pet display surface is scaled
from the original frame size using the current monitor DPI (96 DPI is the
baseline), with bilinear interpolation; DPI changes do not accumulate scale.

`build.rs` embeds every PNG in this directory into the executable, and
`asset.rs` embeds the manifest. Runtime loading uses GDI+ memory streams and
decodes `idle`, `walk`, and `jump` for Cat and Dog. The other animations remain
embedded for future use. Updating these source assets requires rebuilding the
EXE; distribution does not require an external assets directory.
