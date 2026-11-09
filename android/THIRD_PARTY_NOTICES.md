# Third-party material in the Android app

Code libraries are listed with their licences in `app/build.gradle.kts`. This file covers what
is copied into the app's own sources.

## Material Symbols (icons)

The `ic_*.xml` vector drawables in `app/src/main/res/drawable/` that say so in their first
comment are icons from [Material Symbols](https://fonts.google.com/icons), style *Rounded*,
weight 400, grade 0, optical size 24, from
[google/material-design-icons](https://github.com/google/material-design-icons)
(`symbols/android/<name>/materialsymbolsrounded/<name>_24px.xml`).

Copyright Google LLC. Licensed under the Apache License, Version 2.0
(<https://www.apache.org/licenses/LICENSE-2.0>).

Changes made to them: zero-length path segments removed, coordinates written as relative
commands to shorten the paths, the theme tint attribute removed (Compose tints them), and some
marked `autoMirrored` for right-to-left languages. The shapes are unchanged.

`ic_launcher_foreground.xml` and `ic_tile.xml` are paste-get's own.
