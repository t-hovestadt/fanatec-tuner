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
| `0x04` | Request current tuning values | 0x00 |
| `0x01` | Select tuning slot | slot number (1–5) |
| `0x00` | Write a single parameter | 0x00 (value at param byte addr) |
| `0x06` | Toggle advanced mode | 0x00 |

**Sending:** use `WriteFile` on the device handle (HID output report).
Do **not** use `HidD_SetFeature` — the driver uses `hid_hw_output_report`,
which maps to a HID OUTPUT report type, not a FEATURE report.

**Receiving:** use `ReadFile` on the same handle (HID interrupt IN report).
The device echoes tuning values whenever the host sends command 0x04 or
the user adjusts a knob on the base.

---

## Parameter byte addresses

Addresses are the wire byte index within the 64-byte report.

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

| Path suffix | Role | Tuning? |
|---|---|---|
| `&col01` | Wheel input (axes, buttons) | No — `WriteFile` returns `ERROR_INVALID_FUNCTION (1)` |
| `col02` | Tuning endpoint | **Yes** — accepts 64-byte output reports |
| `col03` | Unknown (possibly LED / display) | TBD |

`enumerate_fanatec()` returns all three. The correct one for tuning must be
found by probing: attempt `WriteFile` and move to the next collection on
`ERROR_INVALID_FUNCTION`. The successful collection is typically col02 but
the probe loop is order-independent.

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

## Typical read flow

```
1. Open device (shared, overlapped)
2. WriteFile([0xFF, 0x03, 0x04, 0x00 × 61])   ← request values
3. ReadFile(buf, 64, overlapped)  with WaitForSingleObject(event, 1000ms)
4. Check buf[0]==0xFF && buf[1]==0x03          ← tuning report?
   If not, loop back to step 3 (other HID traffic may arrive first)
5. Decode parameters from buf
```

---

## Typical write flow (future)

```
1. Build 64-byte buffer: [0xFF, 0x03, 0x00, 0x00, ...]
2. Set buf[addr] = encoded_value
3. WriteFile(buf)
4. Optionally request values again to verify
```
