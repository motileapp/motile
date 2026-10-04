# Bringing the GPUI app up to date with main

The port was written against the Mac app at `583220c`. Since then the Mac app moved into
`packages/apple` (shared with the iOS app) and changed in the ways below. Anything only iOS got
is left out. Each step ends with a picture of the GPUI app beside the Mac app.

Done already: the app builds against the new core (`Config.media_limit`), it is a Cargo
workspace of its own so no workflow builds it, and the composer is opaque in the dark.

## 1. Colours and the window

`theme.rs`, from `Shared/Theme/Theme.swift` (light / dark):

| | old | new |
| --- | --- | --- |
| background | `fcfcfc` / `19191a` | `f8f9fc` / `08090d` |
| raised | `ffffff` / `242426` | `ffffff` / `15171d` |
| bubble | `f1f1f3` / `2d2d30` | `eceef4` / `1c1f27` |
| code background | `f6f6f7` / `222224` | `f1f3f8` / `101218` |
| text | `27272a` / `ececee` | `22242b` / `ecedf1` |
| prose | `3a3a40` / `c2c2c7` | `383b45` / `c1c4cd` |
| secondary | `71717a` / `9c9ca6` | `6b6f7c` / `9a9eab` |
| tertiary | `a1a1aa` / `6c6c75` | `9a9eab` / `646875` |

The window is a solid background now on the Mac too, which is what the port already draws.
The popover, settings and sheet surfaces that were measured from the old look are checked
against the new one.

## 2. The composer

- Surface: the Mac draws Liquid Glass over `raised` at 60%, and no shadow. GPUI has no blur, so
  the box and its strips are `raised` at 94%, what the Mac itself falls back to before
  macOS 26, and the shadow goes.
- Placeholders: "Ask anything", "Send a follow-up", "Waiting for X…", "Install Claude Code or
  Codex", and "Message to bring it back" for a thread that is done.
- The done banner says "Done" instead of two sentences.
- An option of a question lights up under the pointer.

## 3. The transcript

- A user message has a row under its bubble: the time and a copy button ("Copy message"),
  right-aligned, shown while the pointer is over the message. The row is 36 points taller.
- A turn's end is one row of 38 points: the copy button first, then "3:42 PM · Worked for 1m
  30s". The rule under it goes. It shows while the pointer is over that reply. The time comes
  from the row's new `at`, written as the Mac's `Time.stamp` (the date in front unless today).
- The working line is as tall as the new turn end.
- Rows scroll under the composer: the fade above it goes, and so do the 16 points after the
  last row.

## 4. Small changes in the other views

- The git notice is at most 300 wide instead of always, and the quick action's label stays on
  one line. Its new capitals ("Commit & Push") come from the core already.
- A row of the command panel and of the branch picker is darker under the pointer than one
  reached with the arrows (two layers of hover on the Mac).
- A settings row puts its controls under its text when both don't fit on a line.
- The tab's close symbol is medium instead of bold. Pressed buttons keep their hover fill
  instead of dimming; the send and stop buttons and an attached picture dim to 70%.
- The sign-in page's last line is centred.

## 5. Scripts and the icon

- `scripts/dev-app.sh` keeps a stack of its own but picks free ports once and keeps them in
  `build/dev/ports`, as `apps/macos/scripts/dev-stack.sh` does, instead of three fixed ones.
- `--mac` still builds the Mac app with `apps/macos/scripts/build-app.sh`, which is unchanged.
- `build-app.sh` takes the new icon by itself: it compiles the Mac app's `AppIcon.icon`.

## Not needed

- `foreground` and `network_changed` are only sent by the iOS app.
- The media limit is only set on iOS.
- The combined git actions in the menu are iOS's; the Mac's menu is as it was.
