# Changelog

## Phase 4 (current)

### Added

- Per-car shift light LED profile database: 110 active car profiles with
  verified fill pattern, color stages, flash color, and pit limiter color;
  sourced from 56 iRacing user manuals (Batch 1 + Batch 2 PDF extraction)
- `docs/SHIFT_LIGHTS.md`: user-facing per-car LED profile catalog with
  fill pattern diagrams and full 109-car table
- `docs/HARDWARE.md`: supported hardware guide — wheel bases, steering wheels,
  button modules, RGB565/RGB555 encoding, known limitations
- Real-time rev LED animation at 30 Hz: reads live RPM from iRacing shared
  memory (`RevLedProfile.rpm_thresholds`), drives LED strip per LED threshold;
  flash pattern fires when RPM exceeds `RevLedProfile.flash_threshold`
- `compute_rev_leds()` in `src/led.rs`: RPM → 9-LED state computation with
  per-LED threshold lookup and blink-on/off flash support
- `led_thread()` in `src/monitor.rs`: persistent HID thread owns col03 handle,
  receives `LedCmd::CarChanged` / `LedCmd::GameExited` via mpsc channel;
  33 ms tick; dirty-flag LED writes (only sends report when LED state changes)
- PROTOCOL.md: complete rewrite — all HID report formats, SEN encoding, RGB565
  table, IRSDK shared memory layout, ColorIndex mapping, wheel capabilities,
  legacy protocol notes

### Fixed

- Cadillac V-Series.R GTP flash color: reverted to blue (`#0000FF`) — v2
  manual explicit: "all LEDs will change to blue and begin flashing" (Batch 1
  had incorrectly set red, misreading the progression-state description)
- McLaren 570S GT4: corrected to `outside_center` pattern with v2 RPM-stage
  mapping ("outer edges inwardly"); pit limiter blue
- Mercedes-AMG GT4: corrected to `outside_center` with full stage mapping from
  v2 manual; flash red, pit yellow flash
- BMW M4 F82 GT4: flash color `#FFFF00` (last active stage color); pit limiter
  screen-only — v2 manual: "red banner / PSL light", no LED flash
- Aston Martin Vantage GT4: flash `#0000FF`, pit limiter green flash — v2
  manual: "upshift when all are flashing blue"; "all shift lights flash green"
- BMW M Hybrid V8 GTP: pit limiter set to screen-only (magenta box on display,
  not the LED strip) — v2 manual: "magenta box next to the gear indicator"
- 14 oval/dirt/NASCAR car notes updated with v2 manual confirmation text

---

## Phase 3

### Added

- `monitor` subcommand: polls iRacing shared memory every 3 seconds
  (configurable), auto-applies the matching `.pws` profile when the active
  car changes
- iRacing car detection: reads `DriverCarIdx` + `CarScreenName` from the
  iRacing shared memory YAML blob (`Local\IRSDKMemMapFileName`)
- AC / EVO car detection: reads `carModel` (UTF-16 at offset 68) from
  AC static shared memory maps (`acpmf_static`, `acevo_pmf_static`)
- 30-second grace period for iRacing session transitions — the process
  briefly disappearing during practice → qualify → race does not reset
  the active car or trigger a re-apply
- Profile fuzzy matching: exact → game-prefix + car → substring →
  reverse substring; underscore/space normalization

### Fixed

- Wake toggle (CMD `0x06`) sent immediately before every write — the
  write window opened by the probe does not persist across multiple writes
- Toggle-only fallback removed from `get_current_state` to avoid
  accidentally flipping the device out of writable state before the write

---

## Phase 2

### Added

- `apply` subcommand: parses a `.pws` profile and writes all parameters
  to the wheel base
- `.pws` parser: extracts `TuningMenuProfile > JSON` block via string
  search (no XML crate), reads all 17 tuning parameters
- `list` / `scan` subcommands: walk profiles directory, display game/car
  mapping and parameter values
- Car name extraction from filename stem
  (`{Game} {Car} - {FF} I {Torque}Nm`)
- `fanatec-tuner.toml` config: profiles directory path, scan interval
- Full-buffer write report: all parameters in one 64-byte CMD=0x00
  report — single-param writes do not work on the DD+
- Before/after diff table: shows which parameters changed after apply

### Fixed

- `build_full_write_report` addr+1 write offset: read-response byte `i`
  maps to write-buffer byte `i+1` (CMD byte at position 2 shifts all
  params right by one)

---

## Phase 1

### Added

- HID device enumeration: discovers all `0x0EB7` (Fanatec) devices via
  Windows `SetupDi` API
- Collection probing: identifies which HID collection responds to tuning
  requests (col03 on DD+) by attempting a write to each and checking for
  `ERROR_INVALID_FUNCTION`
- Wake toggle protocol: CMD `0x06` to enter advanced mode, CMD `0x04` to
  request current values; double-toggle with probe to converge reliably
  from unknown starting state
- Tuning readback: parses all parameters from 64-byte interrupt IN report
- SEN decoding: converts wire byte to degrees for DD+ multi-range encoding
  (three sub-ranges packed into one byte, plus AUTO = 0x00)
- `diag` subcommand: dumps HID capabilities and probe results for all
  collections
- PROTOCOL.md: full reference for report format, commands, parameter
  addresses, SEN encoding, wake sequence, read/write flows
