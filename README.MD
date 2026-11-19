# Nǎi Mā Speak

Chat translation between Chinese and English for World of Warcraft, made for playing on a Taiwanese or Mainland server
without reading Chinese.

奶媽 (nǎi mā, "wet nurse") is what Chinese-speaking players call a healer.

It reads Traditional and Simplified Chinese, and writes your replies in Traditional Chinese with Taiwanese wording, or
in Simplified.

<!-- The overlay in the game. Add the picture as assets/overlay.png, then remove this comment's two marker lines.
![The overlay on a chat tab in the game, with translated chat in the game's own colors](assets/overlay.png)
-->

<img src="assets/companion.png" alt="The companion app's window, on its Backend tab" width="480">

This is a fork of [cherryroots/Translate](https://github.com/cherryroots/Translate). It has two parts:

- **`addon/Translate`**: the in-game addon, listed in the game as "Nai Ma Speak". One folder loads in both WoW: Forever
  and retail (`## Interface: 16001, 120105`).

- **`companion`**: a small Rust app that runs next to the game, translates, and draws the result as an overlay over the
  game.

> **Tested with WoW: Forever on macOS, and on Windows 11 (4K screen, the game's HDR output on).** Retail WoW and Linux are untested since the fork: the addon declares the retail interface and the companion builds and passes its tests on Linux, but nobody has run them with the game.

## How It Works

WoW addons cannot reach the internet, so the two parts talk through the screen:

1. **Game to companion.** The addon draws each new chat line as a small strip of colored blocks in the top-left corner
   of the screen (128 x 18 pixels). The companion finds the strip, decodes it and translates the Chinese lines. The strip's last row shows every color it uses, so it is read correctly whatever the display's color profile or the game's gamma setting does to the colors. The strip is drawn at half brightness: with the game's HDR output on, a capture comes back brightened, and full-brightness colors would clip to white. On Windows the companion also covers the strip with a dark patch that you see and screen captures leave out, so the corner looks plain while the companion still reads it.

2. **Overlay.** The companion shows the lines in a see-through, click-through window. It can sit exactly on one of your chat tabs, taking its place, size and text size, and it draws lines the way the game does: same colors, wording, class-colored names, chat font and text shadow. A Chinese line appears at once, marked "(translation pending)", and turns into English when the translation arrives.
3. **Replies.** You type English, and the Chinese is put on your clipboard to paste into the game's chat box. The companion only ever writes to the clipboard.

The addon never learns what the companion did, and the companion never presses a key or sends chat for you.

Reading the strip is screen capture: it only reads pixels and never sends input to the game, but Blizzard's terms do not clearly allow or forbid it. Use it at your own judgment.

## Features

- **A copy of a chat window.** Pick a chat window in the addon's options and the overlay shows every line that window shows, including system messages and what other addons print, with their color tags. Without one, the overlay gets player chat and the game's own messages as they arrive.
- **Translation backends.** Google Translate's free endpoint (no setup, no key), Claude with a Claude Console API key, or any OpenAI-compatible endpoint: OpenAI, or a local model through Ollama or LM Studio.
- **Learn from the chat.** Chinese lines in the game's chat windows also end in a `[?]` link. Clicking it explains the line in the overlay: each word followed by how it is read, in pinyin or bopomofo, and what it means, then the whole line in English and, when there is one, a note on its grammar. It needs Claude or an OpenAI-compatible model; Google Translate cannot do it.
- **Player names.** With "Translate player names" on, a Chinese name is followed by what it means in English, or how it sounds if it means nothing. A `[?N]` link after a line explains its speaker's name: what it says and what it plays on. Names of players seen speaking stay in Chinese inside translated lines, so you can still tell who is meant.
- **Links in your language.** A linked quest, item, spell or achievement shows the name its sender's game gave it. The addon asks your own game for the name instead, so links arrive in English exactly as your client names them. A link the game has not loaded yet is translated word for word, and corrected within seconds, once the game knows the name.
- **Slang and place hints.** The chat models are told whether you play on a Taiwanese or a mainland server, so whose slang to expect. They are also told the WoW slang found in each line (缺補, 坦, 副本...), from a list of mainland and Taiwanese terms, and the English names of the classic world's zones, towns, dungeons and raids, with the short forms players type. Google Translate takes no hints.
- **In-game buttons.** "Translate" and "App" buttons on the chat window open the reply box and the companion app's window.
- **Translate again on request.** Chinese lines in the game's own chat windows end in a `[T]` link. Clicking it sends that line to the overlay again, ahead of everything waiting, marked "[On request]": for a line that scrolled away, or was dropped when chat was too busy.
- **Catching up.** While chat is quiet the addon keeps sending its last 50 chat lines again (the count is in the addon's options). Lines the companion missed, because another window covered the game or the companion was not running, appear in the overlay in their place within seconds; lines it has are not shown twice.
- **Scrollback.** With the overlay on a chat tab, the mouse wheel over that tab scrolls the overlay.
- **Chat history.** The companion keeps the recent lines in a tab where text can be selected and copied. Restarting the companion brings back the lines of the last ten minutes.

## Install

Download both parts from the [latest release](https://github.com/norswap/nai-ma-speak/releases/latest):

| File | What it is |
| --- | --- |
| `Nai-Ma-Speak-addon.zip` | The addon |
| `Nai-Ma-Speak-macos.zip` | The companion app for macOS (Apple silicon and Intel) |
| `nai-ma-speak-windows-x64.exe` | The companion for Windows |
| `nai-ma-speak-linux-x64` | The companion for Linux (untested) |

### The addon

Unzip `Nai-Ma-Speak-addon.zip` into the AddOns folder of the client you play, so the files end up in a folder named `Translate`:

```
Forever: World of Warcraft/_forever_/Interface/AddOns/Translate/
Retail:  World of Warcraft/_retail_/Interface/AddOns/Translate/
```

Restart the client if "Nai Ma Speak" does not appear in the addon list. If retail lists it as out of date, the retail patch has moved past `120105`: check the number with `/dump select(4, GetBuildInfo())` and update the `## Interface:` line in `Translate.toc`.

### The companion

Give it a folder of its own: it keeps `config.json`, `translate.log` and `backlog.json` next to the program (next to the `.app` on macOS). `config.json` holds your API keys in plain text, and `backlog.json` the overlay's recent chat lines.

- **macOS:** unzip `Nai-Ma-Speak-macos.zip`. The app is not signed by a registered developer, so macOS refuses to open it the first time. To let it through, do one of these once:
  - Right-click (or Control-click) the app and choose **Open**, then **Open** again in the dialog.
  - On recent versions of macOS, where that no longer offers to open it: double-click the app, dismiss the warning, then go to **System Settings > Privacy & Security**, scroll down to the message about "Nai Ma Speak" and click **Open Anyway**.
  - Or in Terminal: `xattr -dr com.apple.quarantine "Nai Ma Speak.app"`.

  macOS then asks for the Screen Recording permission, so the companion can read the strip; a new version may ask again.
- **Windows:** run the `.exe`. It is not signed, so Windows SmartScreen may warn the first time: choose **More info**, then **Run anyway**. Building it yourself needs the Visual Studio C++ Build Tools besides Rust.
- **Linux:** run `chmod +x nai-ma-speak-linux-x64` once, then start it.

### Building it yourself

It needs Rust 1.95 or newer. On macOS, `./macos-app.sh <folder>` inside `companion/` builds `Nai Ma Speak.app` into that folder; run `./macos-signing-cert.sh` once beforehand to sign your builds with a local certificate, so the Screen Recording permission survives rebuilds. Elsewhere, `cargo build --release` inside `companion/` makes `target/release/translate-companion`; Linux also needs the PipeWire, X11 and Wayland development packages listed in `.github/workflows/build.yml`.

Run WoW in **windowed** or **windowed (fullscreen)** mode: overlays cannot draw over exclusive fullscreen, and screen capture may see a black screen there.

## Using it

### In the game

The two buttons sit on the main chat window. Their place (on the tab row, upright beside the window at its top right or bottom right, or hidden) is set under Options > AddOns > Nai Ma Speak, along with which chat window the overlay copies. That window can also be picked in the companion (see below).

- **Translate** opens a box above the chat window. Type English and press Enter: the box closes at once and a note on the overlay says "Translating", then "Copied, paste it". Paste into the game's chat box and send. If the translation fails or is too long for one chat message (255 bytes), the note says so, and Translate reopens the box with your text.
- **App** brings the companion app's window over the game, and sends it away again. (The addon's own few settings are in the game's options.)

The same from the chat box, where `/nms` works in place of `/tr`:

| Command | Does |
| --- | --- |
| `/tr` | Open or close the translate box |
| `/tr hello` | Translate and copy, then reopen the chat box on the channel it was last on |
| `/tr /g hello` | The same, reopening the chat box on that target: `/p`, `/s`, `/y`, `/ra`, `/i`, `/o`, `/2`, `/w Name`, `/r` |
| `/tr /name` | Explain your target's name in the overlay; `/tr /name 黑熊` for one you type |
| `/tr /options` | Open the addon's options |
| `/tr /window General` | Have the overlay copy that chat window; `/tr /window all` for all chat. The companion's "Chat to translate" picker copies this command for you |
| `/tr /all` | Toggle lines without Chinese in the overlay |
| `/tr /test` | Say a Chinese test line in `/say` to check the strip and overlay |
| `/tr /test 10` | Send that many made-up chat lines to the overlay only: party, guild, raid, say, yell, whispers and a channel, from Chinese-named players in class colors, one with an item link |
| `/tr /probe` | Report which client features the addon found |
| `/tr /help` | List the commands |

### The companion window

Settings take effect as you change them and are saved by themselves. **Undo** puts them back as they were when the companion was opened, **Restart** starts it again, and **Hide** hides the window while the overlay stays; the game's App button or the app's icon brings it back.

- **Backend:** what translates, which server you play on (Taiwan or mainland China), which Chinese replies are written in, and whether explanations give readings in pinyin or bopomofo. "Test translation" sends a fixed line and shows the answer and how long it took.
  - *Google Translate* needs nothing, but it is an unofficial endpoint: it is rate-limited and may stop working.
  - *Claude* needs a key from the [Claude Console](https://console.anthropic.com). Haiku is the default, and enough for chat lines.
  - *OpenAI-compatible* has presets for OpenAI and for Tencent's translation model [HY-MT1.5-7B](https://huggingface.co/tencent/HY-MT1.5-7B-GGUF) on a local [Ollama](https://ollama.com) server (`ollama pull hf.co/tencent/HY-MT1.5-7B-GGUF:Q4_K_M`, 4.6 GB, free and offline). "Custom" takes any base URL and model; a bare address like `http://localhost:1234` gets `/v1` added.
- **Overlay:** whether to show the Chinese under each translation, whether to translate player names, when to hide the overlay, how many lines to keep, and where it sits.
  - **Chat to translate** shows which chat window the game sends to the overlay, or all chat. The companion cannot change game settings, so picking another copies a command such as `/tr /window General`: paste it into WoW chat and press Enter, and the picker follows once the game has switched. This works whether or not the overlay sits on a chat tab.
  - "Anchor to chat tab" puts the overlay on one of the game's chat tabs; a spare, empty tab works best. It takes the tab's size and text size unless you untick "from the chat tab" under text size, width or height and set your own (a size of its own keeps the tab's bottom-left corner), and has a background opacity of its own, none by default since the chat window draws one; raise it to read over bright scenes.
  - Unticked, you unlock the overlay, drag it into place and set its size, text size and background opacity yourself. Keep it off the top-left corner, where the strip is.
  - **Cover the pixel strip** (Windows, on by default) puts a dark patch over the strip. Windows leaves the patch out of screen captures (Windows 10 version 2004 or later), so the companion reads the strip through it; where that is not possible the patch stays away and the strip shows.
- **Status:** whether the strip was found and whether it can be read, and which chat tabs the game reported. **Copy report** copies all of that with your settings (without the API keys) and the log, to paste into a bug report.
- **Chat history** and **Translate:** the recent lines, and a field for translating a reply outside the game.

Chat lines are sent to whichever backend you choose. Only a local model keeps them on your computer.

## Troubleshooting

- **The overlay stays empty.** Open the Status tab: it says whether the pixel strip was found, and whether it can be read. "Found, but cannot read it" with a note about the palette row means the addon is older than the companion: update both from the same release. The strip must be visible in the top-left corner of the game, and on macOS the companion needs the Screen Recording permission. `/tr /probe` in the game reports what the addon found, and `/tr /test` sends a line through.
- **"Found the pixel strip, but cannot read it".** When this lasts, the companion logs a diagnosis in `translate.log` (how the strip's color levels came back, and which colors run together) and saves the strip as `strip-unreadable.png` beside it, once per run. That picture is chat in encoded form: it stays on your computer, so attach it to a report only if you are happy to. "Too close together to tell apart" usually means brightness: update the addon (it draws the strip dimmed, which HDR needs), otherwise check Windows HDR and the game's brightness, contrast and gamma.
- **To check a screenshot for the strip,** run `translate-companion --decode screenshot.png`. It prints the same diagnosis and also writes it to `screenshot.png.decode.txt`, since the Windows build cannot print to a terminal.
- **The companion froze or misbehaved.** `translate.log`, next to `config.json`, records crashes, stalls, translation errors and what happened to the strip and the chat tabs. **Copy report** on the Status tab copies it with your settings, without the API keys.

## Known limits

- Chat text inside instances may be a secret value under Midnight rules. The addon cannot read those lines, so they are not shown or translated.
- Each strip frame holds 376 bytes, enough for a chat message (255 bytes at most) with its speaker and colors. Longer lines, such as some system messages, are cut.
- The strip carries about 1,500 bytes a second. In a flood of chat, lines queue in the addon, up to 200, and the oldest are dropped beyond that. A dropped line comes back by itself if it is among the last ones the addon repeats, and can be asked for again with its `[T]` link.
- Icons inside chat lines (raid markers and the like) are left out of the overlay.
- A translation keeps the colors of the original line only where they can be placed: a line in one color, item links in order, and words that come back unchanged.
- The buttons sit on the main chat window and its docked tabs, not on a chat window floating on its own.
- The strip assumes the UI is drawn at the screen's physical resolution. If the companion never finds it, send a screenshot through `--decode`.

## Icon

`assets/icon.svg` is the source for the icons. `companion/icon.png` is the window icon, `companion/icon.ico` is built into the Windows exe, and `addon/Translate/icon.tga` is shown in the AddOns list through `## IconTexture`. To change the icon, edit the SVG and re-export those files at the same sizes.

## Making a release

Run the **Build** workflow from the Actions tab with a tag such as `v0.3.0` in the *release* box, or push a `v*` tag. The workflow builds everything and publishes the release with notes generated from the merged pull requests. The tag sets the version: a `v0.3.0` run first commits "Release v0.3.0" to main with `## Version` in `Translate.toc` and the companion version in `companion/Cargo.toml` and `Cargo.lock` set to 0.3.0, then builds and tags that commit. Prefer the release box: a pushed tag stays on the commit you tagged, before the version commit.

## License and credits

MIT, see [LICENSE](LICENSE). The original project is by cherryroots. Slang terms from two other MIT-licensed addons, the bundled Marcellus font and the Chinese conversion library are credited in [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES). Contributions are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md).
