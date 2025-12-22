//! # Shell CLI Binary
//!
//! This binary provides a simple command-line interface (CLI) for interacting with the `comp-gate` core service.
//! It connects to the core service via TCP (using the address found in the connection file) and allows the user
//! to send commands interactively.
//!
//! ## Supported Commands
//!
//! - `get_device_list`: Retrieves the tree of connected devices.
//! - `get_device_connection_logs`: Retrieves the history of connection events.
//! - `disable_device <ID>`: Disables a specific device.
//! - `enable_device <ID>`: Enables a specific device.
//!
//! ## Usage
//!
//! Run this binary in a terminal. It will prompt with `>` for input.

use std::{
    io::{Read, Write},
    net,
};

use comp_gate::helper::ioapi::{IoApiCommand, IoApiRequest, get_core_connection_addr};

/// The main entry point for the Shell CLI.
///
/// It performs the following:
/// 1. Connects to the core service using `get_core_connection_addr`.
/// 2. Enters a Read-Eval-Print Loop (REPL).
/// 3. Reads user input from stdin.
/// 4. Parses the input into an `IoApiCommand`.
/// 5. Sends the command request to the core.
/// 6. Waits for and prints the response.
fn main() -> anyhow::Result<()> {
    let mut ioapi_stream = net::TcpStream::connect(get_core_connection_addr()?)
        .expect("Failed to connect to comp-gate core");

    loop {
        print!(">");
        // Ensure the prompt is displayed immediately
        let _ = std::io::stdout().flush();

        let mut cmd_buffer: String = String::new();
        std::io::stdin()
            .read_line(&mut cmd_buffer)
            .expect("Failed to read line");

        // Trim whitespace/newlines to ensure clean parsing
        let cmd_input = cmd_buffer.trim();
        if cmd_input.is_empty() {
            continue;
        }

        let request: IoApiRequest =
            match IoApiCommand::try_from(cmd_input.split(" ").collect::<Vec<&str>>().as_slice())
                .ok()
            {
                Some(cmd) => cmd.into(),
                None => {
                    println!("Invalid command");
                    continue;
                }
            };

        println!("{:?}", &*request);

        ioapi_stream
            .write_all(&request)
            .expect("Failed to write request");

        let mut prefix_buf = [0u8; 4];
        ioapi_stream
            .read_exact(&mut prefix_buf)
            .expect("Failed to read prefix size");

        let prefix_size: u32 = u32::from_be_bytes(prefix_buf);

        let mut body = vec![0u8; prefix_size as usize];
        if prefix_size > 0 {
            ioapi_stream
                .read_exact(&mut body)
                .expect("Failed to read message body");
        }

        match std::str::from_utf8(&body) {
            Ok(s) => println!("{}", s),
            Err(_) => println!("{:?}", body),
        }
    }
}
