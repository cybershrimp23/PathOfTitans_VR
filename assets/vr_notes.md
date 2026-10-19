# VR Notes

## Before you play
- **Anti-cheat** must be disabled in the PoT launcher (*Settings → Game*) or injection will fail.
  This limits you to **single-player** and **mod-allowed servers**.
- The game starts **windowed at 1280×1024** (VR-tuned): the VR HUD texture shape follows the
  window shape, and this aspect is tuned.

## Opening the UEVR GUI
- Press **Insert** (inside the game, in VR or on the desktop window) to open the **UEVR GUI**.
  Everything is tuned from there — the recenter, calibration, and all sliders live in that overlay.

## Getting VR to show up
- Give it **~30 seconds** after the window appears for UEVR to come up.
- The **first view is often wrong** (known UEVR/PoT issue). Press **Insert**, then click
  **Reinitialize Runtime** — or take the headset off and put it back on.

## Tuning
- All VR sliders (*FollowView*, UI size, vignette, …) live in the **UEVR GUI** (press **Insert**).

## Your desktop settings are safe
- The launcher **backs up** PoT's `GameUserSettings.ini` before a VR session and **restores it**
  when the game closes, so desktop play keeps your own resolution. Change this in *File → Settings*.

## Known issues
- **VR view fails to initialize correctly.** The first view is often wrong. Press **Insert**, go to
  **Unreal**, change *Rendering Method* from **Native Stereo** to **Synchronised Sequential**, then
  back to **Native Stereo** to force a clean re-init.
- **Group-member health indicators and Points-of-Interest markers are not positioned correctly.**
  If you want to travel to a PoI, make sure it is centered in your view first.
- **The low-health damage vignette is not drawn.**
- **The character editor view causes flashing**, and there are general **issues with the 3D view**
  in that screen. Best avoided while in VR.

## License & disclaimer
- This launcher is free and open source under the **MIT License**.
- **Use the mod at your own risk.** It injects a third-party mod (UEVR) with anti-cheat disabled;
  this may violate the Path of Titans Terms of Service and could get your account restricted or
  banned. There is **absolutely no warranty**.
- **PATH OF TITANS** and **Alderon Games** are trademarks of **Alderon Games Inc.** This is an
  independent, unofficial community tool — **not affiliated with, endorsed by, or sponsored by
  Alderon Games**.
- **Privacy:** this launcher collects **no user data** — no analytics, no telemetry, no network
  calls. It only reads/writes local files and launches the game.
