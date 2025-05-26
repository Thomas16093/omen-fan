# omen-fan-gui
- A simple utility to manually control the fans of a HP Omen laptop
- Works on various HP Omen laptop and even some Victus laptops from testing. 
- Let you choose between integrated ec fan mode or integrated fan mode.
- Supports enabling boost mode via sysfs ( used when cpu reach +95°C ).
- Made and tested on an Omen 16-c0140AX
- Rust made and tested on Omen 16-n0xxx series, Omen 15-dc10xxxx and Omen 15-en1xxx

# Development status
In progress, everything said below is not yet push on the repo
- the gui can talk to the internal program asynchronously.
- The gui is not sized correctly and take too much space
- Goal is to make a tray control too :
    - Need to wait on Iced 14.0
    - Or migrate to Tauri

# WARNING
- Forcing this program to run on incompatible laptops may cause hardware damage. Use at your own risk.
- Max speed of the fans are configured based on the "Boost" state. Increasing them is not recommended and won't provide huge thermal beinifits.

# Documentation
- Use `omen-fan help` to see all available subcommands
- EC Probe documentation can be found at [docs/probes.md](https://github.com/alou-S/omen-fan/blob/main/docs/probes.md)

# Building
- Building with the [acpi_ec](https://github.com/saidsay-so/acpi_ec) project :
    - cargo build --release --features acpi_ec

- Building with a different fan mode :
    - Available modes :
        - default_mode : Default mode (Windows Omen Gaming Hub value)
        - performance_mode : Perfomance mode
        - cool_mode : Cool mode
        - fan_custom : Custom fan curve (Internal fan curve created by [Biswas005](https://github.com/Biswas005))
    - Example :
        - cargo build --release --features cool_mode

- Modifying both feature at the same time require a different writing : 
    - For features acpi_ec + cool_mode : 
        - cargo build --release --features "acpi_ec,cool_mode"

# Silverblue
-copy the target from release folder
-sudo cp /var/home/user-name/omen-fan/omen-fan/target/release/omen-fan /usr/local/bin/
replace user
--Then add service file to the system.
