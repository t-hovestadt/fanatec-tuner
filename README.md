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
- **Full profile load** — tuning parameters (FF, SEN, NDP, FEI, etc.) written
  on every car change; rev LED color palette and button LED colors/brightness
  sent as static one-shot writes to firmware flash; real-time RPM animation
  requires either the Fanatec App or iRacing's shift indicator (see
  [iRacing Shift LEDs](#iracing-shift-leds))
- **LED cleanup** — all wheel LEDs (rev, button, flag) are cleared via direct
  HID commands on car switch, game exit, and Ctrl-C shutdown; LEDs never stay
  stuck after a session ends
- **XML car list matching** — uses Fanatec App's `ProfileCarsList` and
  `CarsList` XMLs for exact iRacing `carPath → .pws` mapping
- **Fresh HID per write** — no stale handles; enumerate once at startup,
  open/write/close per car change
- **Sniff mode** — HID traffic analyzer with `--decode` for structured output:
  protobuf FF 10 decode, tuning state diff, LED subcmd, col01 game state, rate
  stats every 10 s
- **One-shot apply** — apply a single profile from the command line
- **CPU 0 exclusion** — automatically avoids CPU 0 on startup to prevent
  competing with iRacing's sim thread (Type B stutters); disable with
  `--no-cpu-exclude`

---

## Documentation

| Document | Description |
|----------|-------------|
| [PROTOCOL.md](PROTOCOL.md) | HID wire protocol reference — report formats, commands, SEN encoding, RGB565 table, IRSDK layout |
| [docs/SHIFT_LIGHTS.md](docs/SHIFT_LIGHTS.md) | Per-car shift light LED profiles — fill patterns, colors, flash behavior, 109-car catalog |
| [docs/HARDWARE.md](docs/HARDWARE.md) | Supported wheel bases, steering wheels, and button modules |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

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

## LED Control (Current Status)

fanatec-tuner sends the LED color palette and button LED configuration from the
`.pws` profile on each car change. This includes rev LED colors, button LED
colors/brightness, and a SAVE to firmware.

However, rev LED animation (lights responding to RPM in real time) requires a
software loop continuously sending LED commands as RPM changes. The firmware
does not animate LEDs on its own.

**Current options for rev LED animation:**

- **iRacing native mode** — set **Use Wheel Shift Indicator** to **ON** in
  iRacing (Options → Interface → External Displays). iRacing sends basic shift
  data through the Fanatec driver — LEDs show a generic green→yellow→red
  pattern. No Fanatec App needed, but no per-car customization.

- **Fanatec App** — set **Use Wheel Shift Indicator** to **OFF**. The Fanatec
  App drives LEDs with per-car auto presets (custom colors, thresholds, flash
  timing). Requires the Fanatec App running alongside fanatec-tuner.

- **Built-in real-time LED control** — fanatec-tuner reads live RPM from
  iRacing shared memory at 30 Hz, computes per-LED state from thresholds in
  the `.pws` profile (`RevLedProfile.rpm_thresholds`), and sends `FF 01 00`
  LED commands only when the LED state changes. Flash pattern fires when RPM
  exceeds `RevLedProfile.flash_threshold`. No Fanatec App needed for LED
  control. Set **Use Wheel Shift Indicator** to **OFF** in iRacing (Options →
  Interface → External Displays) to disable iRacing's own LED driver.

Button LED colors and brightness are static (set once per car change) and work
independently of the above.

---

## LED Cleanup

fanatec-tuner sends direct HID LED-off commands (`FF 01 00` with all-zero
RGB565 values) via the LED thread in three situations:

| Trigger | What fires |
|---------|-----------|
| Car switch | LEDs cleared before the new profile is applied |
| Game exit (iRacing process gone) | All LEDs cleared immediately |
| Ctrl-C shutdown | All LEDs cleared before process exits |

This covers the "LEDs stuck after game exit" case that occurs when FanaLab or
iRacing's native shift indicator is driving the LEDs. Because fanatec-tuner
sends the LED-off command directly to firmware (not via shared memory), the
clear is guaranteed even if FanaLab or iRacing are still running.

**Note for sim-teleport users**: if you are using iracing-teleport's
`--zero-on-exit` flag (which zeroes shared memory so FanaLab sees RPM=0),
fanatec-tuner's direct HID clear is faster and more reliable — FanaLab has to
poll for the RPM change, while the HID command takes effect immediately.

---

## Prerequisites

- Fanatec wheel base connected via USB
- Fanatec driver installed and `FWPnpService` running — HID reads fail without
  it; do not stop this service
- iRacing installed on the same PC
- `.pws` profile files (see [Getting Profiles](#getting-profiles) below)

---

## Getting Profiles

fanatec-tuner reads `.pws` files from FanaLab or the Fanatec App.

**Maurice's FanaLab Profiles (recommended)** — per-car profiles with full
tuning and LED settings for iRacing, AC, and ACC. Download V2.01.63 from the
Fanatec forum:
https://forum.fanatec.com/topic/990-fanalab-share-your-favorite-profiles/

**Fanatec App recommended profiles** — the Fanatec App maps every iRacing car
to one generic profile (`Recommended FFB Settings - CS DD+ (iR).pws`, FF=89
for iRacing). There is no per-car tuning from Fanatec themselves.
fanatec-tuner can apply this fallback automatically when no per-car `.pws` is
found. Per-car tuning requires Maurice's FanaLab profiles (see above).

**LED Profiles** — per-car shift light patterns (colors, fill direction,
per-gear RPM thresholds) come from
[Lovely Car Data](https://github.com/Lovely-Sim-Racing/lovely-car-data).
70 iRacing cars have verified LED data. Cars without verified LED data receive
tuning only — no LED commands are sent.

**XML car lists** — copy two files from the Fanatec App installation into your
profiles folder:

```
C:\Program Files\Fanatec\FanatecService\Service\xml\ProfileCarsList_iRacing.xml
C:\Program Files\Fanatec\FanatecService\Service\xml\CarsList_iRacing.xml
```

If the Fanatec App is installed, fanatec-tuner finds these automatically.

---

## Quick Start

1. Download `fanatec-tuner.exe` from the
   [latest release](https://github.com/t-hovestadt/fanatec-tuner/releases/latest)
2. Download Maurice's FanaLab profiles and extract the iRacing `.pws` files
3. Copy `ProfileCarsList_iRacing.xml` and `CarsList_iRacing.xml` from the
   Fanatec App `xml\` folder
4. Create the folder layout below and a `fanatec-tuner.toml`
5. Create a bat file (see Bat Files below) and run it

---

## Setup

### Directory layout

```
D:\Simracing\
├── fanatec-tuner.exe
├── fanatec-tuner.toml
├── start-fanatec-tuner.bat
└── profiles\
    ├── CarsList_iRacing.xml
    ├── ProfileCarsList_iRacing.xml
    ├── iRacing Global Mazda MX-5 Cup - 27 I 15Nm.pws
    ├── iRacing FIA F4 - 70 I 15Nm.pws
    ├── iRacing Dallara IR18 - 93 I 15Nm.pws
    └── ...
```

### fanatec-tuner.toml

```toml
[profiles]
path = "profiles"

[monitor]
scan_interval = 3   # seconds between polls (default: 3)

[xml]
path = "profiles"   # look for XML files here too
```

If the Fanatec App is installed, fanatec-tuner also auto-detects XML files at
`C:\Users\Public\Fanatec\OneFanatec\` and
`C:\Program Files\Fanatec\FanatecService\Service\xml\` — no manual copy needed
in that case.

---

## Bat Files

### Monitor mode (recommended — leave running while racing)

```bat
@echo off
cd /d "%~dp0"
echo Closing Fanatec App (if running)...
taskkill /im Fanatec.exe /f >nul 2>&1
timeout /t 2 /nobreak >nul

fanatec-tuner.exe monitor

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

## Example Output

What you should see when monitor mode is working:

```
Using col03  (ClubSport DD+, PID 0x0020)
col01 (display/ACK) found
47 profile(s) loaded
Monitoring — press Ctrl-C to stop

[09:14:22] iRacing / Global Mazda MX-5 Cup → applying profiles\iRacing Global Mazda MX-5 Cup - 27 I 15Nm.pws
  [led] rev: 9 colors
  [led] buttons: 12 sent
  [led] saved
[09:47:31] iRacing / FIA F4 → applying profiles\iRacing FIA F4 - 70 I 15Nm.pws
  [led] rev: 9 colors
  [led] buttons: 12 sent
  [led] saved
[10:12:08] Game exited — LEDs cleared
```

If a car has no matching profile:

```
[09:51:03] iRacing / Dallara P217 → no matching profile
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

Global options:

```
--no-cpu-exclude    Skip CPU 0 exclusion (useful if not running alongside iRacing,
                    or when using Process Lasso for manual affinity management)
```

Close the Fanatec App (`Fanatec.exe`) before running write commands to avoid
conflicts. Do **not** stop `FWPnpService` — it is required for HID
communication and the tool will fail to enumerate devices without it.

Press **Ctrl+C** to stop monitor mode. On exit, fanatec-tuner sends direct
HID LED-off commands (`clear_all_leds`) to the wheel base — rev, flag, and
button LEDs are cleared before the process exits. Tuning parameters are not
reset (the last-applied FFB values remain until the wheel is power-cycled or
a new profile is applied).

If you encounter `Access Denied` errors, try running as administrator.

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

## How It Works

fanatec-tuner communicates directly with the Fanatec wheel base over USB HID
using the Windows `windows-sys` crate. No Fanatec App, FanaLab, or Fanatec
SDK required.

On each car change:

1. iRacing shared memory is read to get the active car's `carPath` identifier
   from the session info YAML (`DriverInfo.Drivers[idx].CarPath`)
2. The `carPath` is looked up in Fanatec's XML car lists to find the matching
   `.pws` profile filename
3. A fresh HID write handle is opened to the wheel base's tuning collection
   (col03 on the DD+)
4. All tuning parameters are written in a single 64-byte output report
5. Rev LED color palette, button LED colors/brightness, and flag LED defaults
   are written as separate LED reports
6. A SAVE command (`FF 03 03`) is sent to persist the LED configuration to
   firmware flash
7. The handle is closed immediately — no handle is held between writes

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

## Troubleshooting

**No Fanatec device found / device not found after 60 s**
- Check the USB cable and that the wheel base is powered on
- Confirm the Fanatec driver is installed (`FWPnpService` must be running —
  open `services.msc` to verify)
- Try a different USB port

**Profile not found for car X**
- Check that `ProfileCarsList_iRacing.xml` is in the profiles folder and
  contains a mapping for that car's `carPath`
- Run `fanatec-tuner scan` to see which profiles were loaded and what car
  names they matched to
- If the XML is missing, fuzzy filename matching is used — ensure the `.pws`
  filename contains the car name as iRacing reports it

**Write failed / values not applying**
- Close the Fanatec App (`Fanatec.exe`) — it competes for the write handle
- Try running `fanatec-tuner.exe` as administrator
- Run `fanatec-tuner diag` to check which HID collections respond

**Tuning values apply but then revert**
- The Fanatec App may be running in the background and overwriting values;
  close it completely (system tray → exit)

**LEDs don't change color on car switch**
- Confirm the `.pws` profile has a `RevLedProfileWheel` section (Maurice's
  profiles do; Fanatec App recommended exports often do not)
- Check that iRacing's **Use Wheel Shift Indicator** is set correctly — see
  [iRacing Shift LEDs](#iracing-shift-leds) above

**Access Denied errors**
- Run `fanatec-tuner.exe` as administrator (right-click → Run as administrator)

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

### Completed: Real-time LED Control

Real-time RPM-driven LED control is live. fanatec-tuner reads live RPM from
iRacing shared memory at 30 Hz and drives the wheel LED strip with per-car
color profiles and flash patterns — no Fanatec App required.

Per-car LED profiles (fill direction, color stages, flash color, pit limiter)
are sourced from iRacing user manuals for 109 cars.
See [docs/SHIFT_LIGHTS.md](docs/SHIFT_LIGHTS.md) for the full catalog.

### Planned

- Flag LED control from iRacing session flags (yellow/blue/checkered)
- Gear display on 7-segment via col01
- AC / ACC / AC EVO car detection active in monitor
- UI for viewing and adjusting current tuning values
- Integration into sim-teleport

---

## Credits

- **Maurice Böschen** — FanaLab tuning profiles for 900+ cars
- **[hid-fanatecff](https://github.com/gotzl/hid-fanatecff)** —
  Linux kernel driver; wire protocol reference
- **[Lovely Car Data](https://github.com/Lovely-Sim-Racing/lovely-car-data)**
  (Lovely Sim Racing, ATSR, Gomez Sim Industries) — per-car LED profiles with
  verified per-LED colors and per-gear RPM thresholds.
  Licensed under [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/).

---

## Disclaimer

This is an independent, open-source project. It is not affiliated with,
endorsed by, or sponsored by Endor AG, Fanatec, or Corsair. "Fanatec" and
related product names are trademarks of their respective owners. Use at your
own risk — this tool communicates directly with your wheel base hardware via
USB HID.
