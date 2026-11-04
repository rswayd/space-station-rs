# Assets license

The code in this repository is AGPL-3.0-only (`LICENSE`). This file covers the assets.

## Where the assets come from

The game's assets (icons, sounds) are not stored in this repository. They are generated from a
checkout of [BandaStation](https://github.com/ss220club/BandaStation), a fork of
[tgstation](https://github.com/tgstation/tgstation), by the converters (`ss13-import dmi`,
`ss13-mapload convert`; see `docs/RUNNING.md`). Every generated set names the source commit it
was made from: `ICON-SOURCES.txt` and `sound/SOURCES.txt` in the assets directory. The current
reference commit is
[`ce5b1f159748db35281bd3b673551598807f4a0b`](https://github.com/ss220club/BandaStation/commit/ce5b1f159748db35281bd3b673551598807f4a0b).

## License

BandaStation's assets are licensed under
[Creative Commons Attribution-ShareAlike 3.0](https://creativecommons.org/licenses/by-sa/3.0/)
(CC BY-SA 3.0) unless otherwise indicated. Per-file exceptions and credits are in the
`attributions.txt` / `attribution.txt` / `license.txt` files of the source tree; the converters
copy every such file that applies to a converted asset, at its source path, next to the output:

- sounds: `sound/<source path>/` (listed in `sound/SOURCES.txt`);
- icons: `licenses/icons/<source path>/` (atlas pages and their source `.dmi` files are listed in
  `ICON-SOURCES.txt`).

[`docs/ASSET-AUDIT.md`](docs/ASSET-AUDIT.md) lists the converted assets whose files name other
terms (CC0, CC BY, non-commercial, site licenses, no stated license) with the risk of each.

## Non-commercial project; restricted assets

This project and its servers are non-commercial. Some converted assets depend on that: their
terms forbid commercial use or limit use to personal, educational and research purposes. They are
the `high` rows of [`docs/ASSET-AUDIT.md`](docs/ASSET-AUDIT.md):

- `sound/items/handling/helmet/visor_up1-2.ogg`, `visor_down1-3.ogg`: CC BY-NC 4.0
  ("Bike helmet visor.wav" by Didi0508, freesound.org/s/541921);
- `sound/items/weapons/gun/revolver/spin1-3.ogg`: cut from a BBC Sound Effects recording, RemArc
  licence (personal, educational and research use only).
- `modular_bandastation/security_levels/sound/gamma.ogg`, `epsilon.ogg` and
  `modular_bandastation/aesthetics_sounds/sound/epsilon_laws.ogg`: CC BY-NC 4.0 (dj-34,
  github.com/dj-34), the Gamma and Epsilon security level sounds.

Anyone who runs or distributes this project commercially (paid access, ads, paid perks) must
exclude or replace these assets first.

## Converted derivatives

The atlases (`atlas/*.qoi`, packed from `.dmi` files, including rendered GAGS icons), the
manifest, re-encoded or copied sound files and the other generated data are adaptations of
those assets and are distributed under CC BY-SA 3.0 as well, except where an attribution file
of a source asset states other terms; those terms then apply to that asset's part.

## Config

`config/` (admin ranks and the admin list format) is copied from BandaStation and is
AGPL-3.0, like BandaStation's code.
