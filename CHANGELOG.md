# Changelog

Changes in this fork (tangomando/nai-ma-speak) since norswap/nai-ma-speak v0.3.0, gathered for a pull request back
to it.

## Unreleased

### Windows: the strip can now be read

The Windows companion found the strip but never read a frame ("its frames fail their check"). The cause was the game's
HDR output (`HDROutput 1`): a capture of the game comes back brightened, and the strip's four color levels 0, 85, 170,
255 read as 0, 149, 255, 255. The top two ran together, so about a third of the blocks were misread.

- **HDR:** the addon draws the strip, frame and palette, at half brightness, always. A first version dimmed it only
  while the `HDROutput` CVar was on, but HDR is a hidden setting switched from the console, and the strip could stay
  bright after a switch. At half brightness the levels stay apart whether or not the capture brightens them (tested at
  gains 1, 1.75 and 2), and the strip is less glaring. The companion now tells the sync colors by each channel against
  the brightest one, not a fixed level of 128, so the dimmed strip is found. **This needs the addon and the companion
  from the same version**: an older companion does not find the dimmed strip.
- **Blocks read from their inside:** each block is the per-channel median of its inner pixels, leaving out a 1 px
  border, instead of one pixel at its center. Edge blending (scaling, sharpening) and single noisy pixels no longer
  change a block.
- **No more false finds on scenery:** at 4K, colorful scenery passed for the sync blocks as runs of 1 px every few
  seconds, and each false find held the lock for 3 s. Runs under 2 px are no longer taken for the strip.

### Diagnosing an unreadable strip

- `--decode` prints a diagnosis: how each channel's four levels came back, the two palette colors closest together,
  how many data blocks are ambiguous, and the length and checksum read against the one computed. It also writes it to
  `<picture>.decode.txt`, since the Windows release is a windowed program and its printing never reaches a terminal.
- When the strip stays unreadable, the companion logs that diagnosis (colors and counts, no chat) and saves the strip
  as `strip-unreadable.png` beside `translate.log`, once per run. The picture is encoded chat and stays on the player's
  computer.
- When colors run together, the Status line says so and points to the game's HDR option, Windows HDR and the game's
  brightness, contrast and gamma, instead of only "its frames fail their check".

### Overlay

- **Chat to translate:** a picker on the Overlay tab, whether or not the overlay is anchored, shows which chat window
  the game sends to the overlay, or all chat. The companion cannot reach the game, so picking another copies
  `/tr /window <name>` for the player to paste into chat.
- **Background opacity on a chat tab:** an anchored overlay has its own background opacity (`anchored_opacity` in
  `config.json`), none by default as before.
- **A smaller strip:** blocks are 2 physical pixels instead of 4, so the strip is 128 x 18 pixels instead of 256 x 36,
  a quarter of the area, and carries as much per frame. The finder measures the block size, so either size is read;
  placing the overlay on a chat tab uses `strip::BLOCK` and needs the addon and the companion from the same version.
- **Strip cover (Windows):** "Cover the pixel strip", on by default (`cover_strip`), puts a near-black window over the
  strip. It is marked `WDA_EXCLUDEFROMCAPTURE`, so the player sees a plain dark corner while screen captures, the
  companion's own included, still see the strip. The cover opens parked off screen and is moved onto the strip only
  after the system confirms it is left out of captures; this is checked on every pass, so a cover that captures would
  see never hides the strip. It goes away with the strip and while WoW is out of sight. Not on macOS yet
  (`NSWindow.sharingType = .none` would be the counterpart, but ScreenCaptureKit may not honor it).

### Addon

- New command `/tr /window <name>` (or `/tr /window all`): the same setting as "Lines sent to the overlay" in the
  options, matched without regard to case.
- New strip line `W\t<name>`, sent after each `L`: the chat window the addon copies, empty for all chat. Older
  companions ignore it, and an older addon simply never sends it (the picker then says "Not reported yet").

### Smaller

- The focused-window check goes by the window title, and asks for the program's name only for an untitled window.
  On Windows asking fails for many programs (those run as administrator, those without version information), and
  xcap logged an error four times a second while one had focus.
- macOS-only window listing code is marked so the Windows build has no dead-code warnings.
- README: Windows marked as tested, the HDR note, the new overlay settings, `/tr /window`, and the new troubleshooting.

### Tested

- Windows 11, 4096x2160 at 100%, WoW: Forever with HDR output on, Claude Haiku backend: the strip is read and chat lines
  are translated.
- Switching with `/tr /window`: the game switched to another window and the companion's picker followed.
- Strip cover: the window sits exactly over the strip (0,0, 256x36) with display affinity 0x11, and the strip is
  read through it. A test window marked the same way vanished from a GDI capture of the screen.
- `cargo test --locked` (71 tests, 8 new) and `cargo clippy --all-targets` clean on Windows. Addon tests pass under
  Lua 5.4 with `unpack = table.unpack`; this machine has no Lua 5.1.
- **Not yet checked:** the always-dimmed strip in the game (the HDR fix was checked with the version that dimmed it
  only under HDR), the macOS build with these changes, and retail.
