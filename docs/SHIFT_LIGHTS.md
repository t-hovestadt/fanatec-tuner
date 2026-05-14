# Shift Lights

Per-car LED profile catalog for fanatec-tuner. Profiles are sourced from
iRacing user manuals (Batch 1: 27 cars; Batch 2: 29 additional cars).

---

## Overview

fanatec-tuner embeds `data/car_led_profiles.json` at compile time. On every
car change, the active car's `carPath` (from iRacing shared memory) is matched
against each profile's `iracing_car_paths` list. The first match wins; if no
match is found, the `_default` profile is used (Green → Yellow → Red,
left-to-right, flash red).

The LED strip on the steering wheel has **9 positions** numbered left to right:

```
  1    2    3    4    5    6    7    8    9
 [·]  [·]  [·]  [·]  [·]  [·]  [·]  [·]  [·]
left                                      right
```

---

## Fill Patterns

### `left_right`
LEDs illuminate sequentially from left to right (position 1 → 9). The most
common pattern. Each LED activates at a progressively higher RPM threshold.

```
Stage 1 (e.g. Green):   [G][ ][ ][ ][ ][ ][ ][ ][ ]
Stage 2 (e.g. Yellow):  [G][G][G][Y][Y][ ][ ][ ][ ]
Stage 3 (e.g. Red):     [G][G][G][Y][Y][Y][R][R][R]
```

### `outside_center`
LED pairs illuminate from the outer edges inward simultaneously:
`[1,9]` → `[2,8]` → `[3,7]` → `[4,6]` → `[5]`. Used by cars whose
real steering wheel displays LEDs symmetrically (MX-5 Cup, GT4 cars,
Gen3 Supercars).

```
Stage 1 (Green):   [G][ ][ ][ ][ ][ ][ ][ ][G]
Stage 2 (Yellow):  [G][Y][ ][ ][ ][ ][ ][Y][G]
Stage 3 (Red):     [G][Y][Y][ ][ ][ ][Y][Y][G]
Full (pre-flash):  [G][Y][Y][R][ ][R][Y][Y][G]
Center:            [G][Y][Y][R][R][R][Y][Y][G]
```

### `all_at_once`
All 9 LEDs illuminate simultaneously in a single color as the shift
warning approaches. Used by cars with a simple on/off LED indicator
rather than a sequential strip (McLaren 720S GT3, Mercedes-AMG F1 W12/W13).

```
Pre-shift:    [ ][ ][ ][ ][ ][ ][ ][ ][ ]
At threshold: [B][B][B][B][B][B][B][B][B]   (e.g. Blue)
```

### `none`
No steering wheel shift LED strip. fanatec-tuner sends no LED commands
for these cars; the wheel LEDs remain at whatever state was last set.
Applies to all NASCAR stock cars, oval, and most dirt cars.

---

## Flash Behavior

When RPM exceeds `flash_threshold` (from the `.pws` profile's
`RevLedProfile`), all currently-lit LEDs switch to `flash_color` and
blink at `flash_hz`. fanatec-tuner drives this at 30 Hz from iRacing
shared memory.

- Flash color is car-specific (see catalog below) — many cars flash **red**
  (generic shift warning), while others use **blue** (Porsche, Cadillac GTP,
  Dallara IR18, Aston Martin GT4).
- The `flash_threshold` comes from the `.pws` profile, not the JSON catalog.
  The JSON records the *color* of the flash, not the RPM threshold.

---

## Pit Limiter

When `SessionFlags & 0x10000` (pit speed limiter active in iRacing), if the
profile has a `pit_limiter_color`, fanatec-tuner switches the LEDs to that
color. If `pit_limiter_flash` is `true`, the LEDs blink at `flash_hz`.

A `—` in the Pit Limiter column means no LED change during pit limiter
(either the car has no pit limiter LEDs, or the indicator is screen-only).

---

## Per-Car Catalog

