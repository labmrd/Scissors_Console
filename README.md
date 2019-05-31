# Scissors Console

## Cloning
This repository uses git submodules, to fully clone the repository use:
```
git clone [URL] --recursive
```

If you already cloned and didn't specify `--recursive`, you can pull the submodules instead by calling:
```
git submodule update --init
```

## Building
To build the code its as simple as:
```
cargo build [--release]
```

Building and/or opening the documentation is just:
```
cargo doc [--open]
```

Running the code can be done using `cargo` by:
```
cargo run [--release]
```
or by double clicking the built executable in the `./target/[debug|release]` folder within the project root directory.

## Build Requirements
The code should build provided you have a working Rust compiler setup (including VS 2019 build tools if on Windows 10) and have installed version 18.6 of the NIDAQ-mx drivers. If you're on Linux you'll need to install `Webkit2GTK 2.8` from your distro's package manager.

On Windows the NI drivers can be installed by downloading and installing the NI Package Manager. On Linux follow the instructions to download and install the RPM file from NI on a Redhat based OS.

## Project Structure
| Folder               | Purpose                                                                             |
|----------------------|-------------------------------------------------------------------------------------|
| nativefiledialog-rs  | Nicer to work with bindings to the NFD C library                                    |
| nativefiledialog-sys | Raw bindings to the NFD C library                                                   |
| nidaqmx-rs           | Nicer to work with bindings to nidaqmx.h, uses Futures 0.1 to abstract the async io |
| nidaqmx-sys          | Raw C bindings to nidaqmx.h                                                         |
| scissors_console     | The binary that runs the data collection and GUI                                    |
