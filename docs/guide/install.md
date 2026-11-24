# Install 555

555 is free, with no account and no ads. Download it from [555.amon.lol](https://555.amon.lol/#download) or the [GitHub releases](https://github.com/amonre555/555/releases/latest).

| System | Requirements | File |
| --- | --- | --- |
| macOS | Apple silicon and Intel, one app | `.dmg` |
| Windows | Windows 10 and 11, 64-bit | `-setup.exe` |
| Android | Android 8 or later, 64-bit phones | `.apk` |
| iPhone and iPad | iOS 15 or later | `.ipa`, with SideStore |
| Linux | Build from source | — |

## macOS

Open the `.dmg` and drag 555 into **Applications**. 555 isn't signed with Apple yet (it costs money every year), so on first launch: right-click 555 → **Open**.

If macOS says **"555 is damaged"** or won't open it, run this in Terminal, then open 555 again:

```sh
xattr -cr /Applications/555.app
```

Once installed, 555 updates itself.

## Windows

Run the installer. If SmartScreen shows up: **More info → Run anyway**. 555 works with the media keys and the Windows media panel, and updates itself once installed.

## Android

555 isn't on the Play Store: open the `.apk` on your phone and allow installs from that app. If Play Protect warns you: **More details → Install anyway**. To update, install the new `.apk` over the old one: your music and settings stay.

## iPhone and iPad

555 isn't on the App Store. You install it with [SideStore](https://sidestore.io), a free app that signs it with your own Apple Account. The first setup takes about 15 minutes and needs a computer once: follow the [step-by-step guide](https://555.amon.lol/ios.html).

After that:

- **Updates:** add the 555 source in SideStore once, and new versions install from the phone. Or download a new `.ipa` and open it in SideStore (**My Apps → +**): it replaces the app and keeps your music and settings.
- **Every 7 days** SideStore renews 555 by itself.

Common errors are covered in the [guide's troubleshooting section](https://555.amon.lol/ios.html).

## Linux

There's no package yet. 555 builds on Linux with Rust and Node.js, and media keys work through MPRIS. See the [build instructions](https://github.com/amonre555/555#readme).

## Is it safe?

The code is public, and every installer is built by GitHub Actions from it. Updates are signed: 555 refuses any update whose signature doesn't match. The Android APK is signed too.
