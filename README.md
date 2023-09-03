# Krynth

Bandlimited Wavetable Oscillator

# Installaion

if you haven't already, install [Git](https://git-scm.com/downloads), then [Rust](https://www.rust-lang.org/tools/install), then install [nih-plug](https://github.com/robbert-vdh/nih-plug)'s plugin bundler using this command:

```
cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
```

Then run the following commands in your terminal:

```
git clone https://github.com/AquaEBM/Krynth.git
cd Krynth
cargo nih-plug bundle synth --release
```

From here, you can either copy the just created .vst3 or .clap bundle (found somewhere in "Krynth/target/release/bundled") into your system's VST3 or CLAP (if your DAW supports it) plugin folders, or add the folder containing it to the list of path's for your DAW to scan for when looking for plugins.

Finally, rescan the plugin paths, and it should be usable from your DAW.
