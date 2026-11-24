# Fixtures

- `gpx/synthetic_*.gpx` – handwritten files with known content, used by parser tests.
- `gpx/real/` – real recorded tracks (to be added). Tests iterate over every file in
  this directory and check that it parses and yields plausible statistics.
- `osm/flying.osm` – handwritten OpenStreetMap objects for the `flying` layer test (`scripts/tiles/test-outdoor-overlay.sh`).
- `gpx/replay/*.gpx` – tracks that the UI tests of Trail play in place of the GPS (`-replayGPX`).
