# MythrilOsc

Bandlimited Wavetable Oscillator

## Installaion

if you haven't already, install [Git](https://git-scm.com/downloads), then [Rust](https://www.rust-lang.org/tools/install), then install [nih-plug](https://github.com/robbert-vdh/nih-plug)'s plugin bundler using this command:

```shell
cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
```

Then run the following commands in your terminal:

```shell
git clone https://github.com/AquaEBM/MythrilOsc.git
cd MythrilOsc
cargo +nightly nih-plug bundle mythril_osc --release
```

From here, you can either copy (or symlink) the just created .vst3 or .clap bundle (found somewhere in "target/release/bundled") into your system's VST3 or CLAP (if your DAW supports it) plugin folders, or add the folder containing it to the list of path's for your DAW to scan for when looking for plugins (don't forget to rescan plugin paths)
