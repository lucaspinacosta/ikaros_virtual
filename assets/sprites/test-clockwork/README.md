# Temporary Clockwork Owl Sprites

These are development-only sprites supplied for testing the Ikaros renderer. They are not the final art direction.

All PNG files use a transparent 48x48 canvas. `animation-manifest.json` is the runtime mapping used by the project. It corrects two limitations in the supplied extraction metadata:

- only four `wing_flap` files are present, so flight has four frames;
- the second set of side-walk frames is named `walk_left_04` through `walk_left_07`, but maps to the right-walk animation.

Replace this directory with final sprites only after preserving the same manifest contract or updating the renderer and `SpriteLoop` frame counts together.
