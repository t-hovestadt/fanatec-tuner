# Fanatec HID Tuning Protocol

Reference for the HID output-report / interrupt-in-report protocol used by
Fanatec steering wheel bases. Derived from the open-source
[hid-fanatecff](https://github.com/gotzl/hid-fanatecff) Linux kernel driver.

---

## Device identification

| Device | Vendor ID | Product ID |
|---|---|---|
| ClubSport Wheelbase V2 | 0x0EB7 | 0x0001 |
| ClubSport Wheelbase V2.5 | 0x0EB7 | 0x0004 |
| CSL Elite (PC, red LED) | 0x0EB7 | 0x0E03 |
| CSL Elite PS4 (PC mode) | 0x0EB7 | 0x0005 |
| Podium DD1 | 0x0EB7 | 0x0006 |
| Podium DD2 | 0x0EB7 | 0x0007 |
| CSL DD / CSL DD Pro | 0x0EB7 | 0x0020 |
| **ClubSport DD+** (2024) | 0x0EB7 | **0x0020** |
| CSR Elite | 0x0EB7 | 0x0011 |
| Porsche 911 Turbo S | 0x0EB7 | 0x0197 |

The ClubSport DD+ shares PID 0x0020 with the CSL DD — confirmed from hardware.

All tuning-capable bases share the same report format; device capabilities
differ only in which parameters are populated (see Parameter table).

---

## Report format

Every tuning-related message is exactly **64 bytes**.

```
Byte 0:  0xFF  — report ID
Byte 1:  0x03  — tuning marker (distinguishes tuning reports from other HID traffic)
Byte 2:  command / flags
Byte 3:  slot or additional argument (command-dependent)
Bytes 4–63:  parameter data and padding
```

The device's interrupt IN reports follow the same layout when responding
to a values-request command.

---

## Commands (byte 2)

| Byte 2 | Purpose | Byte 3 |
|---|---|---|
| `0x00` | Write all parameters | 0x01 (devId; must not be 0x81) |
| `0x01` | Select tuning slot | slot number (1–5) |
| `0x02` | Request current tuning values (READ) | 0x00 |
| `0x03` | Save tuning to flash | 0x00 |
| `0x04` | **Factory reset** — restores device to factory defaults | 0x00 |
| `0x06` | Toggle advanced mode | 0x00 |

**Warning:** `0x04` is a destructive factory reset, not a read command. Always use `0x02` to request tuning values.

**Sending:** use `WriteFile` on the device handle (HID output report).
Do **not** use `HidD_SetFeature` — the driver uses `hid_hw_output_report`,
which maps to a HID OUTPUT report type, not a FEATURE report.

**Receiving:** use `ReadFile` on the same handle (HID interrupt IN report).
The device echoes tuning values whenever the host sends command 0x04 or
the user adjusts a knob on the base.

---

## Parameter byte addresses

Addresses are the wire byte index within the 64-byte **read response**.

**Write direction offset:** in the write format every parameter sits at
`addr + 1`. This is because byte 2 carries the CMD_WRITE_PARAM command
(0x00), shifting all parameters one position to the right.

```
Read response:  [0xFF][0x03][SLOT][SEN][FF][SHO]...
Write command:  [0xFF][0x03][0x00][SLOT][SEN][FF][SHO]...
                              ↑ cmd
```

The driver stores received reports at `ftec_tuning_data[1..]` (shifted by
one) precisely so the retained buffer can be used directly as the write
buffer. Confirmed: `hid-ftecff-tuning.c:85`
```c
drv_data->tuning.ftec_tuning_data[addr + 1] = val;
```

Conversions:
- **noop** — display value = wire byte (no scaling)
- **×10** — display value = wire byte × 10 (wire stores 0–10 or 0–12)
- **sens** — SEN encoding (see below)
- **signed** — interpret wire byte as int8

| Name | Byte | Display range | Conv | Notes |
|---|---|---|---|---|
| SLOT | 0x02 | 1–5 | noop | Active tuning slot |
| SEN | 0x03 | 90–2520° / AUTO | sens | Steering rotation angle |
| FF | 0x04 | 0–100 | noop | Force feedback strength |
| SHO | 0x05 | 0–100 | ×10 | Wheel vibration motor |
| BLI | 0x06 | 0–100 / OFF | noop | Brake Level Indicator; 101 = OFF |
| FFS | 0x07 | 0–1 | noop | FF scaling: 0 = LINEAR, 1 = PEAK |
| DRI | 0x09 | −5 to 3 | signed | Drift mode; CSL Elite only |
| FOR | 0x0A | 0–120 | ×10 | Force effect strength |
| SPR | 0x0B | 0–120 | ×10 | Spring effect strength |
| DPR | 0x0C | 0–120 | ×10 | Damper effect strength |
| NDP | 0x0D | 0–100 | noop | Natural Damping; DD+/DD1/DD2 only |
| NFR | 0x0E | 0–100 | noop | Natural Friction; DD+/DD1/DD2 only |
| BRF | 0x10 | 0–100 | noop | Brake force (load-cell) |
| FEI | 0x11 | 0–100 | noop | Force effect intensity |
| ACP | 0x13 | 1–4 | noop | Analogue Paddles mode |
| INT | 0x14 | 0–20 | noop | FFB interpolation filter; DD+/DD1/DD2 only |
| NIN | 0x15 | 0–100 | noop | Natural Inertia; DD+/DD1/DD2 only |
| FUL | 0x16 | 0–100 | noop | FullForce; CSL DD only |

---

## SEN encoding

The SEN byte encodes steering angle. The encoding depends on the base's
maximum supported rotation range.

**Bases with max rotation ≤ 1090° (CSL Elite, CS V2/V2.5):**
```
degrees = wire_byte × 10
```
Range: wire 0x09–0x6C → 90°–1080°.

**Bases with max rotation > 1090° (CSL DD, DD Pro, DD+, DD1, DD2):**

Three sub-ranges packed into one byte:

| Wire byte | Degrees | Formula |
|---|---|---|
| 0x8A – 0xEC | 90° – 1070° | `90 + 10 × (byte − 0x8A)` |
| 0xED – 0xFF | 1080° – 1260° | `1080 + 10 × (byte − 0xED)` |
| 0x00 – 0x89 | 1270° – 2640° | `1080 + 10 × (256 + byte − 0xED)` |

`0x00` may also appear as **AUTO** (base chooses range from attached wheel).

The host must know the base's `max_range` to distinguish the small-range
formula (`byte × 10`) from the large-range lookup. In the Linux driver,
`max_range` is queried from a separate HID descriptor field during probe.

---

## Opening the device on Windows

```rust
CreateFileW(
    path,
    GENERIC_READ | GENERIC_WRITE,
    FILE_SHARE_READ | FILE_SHARE_WRITE,  // shared — allows FanaLab to coexist
    null,
    OPEN_EXISTING,
    FILE_FLAG_OVERLAPPED,
    0,
)
```

**FanaLab conflict:** FanaLab typically opens the device with exclusive access
(`FILE_SHARE_NONE`). If FanaLab is running, `CreateFileW` will return
`INVALID_HANDLE_VALUE` with `ERROR_SHARING_VIOLATION (32)`. Close FanaLab
before running this tool.

---

## HID collections (PID 0x0020 — CSL DD / ClubSport DD+)

Windows exposes PID 0x0020 as **three separate HID collections**:

| Path suffix | Role | Report size |
|---|---|---|
| `&col01` | Display and ACK burst channel | 8 bytes |
| `col02` | (Varies by firmware) | — |
| `col03` | Tuning + LED control (confirmed on CS DD+) | 64 bytes |

`enumerate_fanatec()` returns all collections. The correct one for tuning must be
found by probing: attempt `WriteFile` and move to the next collection on
`ERROR_INVALID_FUNCTION`. On the ClubSport DD+ this is col03; the probe loop is
order-independent and caches the result in a `OnceLock`.

**col01** is also written to after every tuning write — see *Col01 ACK burst* below.

---

## Windows software locations

**Fanatec SDK DLL:**
```
C:\Program Files\Fanatec\Fanatec Wheel\fw\EndorFanatecSdk64_VS2019.dll
```
This DLL implements the high-level Fanatec API (property get/set, LED
control). It communicates with the base over the same HID collections
described above. Useful reference for future reverse-engineering.

**Fanatec App config:**
```
%APPDATA%\com.example\Fanatec\
```
Stores saved tuning profiles and app preferences.

---

## Wake sequence (required before reads AND writes)

The DD+ requires CMD `0x06` (toggle) before **both reads and writes**.

```
[0xFF, 0x03, 0x06, 0x00 × 61]   ← CMD_TOGGLE_ADVANCED
```

**Important: 0x06 is a TOGGLE, not an enable.** Each call flips the
current state (ON → OFF or OFF → ON). The device starts in an unknown
state relative to a new process, so a single unconditional toggle is
unreliable across consecutive runs.

**For reads (startup probe) — double-toggle with probe:**
1. Send 0x06 toggle + 500 ms + 0x04 request → poll for response
2. If response received → advanced mode is now ON, proceed
3. If no response → we toggled it OFF (it was already ON); send 0x06
   again + 500 ms + 0x04 request → this always succeeds

This converges in ≤ 2 toggles regardless of starting state.

**For writes — one unconditional toggle before every write:**

The toggle opens a write window for exactly one write cycle. The window
expires after the write completes — a second write requires another toggle.
Sending CMD `0x00` (write) without a preceding toggle results in no change
(readback returns the same values). Do not attempt to reuse the write window
across multiple writes.

```
1. Send 0x06 toggle
2. Sleep 500 ms
3. Send CMD 0x00 (write report)
```

The Fanatec App likely issues a fresh toggle before every user-initiated
tuning change for the same reason.

Root cause: the Linux driver (`hid-ftecff.c`) sends this toggle on device
probe, so the Fanatec App pre-wakes the device at startup. Our tool must
do the same when connecting cold, but must not assume the starting state.

---

## Typical read flow

```
1. Open device (shared, overlapped)
2. WriteFile([0xFF, 0x03, 0x06, 0x00 × 61])   ← wake / advanced-mode toggle
3. Sleep 500 ms
4. Drain stale input: loop ReadFile(50ms timeout) until timeout
5. WriteFile([0xFF, 0x03, 0x02, 0x00 × 61])   ← request values (0x02 = READ)
6. ReadFile(buf, 64, overlapped)  with WaitForSingleObject(event, 200ms)
7. Check buf[0]==0xFF && buf[1]==0x03          ← tuning report?
   If not, loop back to step 6 (other HID traffic may arrive first)
8. Decode parameters from buf
```

---

## Typical write flow

The DD+ expects **all parameters in a single report**. Build one 64-byte
buffer with CMD=0x00, copy the current device state shifted by one byte,
overlay the new values, and send once.

```
1. Read current values (see Typical read flow above) → store as base
2. Build write buffer from the read snapshot:
   buf[0]    = 0xFF, buf[1] = 0x03, buf[2] = 0x00 (CMD_WRITE_PARAM)
   buf[3..63] = base[2..62]  (shift read state right by 1)
   buf[3]    = 0x01          ← REQUIRED: see note below
   buf[addr+1] = encoded_value  (for each param to update)
3. WriteFile(write_buf)      ← single write, all params at once
4. Send col01 ACK burst      ← see Col01 ACK burst section below
5. WriteFile([0xFF, 0x03, 0x06, 0x00 × 61])  ← toggle to readable mode
6. Sleep 200 ms
7. Drain stale input: loop ReadFile(50ms timeout) until timeout
8. WriteFile([0xFF, 0x03, 0x02, 0x00 × 61])  ← request values (0x02 = READ)
9. ReadFile → verify
10. WriteFile([0xFF, 0x03, 0x06, 0x00 × 61]) ← toggle back to writable mode
```

**Byte 3 must be `0x01`:** The device silently ignores write reports when
byte 3 is `0x81`. The readback buffer returns either `0x01` or `0x81` in
this position depending on the current toggle state. If copied verbatim into
the write buffer, writes made when the device is in the `0x81` state are
ignored. Always force `buf[3] = 0x01` after the copy. Confirmed empirically
across two separate wheels — this is the single most reliable fix.

**Note:** sending each parameter in its own report (one at a time) does not
work on the DD+ — readback shows no change.

---

## Wheel type identifiers

`.pws` profile files contain a `<Device>` tag with `BaseType` and `WheelType`
attributes identifying which base and rim the profile was tuned for:

```xml
<Device BaseType="12" WheelType="19" SWType="10" PedalType="4" .../>
```

Known values observed so far:

| Attribute | Value | Hardware |
|-----------|-------|----------|
| BaseType | 12 | ClubSport DD+ |
| WheelType | 19 | Observed in CS DD+ profiles |
| WheelType | 15 | Observed in P DD2 profiles |

Full mapping is not yet determined. These values are extracted by the profile
parser (`PwsProfile::base_type` / `::wheel_type`) for future LED and display
control.

**Relevance for LED/ITM control:** profiles contain wheel-specific sections
(`RevLedProfileWheel`, `ButtonLedProfile`, `DisplayLedProfile`, `ITMPRofile`)
that vary by rim. When LED control is implemented, `WheelType` will be matched
against the detected attached rim to select the correct section.

---

## Game IDs

From `HardwareConfiguration.xml`. Each profile and settings block references a
game by `(MajorId, MinorId)`. When `GameMajorId = -1`, the entry is global
(not game-specific).

| Game | MajorId | MinorId |
|------|---------|---------|
| Assetto Corsa | 0 | 0 |
| Assetto Corsa Competizione | 0 | 1 |
| Assetto Corsa EVO | 0 | 2 |
| Assetto Corsa Rally | 0 | 3 |
| Automobilista | 1 | 0 |
| Automobilista 2 | 1 | 1 |
| DiRT Rally | 2 | 0 |
| DiRT Rally 2.0 | 2 | 1 |
| F1 24 | 4 | 10 |
| F1 25 | 4 | 11 |
| iRacing | 5 | 0 |
| Project CARS 2 | 8 | 1 |
| RaceRoom | 10 | 0 |
| rFactor 2 | 11 | 1 |
| Le Mans Ultimate | 11 | 2 |
| Forza Motorsport | 17 | 0 |

---

## LED color index

From `CurrentSettings.xml`. Color index values used in LED configurations:

| ColorIndex | Color |
|-----------|-------|
| 0 | Off |
| 1 | Red |
| 2 | Green |
| 3 | Blue |
| 4 | Yellow |
| 5 | Magenta (unconfirmed) |
| 6 | Cyan (unconfirmed) |
| 7 | White |
| 12 | Orange |

Values 5 and 6 appear in the data but have not been visually verified.

---

## Button LED profiles

Three wheel rim types with distinct button LED layouts, from `CurrentSettings.xml`:

| Profile key | Description |
|-------------|-------------|
| `ButtonLedsBMW` | BMW GT2 rim — 12 buttons, RGB per button |
| `ButtonLedsBMR` | BMW M4 rim — 12 buttons, RGB per button |
| `ButtonLedsGTSWX` | GT SWX rim — 12 buttons, RGB per button + RPM-triggered color cycling |

---

## ITM profiles (In-Telemetry-Monitor)

Wheel-specific display profiles, from `CurrentSettings.xml`:

| Profile key | Target display |
|-------------|----------------|
| `BaseProfile` | Base unit display |
| `BmeProfile` | BMW M4 (BME) wheel |
| `BentleyProfile` | Bentley wheel |
| `GtswxProfile` | GT SWX wheel |
| `SmallOledProfile` | Wheels with a small OLED display |

---

## Telemetry channels

Channel definitions per game, from `Configuration.xml`.

**All games:**
RPM, Speed, Gear, Fuel, Position, LapNumber, Brake, Throttle

**ACC / AC EVO additionally:**
Flags (Yellow, Blue, White, Green, Orange), RainLights, DirectionLights,
TC graphics, ABS graphics, EngineMap, DRS

---

## Fanatec App XML files

Location: `C:\Program Files\Fanatec\FanatecService\Service\xml\`

| File | Contents |
|------|----------|
| `HardwareConfiguration.xml` | Hardware capabilities, game list, `MajorId`/`MinorId` mappings |
| `CurrentSettings.xml` | LED color tables, button LED layouts per rim, ITM display profiles, per-game LED trigger configurations |
| `Configuration.xml` | Telemetry channel definitions per game |
| `CarsList_{game}.xml` | carPath → display name (one file per game) |
| `ProfileCarsList_{game}.xml` | carPath → `.pws` filename (one file per game) |

fanatec-tuner currently reads `CarsList_*.xml` and `ProfileCarsList_*.xml` for
profile matching. `CurrentSettings.xml` and `Configuration.xml` will be used
when LED and display control is implemented.

---

## Col01 ACK burst

After every successful tuning write on col03, send **4 ON/OFF pulse pairs** on
col01 (the display/ACK channel). This matches FanaBridge behaviour and improves
write reliability. Each packet is 8 bytes sent via `WriteFile` (not `HidD_SetOutputReport`).

```
ON:  [0x01, 0xF8, 0x09, 0x01, 0x06, 0xFF, 0x02, 0x00]
OFF: [0x01, 0xF8, 0x09, 0x01, 0x06, 0x00, 0x00, 0x00]
```

Sequence: ON → 1ms → OFF → 1ms → repeat × 4 = 8 packets total.

If col01 cannot be opened (device not present or exclusive access), skip the
burst — writes still succeed without it.

---

## LED protocol (col03)

LED reports share the col03 handle with tuning reports. They are distinguished
by byte 1: tuning uses `0x03`, LED uses `0x01`.

```
Byte 0:  0xFF  — report ID
Byte 1:  0x01  — LED marker
Byte 2:  subcmd
Bytes 3+: payload (subcmd-dependent)
```

| Subcmd | Purpose | Max LEDs | Payload |
|--------|---------|----------|---------|
| `0x00` | Rev LED colors | 9 | RGB565 pairs, big-endian, 2 bytes each |
| `0x01` | Flag LED colors | 6 | RGB565 pairs, big-endian, 2 bytes each |
| `0x02` | Button LED colors | 12 | RGB565 pairs + commit byte at offset 27 |
| `0x03` | Button LED intensities | 16 | 1 byte each (0–7, 3-bit); commit byte at offset 18 |

**RGB565 encoding:** blue occupies bits 15–11, green bits 10–5, red bits 4–0.
Each value is packed big-endian (high byte first) in the report. In code:
```
value = (b5 << 11) | (g6 << 5) | r5
buf[offset]   = (value >> 8) as u8
buf[offset+1] = (value & 0xFF) as u8
```

**Two-phase button LED commit:** to apply colors and intensities atomically, send
colors with `commit=0` (byte 27 = 0x00) then intensities with `commit=1`
(byte 18 = 0x01). The second write triggers the visible update.

Some wheels use `rgb555` encoding (5-5-5, no extra green bit) — see
`colorFormat` in `profiles/wheels/*.json`.

---

## Display protocol (col01)

The 3-digit 7-segment display is driven via the col01 channel using 8-byte
reports. Matches the Linux `hid-fanatecff` driver `ftec_set_display()` and
FanaBridge `DisplayEncoder`.

```
[0x01, 0xF8, 0x09, 0x01, 0x02, seg1, seg2, seg3]
```

Bytes 0–4 are fixed; bytes 5–7 are the 7-segment encoded characters for the
three digit positions (left to right).

**7-segment bit layout:**
```
  seg0  ───
seg5 |     | seg1
  seg6  ───
seg4 |     | seg2
  seg3  ───    • seg7 = decimal point
```

| Value | Meaning |
|-------|---------|
| `0x00` | Blank |
| `0x40` | Dash (`-`) |
| `0x80` | Dot (`.`) |
| `0x3F`–`0x6F` | Digits 0–9 |
| `0x50` | `r` (reverse gear indicator) |
| `0x54` | `n` (neutral gear indicator) |

All three digits are always sent; use `0x00` (blank) for unused positions.
