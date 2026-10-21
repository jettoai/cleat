# Design evidence and icon generator

Source files for the Cleat icon and the settings-window comparisons. They used to live in
a scratch folder outside the repo; commits 2b319fb, 4945fce and 264ffb1 cite that old path.

| File | What it is | Cited by |
|---|---|---|
| `b1222-icons3-gen.py` | Writes icon candidates 4 to 8 as SVG into `<folder>/4` ... `<folder>/8` (argument: output folder; create those subfolders first). Candidates 1 to 3 came from an earlier round whose source was not kept | 2b319fb (icon source) |
| `b1222-icons3-compare.py` | Lays candidates 1 to 8 side by side; it needs all eight folders, so it cannot be re-run from what is kept here | 2b319fb |
| `b1222-icons3-compare.png` | The eight candidates compared; number 8 was adopted | 2b319fb |
| `b1223v-tally-vs-cleat-dark.png` | Settings window next to Tally, dark | 4945fce |
| `b1223v-cleat-light.png` | Settings window, light | 4945fce |
| `b1209vb-compare.png` | Version label on the name's baseline, before and after | 264ffb1 |

The adopted icon files are in `rs/bundle/` (`AppIcon.icns`, `MenuBarIcon.png`, `MenuBarIcon@2x.png`).
