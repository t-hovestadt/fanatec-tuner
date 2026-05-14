# Hardware Support

Compatibility information for fanatec-tuner — which wheel bases, steering
wheels, and button modules are known to work.

---

## Wheel Bases

fanatec-tuner enumerates all USB devices with VID `0x0EB7` (Fanatec) and
probes each HID collection to find the one that responds to the 64-byte
tuning protocol. It does not hard-code a specific base model.

| Wheel Base | USB PID | Status |
|------------|---------|--------|
| ClubSport DD+ | `0x0020` | Confirmed — development hardware |
| CSL DD / CSL DD Pro | `0x0020` | Same PID as DD+, expected to work |
| Podium DD1 | `0x0006` | Same protocol, unconfirmed |
| Podium DD2 | `0x0007` | Same protocol, unconfirmed |
| CSL Elite (PC) | `0x0E03` | Different PID; protocol compatibility unconfirmed |

Bases not listed may work if they expose a 64-byte `FF 03` HID collection.
Run `fanatec-tuner diag` to check which collections are detected.

### Collection layout (DD+)

| Collection | Report size | Purpose |
|------------|-------------|---------|
| col03 | 64 bytes | Tuning parameters (FF 03), LED colors (FF 01), save (FF 03 03) |
| col01 | 8 bytes | Display / ACK — gear, status, game-state |

col03 is identified at startup by sending CMD `0x06` (wake toggle) and
checking for a valid 64-byte response. col01 is the 8-byte collection that
shares the same device path prefix.

---

## Steering Wheel LED Support

LED commands (`FF 01 xx`) are relayed to the steering wheel through the base.
The base passes the report to whichever wheel is connected; unsupported wheels
ignore it silently.

| Wheel | Rev LEDs | Button LEDs | Notes |
|-------|----------|-------------|-------|
| Formula V2 / V2.5 | 9 (RGB565) | 12 | Fully supported |
| BMW M4 GT3 Wheel | 9 (RGB565) | 12 | Fully supported |
| F1 Esports V2 | 9 (RGB565) | 12 | Supported; `verify=false` in FanaBridge |
| ClubSport RS Wheel | 9 (RGB565) | 12 | Supported |
| McLaren GT3 V2 | 9 (RGB565) | 12 | Expected — same LED protocol |
| Podium Button Module Endurance (BME) | — | 24 | Standard RGB565 encoding |
| Podium Button Module Rally (BMR) | — | 24 | RGB555 — see note below |
| Porsche 918 RSR Wheel | — | — | Unknown — no data |

**Rev LEDs** — 9 positions, driven by `FF 01 00` subcmd; 2 bytes per LED in
RGB565 big-endian (R=bits 4–0, G=bits 10–5, B=bits 15–11).

**Button LEDs** — sent as two reports: `FF 01 02` (colors, `commit=0`) then
`FF 01 03` (intensities, `commit=1`). The `commit=1` write in the intensity
report triggers the wheel to apply both.

### RGB565 encoding

All wheels except BMR use RGB565:

```
Bits 15–11: Blue  (5 bits)
Bits 10–5:  Green (6 bits)
Bits 4–0:   Red   (5 bits)
```

Common colors:

| Color | RGB565 (hex) |
|-------|-------------|
| Red | `0x00F8` |
| Green | `0xE007` |
| Yellow | `0xE0FF` |
| Blue | `0x1FF8` |
| Magenta | `0x1FF8` → `0x18F8` |
| Orange | `0x00FD` |
| White | `0xFFFF` |
| Off | `0x0000` |

### RGB555 (Podium Button Module Rally only)

The BMR uses RGB555 for button LEDs — the high bit (bit 15) is unused:

```
Bits 14–10: Blue  (5 bits)
Bits 9–5:   Green (5 bits)
Bits 4–0:   Red   (5 bits)
```

fanatec-tuner currently sends RGB565 to all wheels. On BMR, button LED
colors will be slightly off (green channel loses 1 bit of precision). Rev
LEDs are unaffected (BMR has no rev LED strip).

---

## Detection and Collection Probing

At startup, fanatec-tuner:

1. Enumerates all HID devices with VID `0x0EB7` via Windows `SetupDi` API
2. Groups devices by path prefix into "bases"
3. For each base, opens every HID collection and sends CMD `0x06` (wake
   toggle) followed by CMD `0x04` (read request)
4. The collection that returns a valid 64-byte response is selected as col03
   (tuning + LED)
5. A second pass finds the 8-byte collection (col01, display/ACK) sharing
   the same device path
6. If no base responds within 60 seconds, the tool exits with an error

To inspect what is detected:

```
fanatec-tuner diag
```

This dumps all `0x0EB7` devices, their HID capabilities (report sizes,
usage page/ID), and which collection responded to the tuning probe.

---

## Known Limitations

**FWPnpService required**
The Fanatec PnP service (`FWPnpService`) must be running for HID enumeration
to succeed. Do not stop it. Verify in `services.msc`.

**Fanatec App conflict**
The Fanatec App (`Fanatec.exe`) holds exclusive write handles to col03. Close
it completely (system tray → exit) before running any fanatec-tuner write
command. The Fanatec App writes both tuning and LED simultaneously; if it is
running alongside fanatec-tuner, parameters will be overwritten.

**Administrator requirement**
HID write access on some systems requires running as administrator.
Right-click `fanatec-tuner.exe` → Run as administrator, or set the exe to
always run elevated in Properties → Compatibility.

**RGB555 button module**
The Podium Button Module Rally (BMR) uses RGB555 encoding. fanatec-tuner
sends RGB565; button LED colors will be slightly off (green loses 1 bit).
Rev LED behavior is unaffected.

**CSL Elite legacy protocol**
The CSL Elite (PID `0x0E03`) predates the DD-generation 64-byte `FF 03`
protocol. It may use an older 8-byte report format (`FF 60` / `FF 61`).
fanatec-tuner does not implement this protocol; compatibility is unknown.

**Non-Windows**
fanatec-tuner is Windows-only. The HID layer uses `windows-sys` for
`SetupDi` enumeration and `CreateFile` / `WriteFile` for raw HID access.
iRacing telemetry is read from Windows shared memory
(`Local\IRSDKMemMapFileName`). The binary cross-compiles from macOS/Linux
using `x86_64-pc-windows-gnu` but the resulting exe only runs on Windows.
