# Fanatec HID Protocol Reference

Technical reference for the USB HID protocol used by Fanatec DD-series wheel bases.
Derived from reverse-engineering: [hid-fanatecff](https://github.com/gotzl/hid-fanatecff)
Linux kernel driver, FanaBridge SimHub plugin (C#), and direct hardware verification on a
ClubSport DD+ (PID 0x0020).

---

## Overview

The Fanatec wheel base exposes multiple USB HID collections under vendor ID `0x0EB7`.
On the ClubSport DD+ (and CSL DD), Windows enumerates three collections:

| Collection | Role | Report size |
|-----------|------|-------------|
| `col01` | Display, ACK bursts, wheel input | 8 bytes |
| `col02` | (Firmware-dependent; typically unused) | — |
| `col03` | Tuning writes/reads, LED control, hardware status, protobuf config | 64 bytes |

**col03** is the primary channel for all sim-racing integrations. It handles:
- Tuning parameter reads and writes (`FF 03`)
- Rev, flag, and button LED colors (`FF 01`)
- Hardware status queries (`FF 08`)
- Protobuf config reporting (`FF 10`)

**col01** is written to for the ACK burst after each tuning write, and for 7-segment
display updates. It also delivers wheel input (steering angle, button state) as interrupt
IN reports.

All reports on col03 are exactly **64 bytes**. Reports on col01 are exactly **8 bytes**.

Finding the correct collection: enumerate all `0x0EB7` device paths, attempt `WriteFile`
on each, accept the one that does not return `ERROR_INVALID_FUNCTION`. On the CS DD+
this is always `col03`. Cache the result — no need to re-probe on subsequent writes.

---

## Tuning Parameters — `FF 03`

### Report Structure

```
col03, 64 bytes:
  byte[0] = 0xFF   report ID
  byte[1] = 0x03   tuning marker
  byte[2] = cmd    see Commands table below
  byte[3] = arg    command-dependent
  bytes[4..63]     parameter data / padding
```

### Commands (byte[2])

| Value | Purpose | byte[3] |
|-------|---------|---------|
| `0x00` | **Write** all tuning parameters | `0x01` (devId — must not be `0x81`) |
| `0x01` | Select tuning slot | slot 1–5 |
| `0x02` | **Read** current tuning values | `0x00` |
| `0x03` | **Save** current state to firmware flash | `0x00` |
| `0x04` | Factory reset (destructive — do not use) | `0x00` |
| `0x06` | Toggle advanced/writable mode | `0x00` |

The `0x06` toggle is required before every read and write (see Wake Sequence).

**devId** for byte[3] of write: `0x01` = wheel base, `0x02` = Podium Button Module
(BMR/BME). Always force this to `0x01`; the read response returns `0x01` or `0x81`
depending on toggle state — if `0x81` is copied verbatim into the write buffer, the
write is silently ignored. This is the single most common failure mode.

### Read/Write Offset Convention

Parameters in the **read response** occupy byte addresses starting at offset 2.
In the **write buffer**, each parameter sits at `read_offset + 1` because byte[2]
holds the CMD byte, pushing everything right by one:

```
Read response:  [FF][03][SLOT][SEN][FF][SHO]...   (slot at byte 2)
Write command:  [FF][03][00][SLOT][SEN][FF][SHO]... (cmd=0x00 at byte 2, slot at byte 3)
```

`FanaBridge.BuildWriteBuffer()` copies `readBuf[2..63] → writeBuf[3..64]` exactly.
fanatec-tuner does the same in `build_full_write_report()`.

### Parameter Byte Addresses (read offsets)

Values apply to the read response. For wire encoding:
- **noop** — display value = wire byte (no scaling)
- **×10** — display value = wire byte × 10
- **sens** — SEN special encoding (see SEN section)
- **signed** — interpret as int8

| Name | Read byte | Display range | Encoding | Notes |
|------|-----------|--------------|---------|-------|
| SLOT | 0x02 | 1–5 | noop | Active tuning slot |
| SEN | 0x03 | 90–2520° / AUTO | sens | Steering rotation angle |
| FF | 0x04 | 0–100 | noop | Force feedback strength |
| SHO | 0x05 | 0–100 | ×10 | Wheel vibration motor |
| BLI | 0x06 | 0–100 / OFF | noop | Brake Level Indicator; 101 = OFF |
| FFS | 0x07 | 0–1 | noop | FF scaling: 0=LINEAR, 1=PEAK |
| DRI | 0x09 | −5 to 3 | signed | Drift mode; CSL Elite only |
| FOR | 0x0A | 0–120 | ×10 | Force effect strength |
| SPR | 0x0B | 0–120 | ×10 | Spring effect strength |
| DPR | 0x0C | 0–120 | ×10 | Damper effect strength |
| NDP | 0x0D | 0–100 | noop | Natural Damping; DD+/DD1/DD2 only |
| NFR | 0x0E | 0–100 | noop | Natural Friction; DD+/DD1/DD2 only |
| BRF | 0x10 | 0–100 | noop | Brake force (load cell) |
| FEI | 0x11 | 0–100 | noop | Force effect intensity |
| ACP | 0x13 | 1–4 | noop | Analogue Paddles mode |
| INT | 0x14 | 0–20 | noop | FFB interpolation filter; DD+/DD1/DD2 only |
| NIN | 0x15 | 0–100 | noop | Natural Inertia; DD+/DD1/DD2 only |
| FUL | 0x16 | 0–100 | noop | FullForce; CSL DD only |

### SEN Encoding

SEN encodes steering angle. Two schemes depending on max rotation range:

**Bases with max ≤ 1090° (CSL Elite, CS V2/V2.5):**
```
degrees = wire_byte × 10       (range 0x09–0x6C → 90°–1080°)
```

**Bases with max > 1090° (CS DD, DD Pro, DD+, DD1, DD2):**

Three sub-ranges packed into one byte:

| Wire byte | Degrees | Formula |
|-----------|---------|---------|
| 0x8A–0xEC | 90°–1070° | `90 + 10 × (byte − 0x8A)` |
| 0xED–0xFF | 1080°–1260° | `1080 + 10 × (byte − 0xED)` |
| 0x00–0x89 | 1270°–2640° | `1080 + 10 × (256 + byte − 0xED)` |

`0x00` may also appear as **AUTO** — the base selects range from the attached wheel.
The host must know `max_range` to choose the correct formula.

### Wake Sequence (Required Before Every Read or Write)

The device starts in an unknown toggle state relative to a new process. CMD `0x06`
is a toggle — each call flips the current mode. It must precede reads and writes.

```
[0xFF, 0x03, 0x06, 0x00 × 61]   64-byte wake report
```

**For reads (startup probe) — double-toggle with probe:**
1. Send `0x06` + wait 500 ms + send `0x02` (read request)
2. If response arrives → advanced mode is ON; proceed
3. If no response within 1000 ms → mode was already ON and is now OFF; send `0x06` again,
   wait 500 ms, send `0x02` → this always succeeds

**For writes — one unconditional toggle immediately before each write:**
The toggle opens a one-shot write window. Sending `0x00` without a preceding toggle
results in no change. Do not reuse the window across multiple writes.

```
1. WriteFile: [FF 03 06 00...] — toggle
2. Sleep 500 ms
3. WriteFile: [FF 03 00 01 ...params...] — write all params at once
```

**FWPnpService dependency:** reads fail if the Fanatec driver service is not running.
The `FWPnpService` is installed with the Fanatec driver; do not stop it.

**Drain stale input before reads** (`DRAIN_TIMEOUT_MS = 50`): loop `ReadFile` with
50 ms timeout until timeout fires, discarding any stale reports buffered since the
last operation.

### Typical Read Flow

```
1. Open device (GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE, overlapped)
2. WriteFile [FF 03 06 00...] — toggle
3. Sleep 500 ms
4. Drain: loop ReadFile(50ms) until timeout
5. WriteFile [FF 03 02 00...] — read request
6. ReadFile(64 bytes, WaitForSingleObject 200ms)
7. If buf[0]≠FF or buf[1]≠03: discard and loop to 6 (other HID traffic may arrive)
8. Decode parameters from buf[2..]
```

### Typical Write Flow

Build one 64-byte buffer, overlay the desired parameters, send once. The DD+ requires
all parameters in a single report — one-at-a-time writes do not work.

```
1. Read current values → snapshot
2. buf[0]=0xFF, buf[1]=0x03, buf[2]=0x00 (CMD_WRITE)
3. Copy snapshot[2..62] → buf[3..63]
4. Force buf[3]=0x01 (devId — never copy 0x81)
5. Overlay each changed param at buf[read_offset + 1]
6. WriteFile: write_buf
7. WriteFile on col01: ACK burst (see below)
8. (optional verify) toggle+drain+read+compare
```

---

## Rev LED Colors — `FF 01 00`

```
col03, 64 bytes:
  buf[0] = 0xFF
  buf[1] = 0x01
  buf[2] = 0x00   SUBCMD_REV
  buf[3..4]  = LED 1 RGB565 big-endian (high byte at buf[3])
  buf[5..6]  = LED 2
  ...
  buf[19..20] = LED 9
  buf[21..63] = 0x00
```

**9 LEDs**, numbered 1–9 left-to-right on the strip. Each color is 2 bytes,
big-endian RGB565 (see RGB565 Encoding below). A value of `0x0000` is off.

Send at 30 Hz during active driving. Use dirty-check — skip the write when colors
have not changed since the last frame.

**CSL Elite incompatibility:** The `F8 13` bitmask protocol from hid-fanatecff is
for CSL Elite only (PID 0x0E03/0x0005). The DD+ (0x0020) does not have the
`FTEC_WHEELBASE_LEDS` quirk and does not respond to `F8 13`. Use `FF 01 00` only.

---

## Flag LED Colors — `FF 01 01`

```
col03, 64 bytes:
  buf[0] = 0xFF
  buf[1] = 0x01
  buf[2] = 0x01   SUBCMD_FLAG
  buf[3..4]  = Flag LED 1 RGB565 big-endian
  buf[5..6]  = Flag LED 2
  ...
  buf[13..14] = Flag LED 6
  buf[15..63] = 0x00
```

**6 LEDs**. Typically reflect race flag state (yellow, green, red, blue, checkered).

---

## Button LED Colors — `FF 01 02`

```
col03, 64 bytes:
  buf[0] = 0xFF
  buf[1] = 0x01
  buf[2] = 0x02   SUBCMD_BTN_COLORS
  buf[3..4]  = Button 1 RGB565 big-endian
  ...
  buf[27..28] would be Button 13 — but buf[27] is the commit byte
  buf[27] = commit: 0x00=stage, 0x01=apply immediately
  buf[28..63] = 0x00
```

**12 buttons**, RGB565 per button. Commit byte at offset 27 (HEADER=3 + 12 × 2 = 27).

**Two-phase commit:** send colors with `commit=0x00` first, then intensities with
`commit=0x01`. This makes colors and intensities take effect atomically in one visible
update. If only intensities change, the intensity report can be sent directly with
`commit=0x01`.

---

## Button LED Intensity — `FF 01 03`

```
col03, 64 bytes:
  buf[0] = 0xFF
  buf[1] = 0x01
  buf[2] = 0x03   SUBCMD_BTN_INTENSITIES
  buf[3..17] = 15 × intensity (0–7, 3-bit each)
  buf[18] = commit: 0x00=stage, 0x01=apply
  buf[19..63] = 0x00
```

Up to 16 intensity bytes; commit byte at offset 18 (HEADER=3 + 15 = 18). Values
are 3-bit (0=off, 7=max). Encoder knobs (hwIndex 12–14 on PSWBMW) also use this
payload at their respective hwIndex positions.

**Podium Button Module Rally (PHUB_PBMR):** uses RGB555 instead of RGB565 for
button colors. Green is quantized to 5 bits (`g >> 3`) and the MSB of the 6-bit
green field is always zero. The `colorFormat` field in wheel JSON profiles signals
this variant.

---

## Save — `FF 03 03`

```
col03, 64 bytes:
  buf[0] = 0xFF
  buf[1] = 0x03
  buf[2] = 0x03   CMD_SAVE
  buf[3..63] = 0x00
```

Persists the current LED configuration (color palette, button colors/brightness) to
firmware flash. Send after each LED config write. Without this, the config reverts
on power cycle.

---

## Display — `col01`

The 3-digit 7-segment display uses 8-byte reports on col01.

```
col01, 8 bytes:
  buf[0] = 0x01   Report ID (Windows requires this as first byte)
  buf[1] = 0xF8
  buf[2] = 0x09
  buf[3] = 0x01
  buf[4] = 0x02
  buf[5] = seg1   left digit
  buf[6] = seg2   center digit
  buf[7] = seg3   right digit
```

**7-segment bit encoding:**

```
    seg0  ───
seg5 |     | seg1
    seg6  ───
seg4 |     | seg2
    seg3  ───     (seg7 = decimal point)
```

| Byte value | Character |
|-----------|-----------|
| `0x00` | Blank |
| `0x3F` | 0 |
| `0x06` | 1 |
| `0x5B` | 2 |
| `0x4F` | 3 |
| `0x66` | 4 |
| `0x6D` | 5 |
| `0x7D` | 6 |
| `0x07` | 7 |
| `0x7F` | 8 |
| `0x6F` | 9 |
| `0x40` | `-` (dash) |
| `0x80` | `.` (dot) |
| `0x50` | `r` (reverse) |
| `0x54` | `n` (neutral) |

All three digit positions are always sent; use `0x00` for blank positions.

Linux `hid-fanatecff` sends the same payload without the leading `0x01` Report ID
byte because sysfs strips it; Windows HID requires the Report ID as byte[0].

---

## ACK Burst — col01

After every successful tuning write on col03, send **4 ON/OFF pulse pairs** on col01.
This matches FanaBridge behavior and improves write reliability.

```
ON:  [0x01, 0xF8, 0x09, 0x01, 0x06, 0xFF, 0x02, 0x00]
OFF: [0x01, 0xF8, 0x09, 0x01, 0x06, 0x00, 0x00, 0x00]

Pattern: ON → 1ms → OFF → 1ms, repeated 4 times (8 total packets)
```

`ACK_BURST_COUNT = 4`, `ACK_BURST_DELAY_MS = 1`. Byte[4] = `0x06` is the ACK
subcmd constant (`COL01_TUNING_ACK` in FanaBridge).

If col01 cannot be opened, skip the burst — tuning writes still succeed without it.

---

## Hardware Status — `FF 08`

```
col03:
  buf[0] = 0xFF
  buf[1] = 0x08
  buf[2] = 0x0C   BaseType (0x0C = 12 = ClubSport DD+)
  ...
```

`FF 08` reports arrive periodically as interrupt IN reports and also in response to
certain queries. This is a fixed binary format (not protobuf). BaseType 12 (`0x0C`)
identifies the CS DD+.

---

## Protobuf Config — `FF 10`

```
col03:
  buf[0] = 0xFF
  buf[1] = 0x10
  buf[2] = length   total protobuf payload length
  buf[3..N] = protobuf payload
  buf[N+1..] = trailer bytes
```

Three variants observed by length:

| Length | Context |
|--------|---------|
| `0x0E` (14) | Startup / idle state report |
| `0x16` (22) | Active gameplay — contains current tuning values |
| `0x1C` (28) | Extended status (e.g., after advanced-mode toggle) |

The `FF 10 0x16` variant carries the current tuning state in protobuf encoding.
It is sent periodically (~every 14 seconds) as a status heartbeat from the base.
`sniff --decode` formats decoded fields as `[f1=N  f2=M*  f3=P]` where `*`
marks fields that changed from the previous report.

---

## RGB565 Encoding

All modern DD-series LED reports use BGR565 ("RGB565" by Fanatec convention):
- Bits 15–11: Blue (5 bits)
- Bits 10–5: Green (6 bits)
- Bits 4–0: Red (5 bits)

Packed **big-endian** (high byte first) in the HID report:

```rust
// Rust (fanatec-tuner led.rs):
fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16 & 0x1F;
    let g6 = (g >> 2) as u16 & 0x3F;
    let b5 = (b >> 3) as u16 & 0x1F;
    (b5 << 11) | (g6 << 5) | r5
}

// Packing into report buffer:
buf[offset]   = (color >> 8) as u8;   // high byte first
buf[offset+1] = (color & 0xFF) as u8;
```

```csharp
// C# (FanaBridge ColorHelper — byte-for-byte identical):
ushort r5 = (ushort)((r >> 3) & 0x1F);
ushort g6 = (ushort)((g >> 2) & 0x3F);
ushort b5 = (ushort)((b >> 3) & 0x1F);
return (ushort)((b5 << 11) | (g6 << 5) | r5);
```

**Common color values:**

| Color | RGB | RGB565 (u16) | Bytes |
|-------|-----|--------------|-------|
| Red | 255,0,0 | 0x001F | `00 1F` |
| Green | 0,255,0 | 0x07E0 | `07 E0` |
| Blue | 0,0,255 | 0xF800 | `F8 00` |
| Yellow | 255,255,0 | 0x07FF | `07 FF` |
| Orange | 255,165,0 | 0x0517 | `05 17` |
| Magenta | 255,0,255 | 0xF81F | `F8 1F` |
| White | 255,255,255 | 0xFFFF | `FF FF` |
| Off | 0,0,0 | 0x0000 | `00 00` |

**RGB555 variant (Podium Button Module Rally):** Green is reduced to 5 bits
(`g >> 3`) and the MSB of the 6-bit green field is always zero. This is only
needed for the PHUB_PBMR button module's `FF 01 02` color reports.

---

## Legacy LED Protocols (col01 only — NOT for DD+)

These protocols target CSL Elite rims via col01. They are **not supported** by
the DD+ (PID 0x0020). Listed for completeness only.

| byte[4] | Protocol | Target wheels |
|---------|---------|---------------|
| `0x02` | `[01 F8 09 02 enable 00 00 00]` — GlobalEnable | All legacy rims |
| `0x06` | `[01 F8 09 06 !enable 00 00 00]` — RevStripeEnable (inverted) | RevStripe wheels |
| `0x08` | `[01 F8 09 08 data_lo data_hi 00 00]` — bitmask or RGB333 | BMW M3 GT2, Porsche 918 |
| `0x0A` | `[01 F8 09 0A d0 d1 d2 d3]` — per-LED 3-bit color | GT DD PRO |
| `0x0B` | `[01 F8 09 0B d0 d1 d2 d3]` — per-LED 3-bit flag | (same wheels as 0x0A) |

**RevStripe** (CSLESWP1X, CSLESWP1PS4, CSLESWWRC): Single RGB333 color for entire
strip via subcmd 0x08 after 0x06 enable. RGB333 packs 3 bits per channel into one byte.

The `F8 13` bitmask protocol in hid-fanatecff is gated by `FTEC_WHEELBASE_LEDS`;
DD+ does not have this quirk flag and ignores `F8 13` reports entirely.

---

## Base Compatibility

| PID | Device | Tuning | Rev LEDs (FF 01 00) | F8 13 bitmask | Notes |
|-----|--------|--------|---------------------|---------------|-------|
| 0x0005 | CSL Elite PS4 | Yes | Yes | Yes | WHEELBASE_LEDS quirk |
| 0x0006 | Podium DD1 | Yes | Likely | No | No WHEELBASE_LEDS |
| 0x0007 | Podium DD2 | Yes | Likely | No | No WHEELBASE_LEDS |
| 0x0E03 | CSL Elite (PC) | Yes | Yes | Yes | WHEELBASE_LEDS quirk |
| **0x0020** | **CSL DD / DD+** | **Yes** | **Yes (confirmed)** | **No** | No WHEELBASE_LEDS |

All DD-series bases (DD+, DD1, DD2, CSL DD) use the same `FF 01 xx` col03 protocol
for LED colors. Only CSL Elite uses `F8 13`.

---

## Wheel Type Capabilities

From FanaBridge wheel profile JSON files. `WheelType` from `.pws` `<Device WheelType="N">`:

| Profile ID | Wheel | Rev LEDs | Flag LEDs | Button LEDs | Color format |
|-----------|-------|----------|-----------|-------------|--------------|
| CSSWFORMV2 | Formula V2.5 | 9 × RGB565 | 6 × RGB565 | — | rgb565 |
| CSSWFORMV3 | Formula V3 | 9 × RGB565 | 6 × RGB565 | — | rgb565 |
| CSSWF1ESV2 | F1 Esports V2 | 9 × RGB565 | 6 × RGB565 | — | rgb565 |
| GTSWX | GT Extreme | 9 × RGB565 | 6 × RGB565 | 4 × RGB565 | rgb565 |
| PHUB_PBME | Podium Hub + BME | 9 × RGB565 | 6 × RGB565 | — | rgb565 |
| PHUB_PBMR | Podium Hub + BMR | — | — | 12 × RGB555 | **rgb555** |
| PSWBMW | Podium SW BMW M4 GT3 | — | — | 12 × RGB565 | rgb565 |
| PSWBENT | Podium SW Bentley GT3 | 9 × RGB565 | 6 × RGB565 | — | rgb565 |
| CSSWBMW | BMW M3 GT2 | 9 × legacyOnOff | — | — | legacy (col01) |
| CSSWPORSCHE | Porsche 918 RSR | 9 × legacyOnOff | — | — | legacy (col01) |
| GTSWPRO | GT DD PRO Wheel | 9 × legacy3Bit | — | — | legacy (col01) |
| CSLESWP1X | CSL ES P1 (Xbox) | 1 × legacyStripe | — | — | legacy (col01) |
| CSLESWWRC | CSL ES WRC | 1 × legacyStripe | — | — | legacy (col01) |

Wheels with `legacyOnOff`, `legacy3Bit`, or `legacyStripe` use col01 subcmds 0x06–0x0B
and cannot use the col03 `FF 01 xx` protocol.

**BaseType mapping** (from `.pws` `<Device BaseType="N">`):

| BaseType | Device |
|---------|--------|
| 12 | ClubSport DD+ |
| (others TBD) | — |

---

## iRacing Telemetry (IRSDK Shared Memory)

For real-time LED animation, read from `Local\IRSDKMemMapFileName`.

**Header offsets (little-endian i32):**

| Offset | Field | Notes |
|--------|-------|-------|
| 4 | status | bit 0 = irsdk_stConnected |
| 8 | tickRate | Loop rate in Hz |
| 12 | sessionInfoUpdate | Increments when YAML changes |
| 16 | sessionInfoLen | Length of YAML blob |
| 20 | sessionInfoOffset | Byte offset to YAML blob |
| 24 | numVars | Count of telemetry variables |
| 28 | varHeaderOffset | Byte offset to variable header array |
| 48 | bufDesc[0].tickCount | First buffer tick count (i32) |
| 52 | bufDesc[0].offset | First buffer data offset (i32) ← live data here |

**Variable header** (144 bytes each, starting at `varHeaderOffset`):

| Offset | Type | Field |
|--------|------|-------|
| 0 | i32 | type: 0=char, 1=bool, 2=int, 3=bitField, 4=float, 5=double |
| 4 | i32 | offset into live data buffer |
| 16 | char[32] | name (null-terminated) |
| 48 | char[64] | description |
| 112 | char[32] | unit |

**Key variables:**

| Name | Type | Description |
|------|------|-------------|
| `RPM` | float | Engine RPM |
| `ShiftRPM` | float | Upshift target RPM |
| `RevLimiter` | float | Hard rev limit |
| `Gear` | int | −1=R, 0=N, 1+=forward |
| `Speed` | float | Speed in m/s |
| `SessionFlags` | bitField | Race flag bitmask |
| `IsOnTrack` | bool | Player is on track |
| `IsInGarage` | bool | Player is in garage |

**SessionFlags bitmask:**

| Flag | Bit |
|------|-----|
| Checkered | 0x00000001 |
| White | 0x00000002 |
| Green | 0x00000004 |
| Yellow | 0x00000008 |
| Red | 0x00000010 |
| Blue | 0x00000020 |
| Yellow Waving | 0x00000100 |
| Caution | 0x00002000 |
| Caution Waving | 0x00004000 |
| Black | 0x00008000 |

**Session YAML shift light fields** (under `DriverInfo`):
- `DriverCarSLFirstRPM` — RPM at which first LED activates
- `DriverCarSLLastRPM` — RPM at which last LED activates
- `DriverCarSLShiftRPM` — RPM to upshift (threshold for flash)
- `DriverCarSLBlinkRPM` — RPM for blink warning

---

## Opening the Device on Windows

```rust
CreateFileW(
    path,
    GENERIC_READ | GENERIC_WRITE,
    FILE_SHARE_READ | FILE_SHARE_WRITE,   // shared — allows coexistence
    null_mut(),
    OPEN_EXISTING,
    FILE_FLAG_OVERLAPPED,
    0,
)
```

Use `FILE_SHARE_READ | FILE_SHARE_WRITE` to allow coexistence with the Fanatec
App. FanaLab typically opens with exclusive access; if it is running,
`CreateFileW` returns `INVALID_HANDLE_VALUE` / `ERROR_SHARING_VIOLATION (32)`.

Use `WriteFile` for all HID output (not `HidD_SetFeature`). The Fanatec driver
uses `hid_hw_output_report`, which maps to HID OUTPUT reports, not FEATURE reports.

---

## Game IDs

From `HardwareConfiguration.xml`. `GameMajorId = -1` = global (not game-specific).

| Game | MajorId | MinorId |
|------|---------|---------|
| Assetto Corsa | 0 | 0 |
| Assetto Corsa Competizione | 0 | 1 |
| Assetto Corsa EVO | 0 | 2 |
| Assetto Corsa Rally | 0 | 3 |
| Automobilista 2 | 1 | 1 |
| DiRT Rally 2.0 | 2 | 1 |
| F1 24 | 4 | 10 |
| F1 25 | 4 | 11 |
| iRacing | 5 | 0 |
| rFactor 2 | 11 | 1 |
| Le Mans Ultimate | 11 | 2 |
| Forza Motorsport | 17 | 0 |

---

## Fanatec App XML Files

Location: `C:\Program Files\Fanatec\FanatecService\Service\xml\`

| File | Contents |
|------|---------|
| `HardwareConfiguration.xml` | Hardware capabilities, game list, MajorId/MinorId |
| `CurrentSettings.xml` | LED color tables, button LED layouts, ITM display profiles |
| `Configuration.xml` | Telemetry channel definitions per game |
| `CarsList_{game}.xml` | carPath → display name |
| `ProfileCarsList_{game}.xml` | carPath → `.pws` filename |

fanatec-tuner reads `CarsList_*.xml` and `ProfileCarsList_*.xml` for profile
matching. `HardwareConfiguration.xml` and `CurrentSettings.xml` contain wheel
capability tables and LED color index definitions.

---

## ColorIndex Mapping

From `CurrentSettings.xml` LED configurations (used in `.pws` `RevLedProfileWheel`):

| Index | Color |
|-------|-------|
| 0 | Off |
| 1 | Red |
| 2 | Green |
| 3 | Blue |
| 4 | Yellow |
| 5 | Magenta (unconfirmed) |
| 6 | Cyan (unconfirmed) |
| 7 | White |
| 12 | Orange |
