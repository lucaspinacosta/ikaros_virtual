# Clockwork Owl Assets

This is the active Ikaros development art pack. Its files are transparent RGBA sprites centered on 256x256 canvases. The renderer must use `animation-manifest.json` as its asset contract.

The 256px source resolution gives the desktop overlay room to scale cleanly while preserving pixel-art nearest-neighbor rendering. The layer-shell adapter initially renders the owl at 86px, approximately one third of the source image's former natural rendered size.

`test-clockwork/` remains in the repository only as legacy extraction-test material and must not be used by new renderer code.
