use async_std::task::{self};
use std::process::exit;
use std::time::Duration;
use std::sync::LazyLock;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use iced::widget::{button, column, container, pick_list, row, text, Container};
use iced::futures::lock::Mutex;
use iced::{futures, Alignment, Task};

// Let user compile based on access to ec_sys
// user does not have access -> use the acpi_ec project
#[cfg(feature = "acpi_ec")]
const EC_IO_FILE: &str = "/dev/ec";

// user have access to ec_sys
#[cfg(not(feature = "acpi_ec"))]
const EC_IO_FILE: &str = "/sys/kernel/debug/ec/ec0/io";

const PERFORMANCE_OFFSET: u64 = 0x95;
const FAN1_OFFSET: u64 = 0x34; // Fan 1 Speed Set (units of 100RPM)
const FAN2_OFFSET: u64 = 0x35; // Fan 2 Speed Set (units of 100RPM)
const CPU_TEMP_OFFSET: u64 = 0x57; // CPU Temp (°C)
const GPU_TEMP_OFFSET: u64 = 0xB7; // GPU Temp (°C)
const BIOS_CONTROL_OFFSET: u64 = 0x62; // BIOS Control
const FAN1_MAX: u8 = 55; // Max speed for Fan 1
const FAN2_MAX: u8 = 57; // Max speed for Fan 2

fn read_ec_register(offset: u64) -> u8 {
    let mut file = File::open(EC_IO_FILE)
        .expect("Failed to open EC IO file. Ensure you have the necessary permissions.");
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

fn enable_bios_control() {
    write_ec_register(BIOS_CONTROL_OFFSET, 0x00); // Enable BIOS control
}

fn apply_bios_mode(mode: u8) {
    write_ec_register(PERFORMANCE_OFFSET, mode);
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

fn get_mode_from_ec() -> String {
    let perf_offset: u8 =  read_ec_register(PERFORMANCE_OFFSET);
    match perf_offset {
        0x30 => {
            "Default Mode".to_string()
        }
        0x31 => {
            "Performance Mode".to_string()
        }
        0x50 => {
            "Cool Mode".to_string()
        }
        0x00 => {
            "Legacy Default Mode".to_string()
        }
        _ => {
            "Undefined Mode".to_string()
        }
    }
}

fn mode(value: u8) -> String {
    match value {
        0x30 => {
            "Default Mode".to_string()
        }
        0x50 => {
            "Cool Mode".to_string()
        }
        0x31 => {
            "Performance Mode".to_string()
        }
        0 => {
            "Custom Mode".to_string()
        }
        1..=48 => {
            "Default Mode".to_string()
        }
        50..=79 => {
            "Default Mode".to_string()
        }
        81..=u8::MAX => {
            "Default Mode".to_string()
        }
    }
}

fn mode_to_int(mode: &str) -> u8 {
    match mode {
        "Default Mode" => {
            0x30
        }
        "Cool Mode" => {
            0x50
        }
        "Performance Mode" => {
            0x31
        }
        "Custom Mode" => {
            0
        }
        _ => 0
    }
}

#[derive(Debug, Default)]
struct OmenFanGui {
    options: Vec<String>,
    selected_option: Option<String>,
    chosen_mode: String,
    //current_mode: Arc<Mutex<String>>,
}

#[derive(Debug, Clone)]
enum Message {
    ListUpdated(String),
    Run,
}

static SENT_MODE: LazyLock<Mutex<u8>> = LazyLock::new(|| Mutex::new(0x30));

#[tokio::main]
async fn main() -> iced::Result {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("Root access is required to run this program.");
        exit(1);
    }

    // try to read the ec before using the program
    // will allow the program to crash on fail
    // otherwise, it will fail only the async function
    read_ec_register(CPU_TEMP_OFFSET);

    // start the async function that communicate with the ec
    tokio::spawn(async move {
        run_fan_control().await
    });

    // start the gui part
    iced::application("Omen Fan Gui", OmenFanGui::update, OmenFanGui::view)
        .window_size(iced::Size::new(320.0, 160.0))
        .run_with(OmenFanGui::new)
}

impl OmenFanGui {

    // assign default values to the gui before displaying it
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                options: 
                    vec![
                        "Default Mode".to_string(),
                        "Cool Mode".to_string(),
                        "Performance Mode".to_string(),
                        "Custom Mode".to_string(),
                    ], 
                    selected_option: Some("Default Mode".to_string()), 
                    chosen_mode: "Default Mode".to_string(),
            },
            Task::none()
        )
    }

    // Handle what should happen when we interact with a gui element
    fn update(&mut self, message: Message) {
        match message {
            // If the list is updated -> grabing the value to send it when the button is pressed
            Message::ListUpdated(option) => {
                self.selected_option = Some(option);
                let sel_option = self.selected_option.clone().unwrap();
                println!("Option : {sel_option}");
                if sel_option != self.chosen_mode {
                    self.chosen_mode = sel_option;
                }
            }
            // Send the value by taking control of a shared value with the async value
            // and writing the value to it.
            // Making it useable by the function when releasing the control
            Message::Run => {
                futures::executor::block_on(async {
                    let mode = self.chosen_mode.clone();
                    let value = mode_to_int(mode.as_str());
                    let mut data = SENT_MODE.lock().await;
                    println!("Sent : {mode}");
                    *data = value;
                    drop(data);
                });
            }
        }
    }

    // Define what the gui app will look like
    fn view(&self) -> Container<Message> {
        // adding a list to choose between modes
        let mode_list = pick_list(
            self.options.clone(),
            self.selected_option.clone(), 
            |s| Message::ListUpdated(s),
        )
        .placeholder("Choose a fan mode");

        // Add a Ok button to validate the new mode selected in the list
        let valid_button = button("Ok")
            .on_press(Message::Run);

        // Add a row component to have the text and the list on the same line
        let list_row = row![
            text("Choose a fan mode"),
            mode_list,
        ].spacing(10)
        .align_y(Alignment::Center);

        // Use a column to add the button below the row
        let content = column![
            list_row,
            valid_button,
        ]
        .align_x(Alignment::Center)
        .spacing(20);

        // Place all of that on a container ( the window )
        container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
    }
}

