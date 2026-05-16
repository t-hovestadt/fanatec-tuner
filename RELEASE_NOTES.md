Automatic tuning profile loader and LED controller for Fanatec wheel bases.
Direct HID communication — no Fanatec App required during a session.

## Downloads

| File | Description |
|------|-------------|
| `fanatec-tuner.exe` | Windows x64 binary |

## What's new in v0.1.3

### LED cleanup on game exit / car switch / shutdown

When iRacing exits or you Ctrl-C, fanatec-tuner now sends direct HID LED-off
commands (`FF 01 00` with all-zero payload) to the wheel base firmware. This
clears rev, flag, and button LEDs immediately — they no longer stay stuck
after a session ends.

LED cleanup fires in three situations:
- **Car switch** — LEDs cleared before the new profile is applied (prevents
  color bleed from the previous car's palette)
- **Game exit** — cleared when the iRacing process disappears
- **Ctrl-C / shutdown** — cleared before the process exits

### CPU 0 exclusion

fanatec-tuner now automatically avoids CPU 0 on startup via
`SetProcessAffinityMask`. iRacing's sim thread is hardcoded to CPU 0; running
on the same core causes Type B stutters (15–27 ms frame time spikes). Use
`--no-cpu-exclude` to disable if not running alongside iRacing.

### CI releases

Tag pushes (`v*`) now automatically create a GitHub Release with
`fanatec-tuner.exe` attached. No more manually downloading CI artifacts.

## Features

- **Monitor mode** — auto-detect iRacing car changes, apply matching `.pws`
  profile on every session entry
- **Full profile load** — tuning (FF, SEN, NDP, FEI, …) + rev LED colors +
  button LED colors/brightness + flag LED defaults written on each car change
- **Real-time rev LED animation** — 30 Hz RPM-driven LED strip from iRacing
  shared memory; per-car color profiles and flash patterns; no Fanatec App
  needed
- **70 verified per-car LED profiles** sourced from
  [Lovely Car Data](https://github.com/Lovely-Sim-Racing/lovely-car-data)
  (CC BY-NC-SA 4.0) with per-LED colors and per-gear RPM thresholds
- **LED gating** — only cars with verified profiles get LED commands; all
  other cars receive tuning only
- **LED cleanup** — direct HID LED-off on car switch, game exit, and shutdown
- **XML car list matching** — exact `carPath → .pws` mapping via Fanatec App
  XML files; fuzzy fallback for edge cases
- **Sniff mode** — HID traffic analyzer with `--decode`
- **CPU 0 exclusion** — avoids competing with iRacing's sim thread

## Prerequisites

- Fanatec wheel base connected via USB, driver installed, `FWPnpService` running
- iRacing installed on the same PC
- `.pws` profile files from Maurice's FanaLab pack or Fanatec App exports

See the [README](https://github.com/t-hovestadt/fanatec-tuner/blob/main/README.md)
for full setup instructions.
