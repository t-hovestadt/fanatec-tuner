# fanatec-tuner

Automatic Fanatec wheel base tuning profile loader for sim racing. Monitors
iRacing for car changes and applies matching `.pws` profiles — tuning
parameters, rev LEDs, button LEDs — directly over USB HID. No FanaLab, no
Fanatec App, no manual switching.

---

## Features

- **Monitor mode** — polls iRacing shared memory every few seconds; on every
  session entry it finds the matching profile and writes it to the wheel
- **Full profile load** — writes all tuning parameters plus rev LED colors,
  button LED colors and brightness, and flag LED defaults from the `.pws` file
- **XML car list matching** — uses the Fanatec App's `CarsList` and
  `ProfileCarsList` XMLs for exact `carPath → profile` matching; falls back to
  filename fuzzy matching when XMLs are absent
- **Fresh HID per write** — enumerates once at startup to capture device paths,
  then opens a new handle for each write and closes it immediately; no stale
  handles if the device is replugged
- **Sniff mode** — passive read-only HID capture with `--decode` for structured
  output: protobuf FF 10 field decode, tuning state diff, LED subcmd decode,
  col01 game-state flags and steering axis
- **No exclusive lock held** — the tool releases the HID device between writes
  so other tools can interoperate

---

## Supported hardware

| Device | PID | Status |
|--------|-----|--------|
| ClubSport DD+ | `0x0020` | Confirmed working |
| CSL DD / CSL DD Pro | `0x0020` | Same PID, likely works |
| Podium DD1 / DD2 | `0x0006` / `0x0007` | Unconfirmed |
| CSL Elite (PC) | `0x0E03` | Unconfirmed |

All Fanatec bases share the same HID tuning protocol. The tool probes every
`0x0EB7` device and selects the collection that responds to a tuning request
(col03 on the DD+).

Wheel LEDs are supported on any rim that exposes rev/button LED HID endpoints:
Formula V2/V2.5, BMW M4 GT3, F1 Esports V2, and others.

---

## Supported games

| Game | Detection | Status |
|------|-----------|--------|
| iRacing | Shared memory (`Local\IRSDKMemMapFileName`) + session YAML | Working |
| Assetto Corsa EVO | Shared memory (`Local\acevo_pmf_static`) | Implemented, untested |
| Assetto Corsa / ACC | Shared memory (`Local\acpmf_static`) | Implemented, untested |

AC1 and ACC share the same shared memory name and are both reported as
`"Assetto Corsa"`.

---

## Setup

### 1. Download

Grab `fanatec-tuner.exe` from the [latest release](https://github.com/t-hovestadt/fanatec-tuner/releases/latest).

### 2. Stop the Fanatec service

The Fanatec wheel service holds the HID device with exclusive access.
Stop it before running fanatec-tuner:

```
net stop "Fanatec Wheel Service"
```

Or open `services.msc` and stop **Fanatec Wheel Service**. You can also exit
the Fanatec App from the system tray — this is usually sufficient.

### 3. Place your files

```
D:\Simracing\
├── fanatec-tuner.exe
├── fanatec-tuner.toml
└── profiles\
    └── CS DD+\
        ├── iRacing Acura ARX-06 GTP - 54 I 15Nm.pws
        ├── iRacing BMW M4 GT3 - 58 I 18Nm.pws
        └── ...
```

### 4. Create `fanatec-tuner.toml`

```toml
[profiles]
path = "profiles/CS DD+"

[monitor]
scan_interval = 3
```

### 5. Run it

```bat
@echo off
net stop "Fanatec Wheel Service"
timeout /t 3 /nobreak >nul
D:\Simracing\fanatec-tuner.exe monitor
net start "Fanatec Wheel Service"
```

---

## Commands

```
fanatec-tuner                # Read current tuning values from the wheel
fanatec-tuner apply <file>   # Apply a specific .pws profile
fanatec-tuner monitor        # Auto-detect car changes and apply matching profiles
fanatec-tuner list           # List all profiles from the configured directory
fanatec-tuner scan           # Verbose profile list with game/car mapping
fanatec-tuner sniff          # Passive HID capture (read-only, first 32 bytes)
fanatec-tuner sniff --decode # Structured HID decode (tuning, LED, protobuf, col01)
fanatec-tuner sniff --verbose # Full 64-byte raw capture
fanatec-tuner diag           # HID diagnostic — dump collections and capabilities
```

### Monitor mode

`fanatec-tuner monitor` polls iRacing shared memory on a configurable interval
(default 3 seconds). On every scan where a game session is active it applies
the matching profile — there is no same-car deduplication, so every session
entry (including transitions between practice, qualifying, and race) re-applies
the profile.

A 30-second grace period prevents false exits during iRacing session
transitions, which briefly drop the shared memory mapping.

**Profile matching** — tried in order:

1. `ProfileCarsList_*.xml` → exact `carPath → .pws filename` (highest
   confidence; requires Fanatec App XML files)
2. `CarsList_*.xml` → `carPath → display name` → fuzzy match
3. Filename fuzzy match on the detected car display name
4. Fanatec App recommended profile for the game (fallback)

**Fuzzy matching** steps, each tried in order:

1. Exact game + exact car name
2. Game prefix + exact car name
3. Game prefix + profile car name contains detected car name
4. Game prefix + detected car name contains profile car name

---

## Profile format

`.pws` files from Maurice Böschen's FanaLab profile pack (or your own exports
from the Fanatec App). Each file is an XML container with JSON blocks for:

