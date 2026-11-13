# Fixtures

- `gpx/synthetic_*.gpx` – handwritten files with known content, used by parser tests.
- `gpx/real/` – real recorded tracks (to be added). Tests iterate over every file in
  this directory and check that it parses and yields plausible statistics.
