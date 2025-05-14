use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;
use std::process::Command;

// Let user compile based on access to ec_sys
// user does not have access -> use the acpi_ec project
#[cfg(feature = "acpi_ec")]
const EC_IO_FILE: &str = "/dev/ec";

// user have access to ec_sys
#[cfg(not(feature = "acpi_ec"))]
const EC_IO_FILE: &str = "/sys/kernel/debug/ec/ec0/io";

// use a custom fan curve
#[cfg(feature = "fan_custom")]
const USE_FAN_CURVE: bool = true;

#[cfg(not(feature = "fan_custom"))]
const USE_FAN_CURVE: bool = false;

// use a specific bios mode
#[cfg(feature = "performance_mode")]
const USE_PERFORMANCE_MODE: bool = true;

#[cfg(not(feature = "performance_mode"))]
const USE_PERFORMANCE_MODE: bool = false;

#[cfg(feature = "cool_mode")]
const USE_COOL_MODE: bool = true;

#[cfg(not(feature = "cool_mode"))]
const USE_COOL_MODE: bool = false;

const PERFORMANCE_OFFSET: u64 = 0x95;
const FAN1_OFFSET: u64 = 0x34; // Fan 1 Speed Set (units of 100RPM)
const FAN2_OFFSET: u64 = 0x35; // Fan 2 Speed Set (units of 100RPM)
const CPU_TEMP_OFFSET: u64 = 0x57; // CPU Temp (°C)
const GPU_TEMP_OFFSET: u64 = 0xB7; // GPU Temp (°C)
const BIOS_CONTROL_OFFSET: u64 = 0x62; // BIOS Control
const FAN1_MAX: u8 = 55; // Max speed for Fan 1
const FAN2_MAX: u8 = 57; // Max speed for Fan 2
const BIOS_LEGACY_DEFAULT_MODE: u8 = 0; // Bios default mode on init
const BIOS_DEFAULT_MODE: u8 = 48; // Bios default mode
const BIOS_PERFORMANCE_MODE: u8 = 49; // Bios performance mode
const BIOS_COOL_MODE: u8 = 80; // Bios cool mode

fn load_ec_sys_module() {
    // check which ec module is used
    if EC_IO_FILE == "/dev/ec" {
        // do nothing, the module is always allowed in write
        return
    }
    else {
        // Check if the `ec_sys` module is loaded
        let output = Command::new("lsmod")
            .output()
            .expect("Failed to execute `lsmod` command.");
        if !String::from_utf8_lossy(&output.stdout).contains("ec_sys") {
            // Load the `ec_sys` module with write support
            Command::new("modprobe")
                .args(&["ec_sys", "write_support=1"])
                .status()
                .expect("Failed to load `ec_sys` module.");
        }
    }
}

fn read_ec_register(offset: u64) -> u8 {
    let mut file = File::open(EC_IO_FILE).expect("Failed to open EC IO file. Ensure you have the necessary permissions.");
    file.seek(SeekFrom::Start(offset))
        .expect("Failed to seek to EC register.");
    let mut buffer = [0u8; 1];
    file.read_exact(&mut buffer)
        .expect("Failed to read EC register.");
    buffer[0]
}

fn write_ec_register(offset: u64, value: u8) {
    let mut file = OpenOptions::new()
        .write(true)
        .open(EC_IO_FILE)
        .expect("Failed to open EC IO file. Ensure you have the necessary permissions.");
    file.seek(SeekFrom::Start(offset))
        .expect("Failed to seek to EC register.");
    file.write_all(&[value])
        .expect("Failed to write to EC register.");
}

fn get_max_temp() -> u8 {
    let cpu_temp = read_ec_register(CPU_TEMP_OFFSET);
    let gpu_temp = read_ec_register(GPU_TEMP_OFFSET);
    cpu_temp.max(gpu_temp)
}

fn set_fan_speed(fan1_speed: u8, fan2_speed: u8) {
    write_ec_register(FAN1_OFFSET, fan1_speed);
    write_ec_register(FAN2_OFFSET, fan2_speed);
}

fn disable_bios_control() {
    write_ec_register(BIOS_CONTROL_OFFSET, 0x06); // Disable BIOS control
}

fn apply_bios_mode(mode: u8) {
    write_ec_register(PERFORMANCE_OFFSET, mode);
}

fn mode() -> String{
    let perf_offset: u8 =  read_ec_register(PERFORMANCE_OFFSET);
    match perf_offset {
        0x30 => {
            "Normal Mode".to_string()
        }
        0x31 => {
            "Performance Mode".to_string()
        }
        0x40 => {
            "Cool Mode".to_string()
        }
        _ => {
            "Undefined Mode".to_string()
        }
    }
}

fn get_current_mode() -> (String, u8){
    let mode;
    let value;
    if USE_COOL_MODE {
        mode = "Cool Mode".to_string();
        value = BIOS_COOL_MODE;
    }
    else if USE_PERFORMANCE_MODE {
        mode = "Performance Mode".to_string();
        value = BIOS_PERFORMANCE_MODE;
    }
    else {
        mode = "Default Mode".to_string();
        value = BIOS_DEFAULT_MODE;
    }
    (mode, value)
}

fn temp_to_performance(temp: u8) -> u8{
    match temp {
    86..=u8::MAX => {
        write_ec_register(PERFORMANCE_OFFSET, 0x30);
        93
    }
       0..=85 => {
            write_ec_register(PERFORMANCE_OFFSET, 0x31);
            80
        }
    }
}


// fn enable_bios_control() {
//    write_ec_register(BIOS_CONTROL_OFFSET, 0x00); // Enable BIOS control
// }

fn main() {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("Root access is required to run this program.");
        exit(1);
    }

    // Perform setup tasks
    load_ec_sys_module();

    let idle_speed = 0;
    let poll_interval = Duration::from_secs(1);

    let mut previous_speed = (0, 0);

    loop {
        
        let current_mode = mode();
        println!("The mode is: {current_mode}");

        if USE_FAN_CURVE {
            disable_bios_control();
            let temp = get_max_temp();
            println!("Current temperature: {}°C", temp);
            temp_to_performance(temp);
            let speed = match temp {
                0..=45 => idle_speed,
                46..=50 => 20,
                51..=55 => 37,
                56..=70 => 45,
                71..=75 => 50,
                76..=80 => 70,
                81..=85 => 80,
                86..93 => 90,
                _ => 100,
            };

            let fan1_speed = ((FAN1_MAX as u16 * speed as u16) / 100) as u8;
            let fan2_speed = ((FAN2_MAX as u16 * speed as u16) / 100) as u8;

            if previous_speed != (fan1_speed, fan2_speed) {
                set_fan_speed(fan1_speed, fan2_speed);
                previous_speed = (fan1_speed, fan2_speed);
            }
        }
        else {
            let (bios_mode, value) = get_current_mode();
            if bios_mode != current_mode {
                apply_bios_mode(value);
            }
            println!("The new mode is: {bios_mode}");
        }

        sleep(poll_interval);
    }
}
