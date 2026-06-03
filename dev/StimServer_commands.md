# StimServer Command Reference

Extracted from the StimServer documentation (May 22, 2021) by Michael Stephan.

`kk` = stimulus key (uint16), `ka` = animation key (uint16), `00` = global prefix.

---

## A. General Commands

| Command | Description |
|---|---|
| `[00 0]` | Delete all stimuli (except photo diode signal) |
| `[00 0 e]` | Enable (`e=1`) or disable (`e=0`) photo diode signal |
| `[00 0 0 e]` | Enable (`e=1`) or disable (`e=0`) all stimuli (except photo diode signal) |
| `[00 0 1 p]` | Protect (`p=1`) or unprotect (`p=0`) all stimuli (except photo diode signal) |
| `[00 0 r g b]` | Set screen background color; `r`, `g`, `b` = 0…255 (uint8) |
| `[00 1 0]` | End deferred mode (apply all pending changes) |
| `[00 1 1]` | Start deferred mode |
| `[00 1 2]` | Query performance counter (read 8 bytes as uint64) |
| `[00 1 3 a]` | Set default terminal action `a` (uint8) for newly created animations |
| `[00 1 4]` | Query and reset global error state (read uint16 error mask; bits: 1=server, 2=stimulus, 4=animation) |
| `[00 1 5 r g b α]` | Set default draw color (uint8 components, 0…255); inherited by new particle, symbol, rect, petal, ellipse, wedge stimuli |
| `[00 1 6]` | Query performance frequency (read 8 bytes as uint64) |
| `[00 1 7]` | Query and reset most recent general error code (read uint16) |
| `[00 1 8]` | Query physical frame rate in Hz (read 4 bytes as single float) |
| `[00 1 9 r g b α]` | Set default outline color (uint8, 0…255); inherited by new rect, petal, ellipse, wedge stimuli |
| `[00 1 10 γ]` | Set gamma correction value (single float) for the output device |
| `[00 1 11]` | Cancel deferred mode (discard pending changes) |
| `[00 1 12]` | Query presentation device width and height in pixels (read 4 bytes as two uint16s) |
| `[00 16 0]` | Turn photo diode signal off (black) |
| `[00 16 1]` | Turn photo diode signal on (white) |
| `[00 16 2]` | Toggle photo diode signal |
| `[00 16 3]` | Turn on photo diode signal flicker mode (toggles with each frame) |
| `[00 16 3 p]` | Position photo diode signal: upper-left (`p=0`) or lower-left (`p=1`). Not deferrable. |

---

## A.1 Stimulus Creation

After sending a creation command, read 2 bytes (uint16) from the pipe: `0` = error, otherwise the key `kk` of the new stimulus. These commands are not deferrable.

| Command | Description |
|---|---|
| `[00 2 filename]` | Create a picture stimulus from the specified image file |
| `[00 3 kk filename]` | Create or replace (`kk`) a picture stimulus from the specified image file |
| `[00 4 filename]` | Create a pixel shader stimulus from the specified HLSL file |
| `[00 5 kk filename]` | Create or replace (`kk`) a pixel shader stimulus from the specified HLSL file |
| `[00 8 w h filename]` | Create a particle stimulus of size `w`×`h` pixels (`w`, `h` uint16) from the specified coordinate file |
| `[00 9 w h kk filename]` | Create or replace (`kk`) a particle stimulus from the specified file |
| `[00 10]` | Create a pixel stimulus |
| `[00 11 kk]` | Create or replace (`kk`) a pixel stimulus |
| `[00 12 t s]` | Create a symbol stimulus of type `t` (uint8) and size `s` (uint16). Types: `1`=filled circle, `2`=outlined circle |
| `[00 13 t s kk]` | Create or replace (`kk`) a symbol stimulus of type `t` and size `s` |
| `[00 14 filename]` | Create a bitmap brush stimulus from the specified image file |
| `[00 15 kk filename]` | Create or replace (`kk`) a bitmap brush from the specified image file |
| `[00 18 kk filename]` | Create a pixel-shaded picture stimulus from the specified HLSL file, operating on existing picture object `kk` |
| `[00 20]` | Create a rectangle stimulus (default: filled with "default draw color") |
| `[00 24 filename]` | Create a motion picture stimulus from the specified image/GIF/TIFF file |
| `[00 26]` | Create a petal stimulus (default: filled with "default draw color") |
| `[00 28]` | Create an ellipse stimulus (default: filled with "default draw color") |
| `[00 30]` | Create a wedge stimulus (default: filled wedge, center angle 9°) |

