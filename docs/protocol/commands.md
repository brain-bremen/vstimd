# Command Reference

## System commands

These commands use `system` as the target and are not addressed to a specific stimulus.

### Create stimulus

| Command | Returns | Description |
|---|---|---|
| `CreateRectRequest` | handle | Create a rectangle |
| `CreateCircleRequest` | handle | Create a circle |
| `CreateEllipseRequest` | handle | Create an ellipse |
| `CreateGratingRequest` | handle | Create a grating stimulus |
| `CreateTextRequest` | handle | Create a text stimulus |

All create commands accept optional `name` (human-readable label) and `id` (client-supplied
UUID string; server generates one if empty).

### Scene-wide

| Command | Description |
|---|---|
| `SetBackgroundRequest` | Set background colour (`Color`) |
| `SetDeferredModeRequest` | Begin (`active=true`) or commit/cancel (`active=false, cancel`) a deferred batch |
| `DeleteAllRequest` | Delete all stimuli |
| `SetAllEnabledRequest` | Show or hide all stimuli |

### Query

| Command | Response field | Description |
|---|---|---|
| `QueryServerInfoRequest` | `server_info` | Display size, refresh rate, version |
| `ListStimuliRequest` | `stimulus_list` | Table of all active stimuli (handle, type, enabled, UUID, name) |

---

## Per-stimulus commands

These commands use a `stimulus` handle as the target.

### Lifecycle

| Command | Description |
|---|---|
| `DeleteRequest` | Delete the stimulus |
| `SetEnabledRequest` | Show or hide (`enabled: bool`) |
| `SetNameRequest` | Rename (does not affect handle or UUID) |

### Transform

| Command | Fields | Description |
|---|---|---|
| `SetPositionRequest` | `x`, `y` | Move to pixel coordinates |
| `SetOrientationRequest` | `angle_deg` | Rotate (degrees, CCW) |

### Appearance

| Command | Fields | Description |
|---|---|---|
| `SetFillColorRequest` | `Color` | Fill colour (RGBA 0–1) |
| `SetOutlineColorRequest` | `Color` | Outline colour |
| `SetOutlineWidthRequest` | `line_width` | Outline stroke width in pixels |
| `SetAlphaRequest` | `opacity` | Global opacity (0–1) |
| `SetDrawModeRequest` | `DrawMode` | `FILLED`, `OUTLINED`, or `FILLED_AND_OUTLINED` |

### Shape-specific

| Command | Applies to | Fields |
|---|---|---|
| `SetRectSizeRequest` | Rectangle | `width`, `height` |
| `SetCircleRadiusRequest` | Circle | `radius` |
| `SetEllipseSizeRequest` | Ellipse | `width`, `height` |

### Grating-specific

| Command | Fields |
|---|---|
| `SetGratingPhaseRequest` | `phase` (cycles) |
| `SetGratingSfRequest` | `sf` (cycles/pixel) |
| `SetGratingContrastRequest` | `contrast` (0–1) |
| `SetGratingWaveformRequest` | `WaveformType` (SIN/SQR/SAW/TRI) |
| `SetGratingMaskRequest` | `MaskType` (NONE/CIRCLE/GAUSS/HANN/RAISED_COS) |
| `SetGratingDriftSpeedRequest` | `speed` (cycles/frame) |
| `SetGratingDriftDecoupledRequest` | `decoupled` (bool) — drift direction independent of orientation |
| `SetGratingDriftAngleRequest` | `angle_deg` — drift direction when decoupled |
| `SetGratingForeColorRequest` | `Color` — colour at carrier peak |
| `SetGratingBackColorRequest` | `Color` — colour at carrier trough |
| `SetGratingOpacityRequest` | `opacity` (0–1) |

### Text-specific

| Command | Fields | Description |
|---|---|---|
| `SetTextRequest` | `text` | Replace the displayed string |
| `SetTextColorRequest` | `Color` | Text colour (RGBA 0–1) |

### Query

| Command | Response field | Description |
|---|---|---|
| `QueryStimulusRequest` | `stimulus_info` | Full current state of the stimulus |
