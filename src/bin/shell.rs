use std::{
    io::{Read, Write},
    net,
};

use comp_gate::helper::ioapi::{IoApiCommand, IoApiRequest, get_core_connection_addr};

fn main() -> anyhow::Result<()> {
    let mut ioapi_stream = net::TcpStream::connect(get_core_connection_addr()?)
        .expect("Failed to connect to comp-gate core");

    loop {
        print!(">");
        let mut cmd_buffer: String = String::new();
        std::io::stdin()
            .read_line(&mut cmd_buffer)
            .expect("Failed to read line");

        let request: IoApiRequest =
            match IoApiCommand::try_from(cmd_buffer.split(" ").collect::<Vec<&str>>().as_slice())
                .ok()
            {
                Some(cmd) => cmd.into(),
                None => {
                    println!("Invalid command");
                    continue;
                }
            };

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
