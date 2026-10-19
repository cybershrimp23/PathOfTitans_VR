# VR Notes

## Opening the UEVR GUI
- Press **Insert** (inside the game, in VR or on the desktop window) to open the **UEVR GUI**.
  Everything is tuned from there — the recenter, calibration, and all sliders live in that overlay.

## Tuning
- All VR sliders (*FollowView*, UI size, vignette, …) live in the **UEVR GUI** under Path of Titans settings (press **Insert**).

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
