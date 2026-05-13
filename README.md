# fanatec-tuner

Automatic tuning profile loader for Fanatec wheel bases. Monitors iRacing for
car changes and applies the matching Fanatec tuning profile (`.pws`) via USB
HID — tuning parameters, rev LED colors, button LED colors, and flag LED
defaults.

Replaces manual profile switching in FanaLab or the Fanatec App.

---

## Features

- **Monitor mode** — runs continuously, detects car changes via iRacing shared
  memory, applies the matching `.pws` profile on every session entry
- **Full profile load** — tuning parameters (FF, SEN, NDP, FEI, etc.), rev LED
  colors, button LED colors and brightness, flag LED defaults, SAVE to firmware
- **XML car list matching** — uses Fanatec App's `ProfileCarsList` and
  `CarsList` XMLs for exact iRacing `carPath → .pws` mapping
- **Fresh HID per write** — no stale handles; enumerate once at startup,
  open/write/close per car change
- **Sniff mode** — HID traffic analyzer with `--decode` for structured output:
  protobuf FF 10 decode, tuning state diff, LED subcmd, col01 game state, rate
  stats every 10 s
- **One-shot apply** — apply a single profile from the command line

---

## Supported Hardware

| Device | PID | Status |
|--------|-----|--------|
| ClubSport DD+ | `0x0020` | Confirmed |
| CSL DD / CSL DD Pro | `0x0020` | Same PID, likely works |
| Podium DD1 / DD2 | `0x0006` / `0x0007` | Unconfirmed |
| CSL Elite (PC) | `0x0E03` | Unconfirmed |

All Fanatec bases share the same HID tuning protocol. The tool probes every
`0x0EB7` device and selects the collection that responds to the tuning request
(col03 on the DD+).

Wheel LEDs work on any rim with RGB HID endpoints: Formula V2/V2.5,
BMW M4 GT3, F1 Esports V2, ClubSport RS, and others.

---

## Supported Games

| Game | Detection | Status |
|------|-----------|--------|
| iRacing | Shared memory + session YAML | Working |
| Assetto Corsa EVO | Shared memory (`acevo_pmf_static`) | Implemented, untested |
| Assetto Corsa / ACC | Shared memory (`acpmf_static`) | Implemented, untested |

---

## Quick Start