// async function that take most of the old code
async fn run_fan_control(){
    let idle_speed = 0;
    let poll_interval = Duration::from_secs(1);

    println!("fan_control started !");

    let mut previous_speed = (0, 0);
    let mut already_throttling = false;

    loop {
        // Each loop, try to take control of the shared value
        let lock = SENT_MODE.lock().await;
        let lock_mode = *lock;
        // Get the value stored and keep it for use in the rest of the code
        let chosen_mode = mode(lock_mode);
        // Release the lock to let the gui write a new value if needed
        drop(lock);

        // Get the current mode from the the ec
        let current_mode = get_mode_from_ec();

        // Test if the chosen mode is Custom 
        // we don't need to test current mode as we deactivate the ec control
        if chosen_mode == String::from("Custom Mode") {
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
        // Else compare ec and chosen mode to see if a new mode have been requested
        // Only one apply because changing the mode do not start the reset countdown
        else if current_mode != chosen_mode {
            // changing the mode
            let ec_mode = mode_to_int(&chosen_mode);
            println!("Applying {ec_mode}");
            apply_bios_mode(ec_mode);
        }

        // Take over the fan to boost them when the temperature is too high
        if get_max_temp() > 95 && chosen_mode != String::from("Custom Mode") {
            if !already_throttling {
                println!("CPU is thermal throttling ! taking over the fans")
            }
            disable_bios_control();
            set_fan_speed(46, 44);
            already_throttling = true;
        }
        // give back the control to the ec when the temperature is back under control
        else {
            enable_bios_control();
            already_throttling = false;
        }

        task::sleep(poll_interval).await;
    }
}