---

## A.2 Animation Creation

After sending a creation command, read 2 bytes (uint16): `0` = error, otherwise the animation key `ka`. These commands are not deferrable.

| Command | Description |
|---|---|
| `[00 130 filename]` | Create a general motion path animation from the specified binary file (`single(2,n)` pixel coordinates per frame) |
| `[00 132 vv]` | Create a straight-line-segment motion path; `vv` = velocity in pixels/second (uint16) |
| `[00 136 start end duration m]` | Create a linear range animation; `start`/`end` are single floats, `duration` is seconds (single float), `m` is mode (uint8): `1`=alpha (pictures/wedges), or pixel shader parameter index |
| `[00 138 nn]` | Create a flash animation for `nn` frames (uint16) |
| `[00 138 nn mm]` | Create a flicker animation: stimulus on for `nn` frames, off for `mm` frames (both uint16) |
| `[00 140 memMapName]` | Create an external position control animation; `memMapName` is the shared memory section name providing two single floats (x, y) |

---

## B.1 General Stimulus Commands

| Command | Description |
|---|---|
| `[kk 0]` | Remove (destroy) stimulus. Not deferrable. |
| `[kk 0 e]` | Enable (`e=1`) or disable (`e=0`) stimulus |
| `[kk 3 p]` | Protect (`p=1`) or unprotect (`p=0`) stimulus |
| `[kk 3 x y]` | Move center of stimulus to `x`, `y` (single floats) |
| `[kk 7]` | Query and reset error state of stimulus (read uint16). Not deferrable. |
| `[kk 8]` | Query position of stimulus as two single floats. Not deferrable. |
| `[kk 14]` | Bring stimulus to front (drawn last). Returns new key as uint16. |
| `[kk 14 ks]` | Swap drawing order (keys) of two stimuli `kk` and `ks` |

---

## B.2 Outline Commands

Applies to: rectangle, petal, ellipse, wedge.

| Command | Description |
|---|---|
| `[kk 6 mode]` | Set draw mode (uint8, default `1`): `1`=filled, `2`=outlined, `3`=filled+outlined |
| `[kk 9 r g b α]` | Set outline color (uint8, 0…255); initial value inherited from "default outline color" |
| `[kk 10 linewidth]` | Set outline line width (single float, default `2`) |

---

## B.3 Picture Commands

| Command | Description |
|---|---|
| `[kk 1 α]` | Set global alpha (transparency) value; `α` = 0…255 (uint8) |
| `[kk 2 i]` | Set rotation increment to `i` (i = −128…127) |
| `[kk 4 φ]` | Set orientation angle in degrees (single float) |

---

## B.4 Pixel Shader Commands

| Command | Description |
|---|---|
| `[kk 1 i value]` | Set parameter `i` of shader to `value` (single float) |
| `[kk 2 value]` | Set animation increment (speed) to `value` (single float) |
| `[kk 5 i r g b α]` | Set color `i` (i = 1…4) components (uint8, 0…255) |
| `[kk 6 value]` | Set animated value (single float) |
| `[kk 9 width height]` | Set stimulus size (uint16s). Valid only for "new format" shaders. |

---

## B.5 Particle Stimulus Commands

| Command | Description |
|---|---|
| `[kk 1 1 value]` | Set particle diameter (uint16, initial value `4`) |
| `[kk 1 2 value]` | Set circular patch radius (0…1.42, single float); `0` = disable (rectangular mode) |
| `[kk 1 3 value]` | Set Gaussian patch radius (single float); `0` = disable Gaussian patch |
| `[kk 2 velocity]` | Set particle velocity (normalized coordinate increment per frame, single float) |
| `[kk 4 angle]` | Set global direction of particle movement in degrees (single float) |
| `[kk 5 r g b α]` | Set particle color components (uint8, 0…255) |
| `[kk 6 value]` | Set animated shift value (single float) |

---

## B.6 Symbol Commands

| Command | Description |
|---|---|
| `[kk 1 1 value]` | Set symbol size (uint16) |
| `[kk 5 r g b α]` | Set symbol color components (uint8, 0…255) |

---

## B.7 Bitmap Brush Commands

