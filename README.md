# fanatec-tuner

Reads Fanatec wheel tuning profiles from FanaLab `.pws` files and
automatically applies them whenever you switch cars in iRacing. Works
over USB HID — no Fanatec SDK, no FanaLab running.

---

## Supported hardware

| Device | PID | Status |
|--------|-----|--------|
| ClubSport DD+ | `0x0020` | Confirmed working |
| CSL DD / CSL DD Pro | `0x0020` | Same PID, likely works |
| Podium DD1 / DD2 | `0x0006` / `0x0007` | Unconfirmed |
| CSL Elite (PC, red LED) | `0x0E03` | Unconfirmed |

All Fanatec bases share the same HID tuning protocol. The tool probes
every `0x0EB7` device and selects the collection that responds to a
tuning request (col03 on the DD+).

---

## Supported games

| Game | Detection | Status |
|------|-----------|--------|
| iRacing | Shared memory (`Local\IRSDKMemMapFileName`) | Working |
| Assetto Corsa / EVO / ACC | Shared memory (`acpmf_static`, `acevo_pmf_static`) | Planned |

---

## Profile format

fanatec-tuner reads `.pws` files from Maurice Böschen's FanaLab profile
pack. These are XML files containing a `TuningMenuProfile > JSON` block
with all tuning parameters.

Profile filenames must follow the pattern:
```
{Game} {Car} - {FF} I {Torque}Nm.pws
```
Example: `iRacing Acura ARX-06 GTP - 54 I 15Nm.pws`

The game name and car name are extracted from the filename stem. Fuzzy
matching handles minor differences between the filename and iRacing's
reported car name (substring, reverse substring, underscore→space
normalization).

---

## Setup

### 1. Stop the Fanatec App and service

The Fanatec App (`FWPnpService`) holds the HID device with exclusive
access. Stop it before running fanatec-tuner:

```
net stop FWPnpService
```

Or open `services.msc` and stop **Fanatec Wheel Service**. You can also
exit the Fanatec App from the system tray.

### 2. Place files

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

### 3. Create fanatec-tuner.toml

```toml
[profiles]
path = "profiles/CS DD+"

[monitor]
scan_interval = 3
```

---

## Usage

```
fanatec-tuner              # Read current tuning values from wheel
fanatec-tuner list         # List all profiles from configured directory
fanatec-tuner scan         # Verbose profile list with game/car mapping
fanatec-tuner apply <file> # Apply a specific .pws profile
fanatec-tuner monitor      # Auto-detect game + car, apply matching profile
fanatec-tuner diag         # HID diagnostic — dump collections and capabilities
```

### Monitor mode

`fanatec-tuner monitor` polls iRacing shared memory every 3 seconds
(configurable via `monitor.scan_interval`). When the active car changes,
it finds the matching `.pws` profile and applies it automatically.

Profile matching priority:
1. Exact game name + exact car name
2. Game prefix match + exact car name
3. Game prefix + profile car name contains detected car name
4. Game prefix + detected car name contains profile car name

iRacing session transitions (practice → qualify → race) briefly remove
the game process. fanatec-tuner waits 30 seconds before declaring the
game exited, so a profile is not needlessly re-applied between sessions.

---

## How writes work

The DD+ requires a **wake toggle** (CMD `0x06`) sent immediately before
every write. The toggle opens a write window for exactly one write cycle;
subsequent writes without another toggle are silently ignored.

Flow for each profile apply:
```
1. Send CMD 0x04 (request values) → read current state
2. Build 64-byte write buffer with all parameters
3. Send CMD 0x06 (wake toggle) — opens write window
4. Sleep 500 ms
5. Send CMD 0x00 (write) — all parameters in one report
6. Sleep 200 ms
7. Send CMD 0x04 (request values) → read back and diff
```

All parameters are sent in a single 64-byte report. Sending each
parameter in a separate report does not work on the DD+ — readback shows
no change.

See [PROTOCOL.md](PROTOCOL.md) for the full protocol reference.

---

## Known limitations

- **Fanatec App must be stopped** — it holds the HID device with
  exclusive access. fanatec-tuner will fail to open the device with
  `ERROR_SHARING_VIOLATION` if the App or `FWPnpService` is running.
- **iRacing only for monitor mode** — AC / EVO / ACC car detection is
  not yet implemented.
- **Apply takes ~1.5 seconds** — the 500 ms post-toggle sleep is
  required by the device before it will accept the write.

---

## Building from source

Requires Rust stable and the Windows cross-compile target:

```
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64   # macOS
cargo build --release --target x86_64-pc-windows-gnu
```

CI builds on `windows-latest`. Always run clippy against the Windows
target before pushing `#[cfg(windows)]` code:

```
cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
```

---

## Credits

- **Maurice Böschen** — FanaLab tuning profiles for 900+ cars
- **[hid-fanatecff](https://github.com/gotzl/hid-fanatecff)** —
  open-source Linux kernel driver; protocol reverse-engineered from source
