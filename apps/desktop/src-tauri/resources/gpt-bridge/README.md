# Windows bridge package resources

Run `scripts/windows/prepare-gpt-bridge.ps1` from a native Windows checkout before
building Tauri. It publishes the .NET 10 bridge and runtime here as self-contained
Windows x64 resources, so the installed app does not require a .NET installation.
Generated binaries are ignored. `scripts/windows/build.ps1` runs this preparation
before any Cargo/Tauri build; no startup download or runtime package installation
is performed. Missing resources safely leave manual handoff available.

The desktop parent launches `Entwurf.Bridge.exe` only for bounded, authenticated,
read-only discovery and then closes/reaps it. The current bridge cannot pass all
four automation gates and performs no ChatGPT input, submit, or capture actions.
