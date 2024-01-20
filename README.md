# Dust

[![Build status](https://github.com/kelpsyberry/dust/actions/workflows/clippy.yml/badge.svg?branch=main&event=push)](https://github.com/kelpsyberry/dust/actions/workflows/clippy.yml?query=branch%3Amain+event%3Apush)
[![Discord server](https://dcbadge.vercel.app/api/server/MRDEvx8rKy?style=flat&theme=default-inverted)](https://discord.gg/MRDEvx8rKy)

![Screenshot](screenshot.png)

## Web version

[![Web deploy status](https://github.com/kelpsyberry/dust/actions/workflows/deploy-web.yml/badge.svg?branch=main&event=push)](https://github.com/kelpsyberry/dust/actions/workflows/deploy-web.yml?query=branch%3Amain+event%3Apush)

[Web frontend](https://dust-emu.netlify.app)

## Prebuilt binaries

[![Binary release build status](https://github.com/kelpsyberry/dust/actions/workflows/build-release.yml/badge.svg?branch=main&event=push)](https://github.com/kelpsyberry/dust/actions/workflows/build-release.yml?query=branch%3Amain+event%3Apush)
[![macOS app bundle release build status](https://github.com/kelpsyberry/dust/actions/workflows/build-release-macos-app-bundles.yml/badge.svg?branch=main&event=push)](https://github.com/kelpsyberry/dust/actions/workflows/build-release-macos-app-bundles.yml?query=branch%3Amain+event%3Apush)

- The base configuration only includes all features necessary to run games as an end user
- The debugging configuration additionally enables logging of diagnostic events on the emulated system (i.e. invalid I/O device usage or port accesses) and several debugging views (i.e. memory and register viewer and a disassembly view), all accessible from the Debug menu
- The debugging + GDB server configuration also enables support for a GDB client to attach to and debug the emulated program, by starting/stopping the server from the Debug menu while running a program

> **NB**: The debugging configurations only add debugging features for loaded programs; all binaries are compiled with optimizations and don't include debug symbols for the emulator itself.

| OS and binary type | Base | Debugging | Debugging + GDB server |
| ------------------ | ---- | --------- | ---------------------- |
| Windows x86_64 .exe | [Windows.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Windows.zip) | [Windows-debug.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Windows-debug.zip) | [Windows-debug-gdb.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Windows-debug-gdb.zip) |
| Linux x86_64 binary | [Linux.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Linux.zip) | [Linux-debug.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Linux-debug.zip) | [Linux-debug-gdb.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/Linux-debug-gdb.zip) |
| macOS universal app | [macOS-app.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release-macos-app-bundles/main/macOS-app.zip) | [macOS-app-debug.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release-macos-app-bundles/main/macOS-app-debug.zip) | [macOS-app-debug-gdb.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release-macos-app-bundles/main/macOS-app-debug-gdb.zip) |
| macOS x86_64 binary | [macOS-x86_64.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-x86_64.zip) | [macOS-x86_64-debug.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-x86_64-debug.zip) | [macOS-x86_64-debug-gdb.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-x86_64-debug-gdb.zip) |
| macOS ARM64 binary | [macOS-aarch64.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-aarch64.zip) | [macOS-aarch64-debug.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-aarch64-debug.zip) | [macOS-aarch64-debug-gdb.zip](https://nightly.link/kelpsyberry/dust/workflows/build-release/main/macOS-aarch64-debug-gdb.zip) |

## Credits
- Martin Korth, for summarizing resources on the DS on [GBATEK](https://problemkaputt.de/gbatek.htm)
- [Arisotura](https://github.com/Arisotura), for her research on the system in melonDS, [test ROMs](https://github.com/Arisotura/arm7wrestler) and [corrections and additions to the info on GBATEK](https://melonds.kuribo64.net/board/thread.php?id=13), and for the game database used in this emulator
- [StrikerX3](https://github.com/StrikerX3), for his research on 3D rendering on the DS
- [Simone Coco](https://github.com/CocoSimone), [Fleroviux](https://github.com/fleroviux), [Lady Starbreeze](https://github.com/LadyStarbreeze), [Merry](https://github.com/merryhime), [Powerlated](https://github.com/Powerlated) and [Peach](https://github.com/wheremyfoodat) for help throughout development
- The Emulation Development server on Discord as a whole, for providing several invaluable resources

## Sister projects
- [**Kaizen**](https://github.com/SimoneN64/Kaizen): Experimental work-in-progress low-level N64 emulator
- [**n64**](https://github.com/Dillonb/n64): Low-level, accurate, fast and easy to use Nintendo 64 emulator
- [**Panda3DS**](https://github.com/wheremyfoodat/Panda3DS): A new HLE Nintendo 3DS emulator
- [**melonDS**](https://github.com/melonDS-emu/melonDS): "DS emulator, sorta"; a DS emulator that focuses on being accurate and easy to use
- [**SkyEmu**](https://github.com/skylersaleh/SkyEmu): A low-level GameBoy, GameBoy Color, GameBoy Advance and Nintendo DS emulator that is designed to be easy to use, cross platform and accurate
- [**NanoBoyAdvance**](https://github.com/nba-emu/NanoBoyAdvance): A Game Boy Advance emulator focusing on hardware research and cycle-accurate emulation
- [**Chonkystation**](https://github.com/liuk7071/ChonkyStation): WIP PS1 emulator
