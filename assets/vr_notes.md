## About Path of Titans VR Mod

This is a VR Mod for the game Path Of Titans, it is unofficial and provided by the community 
for free for your enjoyment. 

Some quick notes:

 * Uses the same controller inputs as the regular game, so keyboard and mouse or game controller
 * To work it must disable *anti-cheat* which means it's limited to single player mode, or servers which allow mods like this one. It will not work with official servers. 
 * *Use the mod at your own risk.** It injects a third-party mod (UEVR) with anti-cheat disabled;
  this may violate the Path of Titans Terms of Service and could get your account restricted or
  banned. There is **absolutely no warranty**.
 * This VR mod is intented for a seated experiance, and may cause nausia and dizziness, if you feel uncomfortable it's best to take a break, it won't get better if you push through it. 

## Opening the UEVR GUI
- Press **Insert** (inside the game, in VR or on the desktop window) to open the **UEVR GUI**.
  Everything is tuned from there — the recenter, calibration, and all sliders live in that overlay.

### FAQ

 - How can I change the settings for the mod? Hit the _insert_ key this will bring up the UEVR GUI from here you can try to change the settings. 
 - How can I disable the motion vignette? Open the UEVR GUI, go to the "Path of Titans" tab, disable the motion vingette (it's *strongly* recommended you leave this on) 
 - Why isn't the HUD showing up? Open the UEVR GUI, click on "Runtime" tab, then under "Overlay Options" try adjusting the "UI Distance" and "UI Size" - please file a bug and let us know what headset you're using and what UI Distance and UI Size settings worked for you. 
 - The VR view looks wrong, how do I fix it? Open the UEVR GUI, under "Unity" click "Reinitialize runtime"
 - The game doesn't perform well, what can I do? Go to the Path of Titans settings and try adjusting the graphics settings down your goal should be to get your frame render time on your GPU under 10ms. You can also adjust the resolution of your VR headset, you'll need to play with all these settings to figure out what works best for you. 

## Your desktop settings are safe
- The launcher **backs up** PoT's `GameUserSettings.ini` before a VR session and **restores it**
  when the game closes, so desktop play keeps your own resolution. Change this in *File → Settings*.

## Launching from SteamVR Home
- Install with the "Add to SteamVR Home" option checked (or use **File → Settings → Add to
  SteamVR Home** in the launcher). This registers a **Path of Titans VR** tile with SteamVR.
- Restart SteamVR, then put on your headset: the tile appears in SteamVR Home and starts the
  game **directly into VR** - no desktop clicking needed. The launcher waits in the background
  until the game closes, then restores your desktop settings as usual.
- Configure the launcher (game path etc.) from the desktop GUI **before** using the Home tile.
- Troubleshooting: the launcher writes each Home session's progress to
  `%APPDATA%\PoTVRLauncher\config\launch.log` - check it if VR doesn't come up.

## Known issues
- **HUD and game menus may not appear.** See the FAQ for how to fix this.
- **VR view fails to initialize correctly.** The first view is often wrong. Press **Insert**, go to
  **Unreal**, change *Rendering Method* from **Native Stereo** to **Synchronised Sequential**, then
  back to **Native Stereo** to force a clean re-init, or try hitting the **Reinitialize Runtime** button.
- **Group-member health indicators and Points-of-Interest markers are not positioned correctly.**
  If you want to travel to a PoI, make sure it is centered in your view first.
- **The low-health damage vignette is not drawn.**

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