| Command | Description |
|---|---|
| `[kk 10]` | Remove (destroy) opacity mask. Not deferrable. |
| `[kk 10 e]` | Enable (`e=1`) or disable (`e=0`) opacity mask |
| `[kk 11 filename]` | Load an opacity mask from the specified image file |
| `[kk 13 x y]` | Move opacity mask relative to bitmap brush (single precision floats) |

---

## B.8 Pixel Shaded Picture Commands

| Command | Description |
|---|---|
| `[kk 1 i value]` | Set parameter `i` of shader to `value` (single float) |
| `[kk 2 value]` | Set animation increment (speed) to `value` (single float) |
| `[kk 6 value]` | Set animated value (single float) |

---

## B.9 Rectangle Commands

Also supports [Outline Commands (B.2)](#b2-outline-commands).

| Command | Description |
|---|---|
| `[kk 1 1 width height]` | Set rectangle size (uint16s, initial `11×21` pixels) |
| `[kk 4 φ]` | Set orientation angle in degrees (single float) |
| `[kk 5 r g b α]` | Set fill color components (uint8, 0…255) |

---

## B.10 Motion Picture Commands

Also supports [Picture Commands (B.3)](#b3-picture-commands).

| Command | Description |
|---|---|
| `[kk 6 n]` | Select frame `n` as current frame (uint32, starting at 0) |
| `[kk 9 nn]` | Set number of frames to display before advancing (uint16; `0` = hold current frame permanently, default `1`) |

---

## B.11 Petal Commands

Also supports [Outline Commands (B.2)](#b2-outline-commands).

| Command | Description |
|---|---|
| `[kk 1 1 r]` | Set small radius `r` (single float, default `25`) |
| `[kk 1 2 R]` | Set large radius `R` (single float, default `100`) |
| `[kk 1 3 d]` | Set distance `d` (single float, default `250`) |
| `[kk 1 4 q]` | Set parameter `q` (single float, default `0.3819660113`) |
| `[kk 4 φ]` | Set orientation angle in degrees (single float, default `0`) |
| `[kk 5 r g b α]` | Set fill color components (uint8, 0…255) |

---

## B.12 Ellipse Commands

Also supports [Outline Commands (B.2)](#b2-outline-commands).

| Command | Description |
|---|---|
| `[kk 1 1 width height]` | Set ellipse size (uint16s, initial `100×100` pixels) |
| `[kk 4 φ]` | Set orientation angle in degrees (single float, default `0`) |
| `[kk 5 r g b α]` | Set fill color components (uint8, 0…255) |

---

## B.13 Wedge Commands

Also supports [Outline Commands (B.2)](#b2-outline-commands).

| Command | Description |
|---|---|
| `[kk 1 1 gamma]` | Set center angle in degrees (single float, default `9`) |
| `[kk 4 φ]` | Set orientation angle in degrees (single float, default `0`). Controllable via linear range animation (mode=1). |
| `[kk 5 r g b α]` | Set fill color components (uint8, 0…255) |

---

## C.1 General Animation Commands

| Command | Description |
|---|---|
| `[ka 0]` | Remove (destroy) animation. Not deferrable. |
| `[ka 0 a]` | Set terminal action bitmask `a` (uint8): `1`=disable stimulus, `4`=toggle photodiode, `8`=signal event, `16`=restart (cyclic), `32`=reverse, `64`=goto initial state, `128`=end deferred mode. `0` = deassign from stimulus (default). |
| `[ka 0 e kk]` | Assign (`e=1`) or deassign (`e=0`) animation `ka` to/from stimulus `kk` |
| `[ka 7]` | Query and reset error state of animation (read uint16). Not deferrable. |

---

## C.2 Straight Line Segments Animation Commands

| Command | Description |
|---|---|
| `[ka 11 vertices]` | Set vertex coordinates (int16 pixel coordinates) for the motion path polygon. Max 31 vertices. Animation starts immediately if already assigned to a stimulus. |

---

## C.3 Flash Animation Commands

| Command | Description |
|---|---|
| `[ka 2 nn]` | Set number of frames (uint16) |

---

## C.4 Flicker Animation Commands

| Command | Description |
|---|---|
| `[ka 2 nn mm]` | Set number of "on" frames (`nn`) and "off" frames (`mm`) (both uint16) |

---

## C.5 External Position Control Animation Commands

| Command | Description |
|---|---|
| `[ka 3 x y]` | Set position offset (single floats); added to coordinates from shared memory |