1. Download `fanatec-tuner.exe` from the
   [latest release](https://github.com/t-hovestadt/fanatec-tuner/releases/latest)
2. Place it in your sim racing folder (e.g. `D:\Simracing\`)
3. Place your `.pws` profiles in a subfolder
4. Copy the Fanatec XML car lists into an `xml\` subfolder alongside
   the profiles (source: `C:\Program Files\Fanatec\FanatecService\Service\xml\`)
5. Create `fanatec-tuner.toml` (see below) and a bat file, then run it

---

## Setup

### Directory layout

```
D:\Simracing\
├── fanatec-tuner.exe
├── fanatec-tuner.toml
└── profiles\
    ├── xml\
    │   ├── CarsList_iRacing.xml
    │   └── ProfileCarsList_iRacing.xml
    └── CS DD+\
        ├── iRacing Acura ARX-06 GTP - 54 I 15Nm.pws
        ├── iRacing BMW M4 GT3 - 58 I 18Nm.pws
        └── ...
```

### fanatec-tuner.toml

```toml
[profiles]
path = "profiles/CS DD+"

[monitor]
scan_interval = 3   # seconds between polls (default: 3)
```

The tool also auto-detects Fanatec App XML files at
`C:\Users\Public\Fanatec\OneFanatec\` and
`C:\Program Files\Fanatec\FanatecService\Service\xml\`.

---

## Bat Files

### Monitor mode (recommended — leave running while racing)

```bat
@echo off
cd /d "%~dp0"
echo Stopping Fanatec Wheel Service...
net stop FWPnpService >nul 2>&1
timeout /t 3 /nobreak >nul

fanatec-tuner.exe monitor

echo Restarting Fanatec Wheel Service...
net start FWPnpService >nul 2>&1
pause
```

### One-shot apply

```bat
@echo off
cd /d "%~dp0"
fanatec-tuner.exe apply "profiles\CS DD+\iRacing BMW M4 GT3 - 58 I 18Nm.pws"
pause
```

### Sniff — HID traffic capture

```bat
@echo off
cd /d "%~dp0"
fanatec-tuner.exe sniff --decode
pause
```

---

## Commands

```
fanatec-tuner                     Read current tuning values from wheel
fanatec-tuner monitor             Auto-detect car changes, apply matching profile
fanatec-tuner apply <file>        Apply a specific .pws profile
fanatec-tuner list                List all profiles from the configured directory
fanatec-tuner scan                Verbose profile list with game/car mapping
fanatec-tuner sniff               Raw HID capture (read-only, first 32 bytes)
fanatec-tuner sniff --decode      Structured decode (tuning, LED, protobuf, col01)
fanatec-tuner sniff --verbose     Full 64-byte raw capture
fanatec-tuner diag                HID diagnostic — dump collections and caps
```

The Fanatec Wheel Service (`FWPnpService`) must be stopped before running any
command except `sniff`. The service holds an exclusive write handle.

---

## Profile Format

Uses `.pws` files from Maurice Böschen's FanaLab profile pack or your own
Fanatec App exports. Each profile is an XML container with JSON blocks:

| Section | HID Report | Description |
|---------|------------|-------------|
| `TuningMenuProfile` | `FF 03 00` | FF, SEN, NDP, NFR, INT, NIN, FUL, SHO, BLI, FFS, FOR, SPR, DPR, BRF, FEI |
| `RevLedProfileWheel` | `FF 01 00` | Rev LED colors (RGB565 × 9) |
| `ButtonLedProfile` | `FF 01 02` + `FF 01 03` | Button colors + brightness |
| `FlagLed` | `FF 01 01` | Flag LED defaults (sent as all-off) |
| — | `FF 03 03` | SAVE — persist LED config to firmware flash |

Profile filenames follow the pattern:

```
{Game} {Car} - {FF} I {Torque}Nm.pws
```

Example: `iRacing BMW M4 GT3 - 58 I 18Nm.pws`

---

## Car Matching

When a car is detected in iRacing, matching tries in order:

1. **`ProfileCarsList_iRacing.xml`** — exact `carPath → .pws filename` lookup
   (highest confidence)
2. **`CarsList_iRacing.xml`** — `carPath → display name`, then fuzzy filename
   match
3. **Filename fuzzy match** — car name substring in profile filename,
   with underscore/space normalization
4. **Fanatec recommended profile** — game-level fallback if installed

The `carPath` comes from iRacing's session YAML
(`DriverInfo.Drivers[idx].CarPath`) and matches the tag names in Fanatec's
XML files directly.

---

## Sniff Decode Output

With `--decode`, each report is formatted by type:

```
[00:01.234]* col03 TUNING  slot=1 SEN=900deg FF=27* NDP=50 NFR=0 ...
[00:01.235]  col03 LED-REV  9 colors: 0000 F800 FC00 FFE0 07E0 ...
[00:01.236]  col03 LED-BTN-COLOR   commit=0
[00:01.237]  col03 LED-BTN-INTENS  commit=1
[00:01.238]  col03 PROTO len=14: FF 10 ... [f1=1  f2=900*  f3=27]
[00:01.239]  col03 HW-STATUS: FF 08 ...
[00:01.240]  col01  state=on-track/03  steer=-142  status=00000001
[00:10.000]  [rate] col03: FF03=1.0/s  FF10=50.0/s
```

`*` marks bytes or fields that changed since the previous report of that type.
Every captured report appears in output — no silent drops.

---

## Configuration Reference

```toml
[profiles]
# Directory containing .pws files (default: "profiles")
path = "profiles/CS DD+"

[monitor]
# Poll interval in seconds (default: 3)
scan_interval = 3

[xml]
# Optional extra path to Fanatec XML car list files
path = "C:/path/to/xml"

[fanatec_app]
# Optional override for OneFanatec data folder
# (auto-detected at C:\Users\Public\Fanatec\OneFanatec)
path = "C:/Users/Public/Fanatec/OneFanatec"
```

---

## Building from Source

```sh
# Windows (native)
cargo build --release

# Cross-compile from macOS/Linux
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Requires Windows target (`windows-sys` for HID API and shared memory).
CI builds on `windows-latest` on every push and uploads the exe as an artifact.

Lint before pushing `#[cfg(windows)]` code:

```sh
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```

---

## Roadmap

- Real-time RPM-driven LED control from iRacing shared memory
- Per-car shift point extraction from iRacing session YAML
- AC / ACC / AC EVO car detection active in monitor
- UI for viewing and adjusting current tuning values
- Integration into sim-teleport

---

## Credits

- **Maurice Böschen** — FanaLab tuning profiles for 900+ cars
- **[hid-fanatecff](https://github.com/gotzl/hid-fanatecff)** —
  Linux kernel driver; wire protocol reference