| Section | Contents |
|---------|----------|
| `TuningMenuProfile` | FF, SEN, NDP, NFR, INT, NIN, FUL, SHO, BLI, FFS, FOR, SPR, DPR, BRF, FEI |
| `RevLedProfileWheel` | Per-gear rev LED colors (up to 9 LEDs, RGB color index) |
| `ButtonLedProfile` | Button LED colors (R/G/B 0–15) and brightness (0–15) per button |
| `FlagLed` | Flag LED assignments (sent as all-off; firmware overrides on flag events) |

Profile filenames follow the pattern:

```
{Game} {Car} - {FF} I {Torque}Nm.pws
```

Example: `iRacing BMW M4 GT3 - 58 I 18Nm.pws`

---

## XML car list files

Fanatec App installs XML files that map internal car identifiers to display
names and recommended profiles. fanatec-tuner reads these for exact matching.

**Auto-detected paths (checked in order):**

1. `profiles/xml/` next to the exe — local override, copy files here if you
   don't have the Fanatec App installed
2. Fanatec App install path (from registry or known default)
3. `C:\Program Files\Fanatec\FanatecService\Service\xml\`
4. Custom path set via `[xml] path` in `fanatec-tuner.toml`

When present, `ProfileCarsList_*.xml` gives exact `carPath → .pws filename`
mapping; `CarsList_*.xml` gives `carPath → display name` for fuzzy fallback.

---

## HID protocol reference

All reports are 64 bytes, report ID `0xFF`.

| Direction | Byte 1 | Byte 2 | Description |
|-----------|--------|--------|-------------|
| Write | `0x03` | `0x06` | Wake toggle — required before every write |
| Write | `0x03` | `0x00` | Write all tuning params (full 64-byte buffer) |
| Write | `0x03` | `0x02` | Request current values |
| Write | `0x03` | `0x03` | SAVE — persist LED config to firmware flash |
| Write | `0x01` | `0x00` | Rev LEDs — up to 9 × RGB565 colors |
| Write | `0x01` | `0x01` | Flag LEDs — up to 6 × RGB565 colors |
| Write | `0x01` | `0x02` | Button LED colors — up to 12 × RGB565 + commit byte |
| Write | `0x01` | `0x03` | Button LED intensities — up to 16 × 0–7 + commit byte |
| Read | `0x03` | `0x01` | Tuning response (standard mode) |
| Read | `0x03` | `0x81` | Tuning response (advanced mode) |
| Read | `0x10` | — | Device status — protobuf-encoded, wire type 0 varints |

**RGB565 encoding:** blue bits 15–11, green 10–5, red 4–0, packed big-endian.

The DD+ requires a wake toggle (`CMD 0x06`) immediately before every write.
The toggle opens a write window for one cycle; subsequent writes without
another toggle are silently ignored.

Write flow per profile apply:

```
1. Send CMD 0x02 (request values) → read current state into base buffer
2. Build 64-byte write buffer: copy base[2..62] → write[3..63], overlay params
3. Send CMD 0x06 (wake toggle)
4. Sleep 500 ms
5. Send CMD 0x00 (write all params)
6. Sleep 200 ms
7. Send CMD 0x02 (request values) → readback and diff
8. Send LED reports (rev, flag, button colors, button intensities)
9. Send CMD 0x03 (SAVE)
```

---

## Building from source

Requires Rust stable and a Windows target:

```sh
# Cross-compile from macOS/Linux
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu

# Native Windows build
cargo build --release
```

Always lint against the Windows target before pushing `#[cfg(windows)]` code:

```sh
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```

CI builds on `windows-latest` on every push and uploads the exe as an artifact.

---

## Configuration reference

```toml
[profiles]
# Directory containing .pws files (default: "profiles")
path = "profiles/CS DD+"

[monitor]
# How often to poll for car changes, in seconds (default: 3)
scan_interval = 3

[xml]
# Optional custom path to Fanatec App XML car list files
path = "C:/path/to/xml"

[fanatec_app]
# Optional override for the Fanatec App install directory
path = "C:/Program Files/Fanatec/FanatecService"
```

---

## Planned features

- **RPM-driven rev LED control** — read iRacing shared memory at ~30 Hz and
  send real-time LED updates based on actual RPM, replacing the Fanatec App's
  static preset
- **Per-car shift point extraction** — pull RPM limits from iRacing session
  YAML for accurate shift light timing per car
- **AC / EVO monitor wiring** — detection is implemented; profile
  auto-apply for non-iRacing games is pending
- **Fanatec App recommended profile fallback** — already loaded and used as
  a last-resort match when no per-car profile exists

---

## Credits

- **Maurice Böschen** — FanaLab tuning profiles for 900+ cars
- **[hid-fanatecff](https://github.com/gotzl/hid-fanatecff)** —
  open-source Linux kernel driver; wire protocol reverse-engineered from source
