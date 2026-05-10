# Changelog

## Phase 3 (current)

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
