# Changelog

Notable changes to this fork, on top of upstream
(`jjnuthuagen/cosmic-start-menu-applet`, at the repo-publishing commit).

## Unreleased

### Fixed

- Search matches a query word by word, so an app whose name holds every word —
  `office libre`, `fire fox` — is found instead of returning nothing.
- A tile colour with a non-ASCII character where 3 or 6 hex digits are expected
  no longer panics the applet at render.
- An animated GIF keeps its shape: frames cover-crop the tile like a still,
  instead of being stretched to the tile box.
- A tile picture is sniffed from six bytes rather than reading the whole file
  (a video, a huge photo) on every frame, and GIF frames decode one at a time,
  capped.
- A folder no longer repeats an app that the pinned "Most used" block already
  shows — it was drawn twice and was two keyboard stops.
- The Folders label is measured with a `Line::Label`'s height, so a letter jump
  in Folders mode lands on its header instead of 6 px past it.

### Changed

- `just restart` cycles whichever panel's wing lists the applet, so an install
  into the Dock is restarted rather than killed and left on the old build.
- CI runs the build on manual dispatch as well as on a push.
