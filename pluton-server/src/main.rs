use std::env;

use cliclack::{clear_screen, set_theme};

mod theming;
mod creation;
mod configuration;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    clear_screen()?;
    set_theme(theming::MagentaTheme);

    let args: Vec<String> = env::args().collect();
    if args.len() == 2 {
        match args[1].as_str() {
            "--create_server" => {
                return creation::create_server().await; 
            }
            "--start_server" => {
                return server::start_server().await;
            }
            "--configure_server" => {
                return configuration::configure_server().await; 
            }
            "--help" => {
                println!("Available options:\n--create_server\n--start_server\n--configure_server\n--help");
            }
            other => {
                println!("Unknown arguement: {}", other);
            }
        }
    }
    else if args.len() == 3 {
        match (args[1].as_str(), args[2].as_str()) {
            ("--start_server", "--gui") => {
                return server::start_server().await;
            }
            (other_arg_1, other_arg_2) => {
                eprintln!("Unknown arguement: {} {}", other_arg_1, other_arg_2);
            }
        }
    } else {
        let server_data_path = std::path::Path::new("server_data.db");
        if !server_data_path.exists() {
            creation::create_server().await?
        }

        return server::start_server().await;
    }

    Ok(())
}