109 cars with active LED profiles (sorted alphabetically). For fill direction
details see [Fill Patterns](#fill-patterns) above.

| Car | Pattern | Colors | Flash | Pit Limiter |
|-----|---------|--------|-------|-------------|
| Acura Arx-06 Gtp | left right | Green → Yellow → Red | Red | Magenta (flash) |
| Acura Nsx Gt3 Evo 22 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Aston Martin Dbr9 Gt1 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Aston Martin Vantage Gt3 Evo | left right | Green → Yellow → Red | Red | Green (flash) |
| Aston Martin Vantage Gt4 | left right | Green → Yellow → Red | Blue | Green (flash) |
| Audi 90 Gto | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Audi R18 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Audi R8 Lms Evo Ii Gt3 | left right | Green → Yellow → Red | Red | Blue (flash) |
| Audi R8 Lms Gt3 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Audi Rs3 Lms Tcr | left right | Green → Yellow → Red | Red | Red (flash) |
| Bmw M Hybrid V8 Gtp | left right | Green → Yellow → Red | Red | — |
| Bmw M2 Cs Racing | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Bmw M4 F82 Gt4 | outside center | Green → Yellow | Yellow | — |
| Bmw M4 G82 Gt4 Evo | left right | Green → Yellow → Red | Red | Red (flash) |
| Bmw M4 Gt3 | left right | Green → Yellow → Orange | Red | Magenta (flash) |
| Bmw M8 Gte | left right | Green → Yellow → Red | Red | Magenta (flash) |
| Bmw Z4 Gt3 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Cadillac Cts-V Racecar | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Cadillac V-Series.R Gtp | left right | Green → Yellow → Red | Blue | Blue (flash) |
| Chevrolet Corvette C6.R Gt1 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Chevrolet Corvette C7 Daytona Prototype | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Chevrolet Corvette C8.R Gte | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Chevrolet Corvette Z06 Gt3 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara Dw12 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara F3 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara Il-15 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara Ir-01 Indy Pro 2000 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara Ir-05 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Dallara Ir18 Indycar | left right | Green → Yellow → Red | Blue | Blue (flash) |
| Dallara P217 Lmp2 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ferrari 296 Challenge | left right | Green → Yellow → Red | Red | Green (flash) |
| Ferrari 296 Gt3 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ferrari 488 Gt3 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ferrari 488 Gt3 Evo 2020 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ferrari 488 Gte | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ferrari 499P Gtp | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Fia Cross Car | left right | Green → Yellow → Red | Red | — |
| Fia F4 | left right | Green → Red | Red | Green |
| Ford Fiesta Rs Wrc | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ford Gt Gt2 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ford Gt Gt3 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ford Gt Gte | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ford Mustang Fr500S | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ford Mustang Gt3 | outside center | Green → Yellow → Red | Blue | White (flash) |
| Ford Mustang Gt4 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Formula Renault 2.0 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Formula Renault 3.5 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Formula Vee | left right | Red | Red | — |
| Gen3 Supercars | outside center | Green → Yellow | Red | Blue (flash) |
| Honda Civic Type R Tcr | left right | Green → Red | Red | White (flash) |
| Hpd Arx-01C | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Hyundai Elantra N Tc | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Hyundai Veloster N Tc | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Indy Pro 2000 Pm-18 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Kia Optima | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Lamborghini Huracan Gt3 Evo | left right | Green → Yellow → Red | Blue | Yellow (flash) |
| Ligier Js P320 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Lotus 49 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Lotus 79 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Mazda Mx-5 Cup | outside center | Green → Yellow → Red | Red | Yellow (flash) |
| Mclaren 570S Gt4 | outside center | Green → Yellow → Red | Red | Blue (flash) |
| Mclaren 720S Gt3 | all at once | Blue | Red | Yellow (flash) |
| Mclaren Mp4-12C Gt3 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Mclaren Mp4-30 F1 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Mercedes-Amg F1 W12 E | all at once | Blue | Blue | Yellow (flash) |
| Mercedes-Amg F1 W13 E | all at once | Blue | Blue | Yellow (flash) |
| Mercedes-Amg Gt3 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Mercedes-Amg Gt3 Evo 2020 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Mercedes-Amg Gt4 | outside center | Green → Yellow → Red | Red | Yellow (flash) |
| Nissan Gtp Zx-T | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Pontiac Solstice | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Pontiac Solstice Rookie | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 718 Cayman Gt4 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 911 Gt3 Cup 991 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 911 Gt3 Cup 992.1 | left right | Green → Yellow → Red | Blue | Yellow (flash) |
| Porsche 911 Gt3 Cup 992.2 | left right | Green → Yellow → Red | Blue | Yellow (flash) |
| Porsche 911 Gt3 R (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 911 Gt3 R 992 | left right | Green → Yellow → Red | Blue | Yellow (flash) |
| Porsche 911 Rsr | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 919 Hybrid | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Porsche 963 Gtp | outside center | Green → Red | Blue | Green (flash) |
| Porsche Mission R | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Pro Mazda (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Radical Sr10 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Radical Sr8 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ray Ff1600 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Renault Clio Cup | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Riley Mkxx Daytona Prototype (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ruf Rt 12R Awd | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ruf Rt 12R C-Spec | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ruf Rt 12R Rwd | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Ruf Rt 12R Track | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Scca Spec Racer Ford | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Skip Barber Formula 2000 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Stock Car Brasil Chevrolet Cruze | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Stock Car Brasil Toyota Corolla | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Subaru Wrx Sti | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Super Formula Lights | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Supercars Holden Zb Commodore | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Toyota Gr86 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Toyota Sf23 Super Formula | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Usf 2000 | left right | Green → Yellow → Red | Red | Yellow (flash) |
| V8 Supercar Ford Falcon 2009 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| V8 Supercar Ford Falcon 2014 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| V8 Supercar Holden 2014 (Legacy) | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Vw Beetle Grc | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Vw Beetle Grc Lite | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Vw Jetta Tdi Cup | left right | Green → Yellow → Red | Red | Yellow (flash) |
| Williams-Toyota Fw31 | left right | Green → Yellow → Red | Red | Yellow (flash) |

---

## Cars Without Shift LEDs

These 29 cars have no steering wheel shift LED strip. fanatec-tuner sends no
LED commands when they are active; the wheel LEDs remain at the previously
applied state.

- ARCA Menards Series
- Dirt Late Model
- Dirt Micro Sprint Car
- Dirt Midget
- Dirt Mini Stock
- Dirt Sprint Car Non-Winged
- Dirt Sprint Car Winged
- Dirt Street Stock
- Late Model Stock Car
- Late Model Stock Car 2023
- Legends Car
- Lucas Oil Pro Truck
- Mini Stock
- NASCAR Craftsman Truck Series
- NASCAR Cup Series 1987 Classics
- NASCAR Cup Series 2018–2019 Legacy
- NASCAR Cup Series Gen4
- NASCAR Cup Series Legacy (Pre-2018)
- NASCAR Cup Series Next Gen Chevrolet Camaro
- NASCAR Cup Series Next Gen Ford Mustang
- NASCAR Cup Series Next Gen Toyota Camry
- NASCAR Xfinity Series
- NE Dirt Modified / SK Modified
- Silver Crown Car
- Sprint Car
- SRX Series
- Street Stock
- Street Stock Variants
- Super Late Model

---

## Contributing / Adding Profiles

Profiles live in `data/car_led_profiles.json`. Each entry is a JSON object
with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `car_id` | string | Lowercase display name (unique) |
| `iracing_car_paths` | string[] | iRacing `CarPath` identifiers (from session YAML `DriverInfo.Drivers[].CarPath`) |
| `pattern` | string | `"left_right"`, `"outside_center"`, `"all_at_once"`, or `"none"` |
| `led_count` | integer | Always `9` for current Fanatec wheels |
| `colors` | object | Map from stage name (e.g. `"stage1"`) to hex RGB (`"#00FF00"`) |
| `stages` | array | Ordered list of `{ "leds": [...], "color": "<stage name>" }` — defines which LEDs light at each step |
| `flash_color` | string\|null | Hex color for blink-at-shift-point flash |
| `flash_hz` | number | Blink rate in Hz (typically `15`) |
| `pit_limiter_color` | string\|null | Hex color when pit speed limiter is active; `null` = no LED change |
| `pit_limiter_flash` | bool\|null | Whether the pit limiter color blinks |
| `notes` | string | Source citation and behavior details |

### Finding the correct values

1. **Pattern and colors** — download the iRacing user manual PDF for the car
   from the iRacing members site. The shift light section is usually between
   "Loading a Setup" and "Advanced Setup Options" (pages 7–10). Look for
   "shift lights illuminate" or "upshift".

2. **carPath** — look up the car in
   `C:\Program Files\Fanatec\FanatecService\Service\xml\CarsList_iRacing.xml`
   or from session YAML in iRacing shared memory
   (`Local\IRSDKMemMapFileName`, field `DriverInfo.Drivers[idx].CarPath`).

3. **Flash color** — distinguish carefully: the phrase "lights change to red"
   in a manual often describes the last color in the *progression* (the lit
   state before flashing), not the flash color itself. The flash color is
   described separately, e.g. "all LEDs will change to blue and begin
   flashing" (Cadillac GTP).

4. **Pit limiter** — check if the manual describes LED behavior during pit
   limiter. Screen-only indicators (e.g. "magenta box on display") mean
   `pit_limiter_color: null`.
