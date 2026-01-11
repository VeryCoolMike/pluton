use cliclack::{input, intro, log, outro, confirm};
use console::style;
use rand::{distributions::Alphanumeric, Rng};
use libsql::{Builder, params};
use pluton_core::account_management::check_password_strength;

pub async fn create_server() -> anyhow::Result<()> {
    let server_data_path = std::path::Path::new("server_data.db");
    if server_data_path.exists() {
        println!("A server has already been made.");
        return Ok(());
    }

    intro(style(" Pluton Server Creation ").on_blue().black())?;
    log::remark("All settings can be changed later with --configure_server")?;

    let server_name: String = input("What would you like your server to be called?")
        .placeholder("")
        .validate(|input: &String| {
            if input.is_empty() {
                Err("Please enter a name.")
            } else if input.len() > 64 {
                Err("A server's name must be 64 or less characters.")
            } else {
                Ok(())
            }
        })
        .interact()?;

    let random_password: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let server_password: String = input("What would you like the password to be?")
        .default_input(&random_password)
        .validate(|input: &String| {
            if input.is_empty() {
                Err("Password must not be empty for security.")
            } else if input.len() > 255 {
                Err("The password must be 255 or less characters.")
            } else {
                Ok(())
            }
        })
        .interact()?;

    println!("{}", check_password_strength(server_password.clone()).await);

    if !confirm("Are you sure you want to set this as your new password?").interact()? {
        outro("Password has not been set. Exiting!");
        return Ok(());
    }


    let db = Builder::new_local("server_data.db").build().await?;
    let conn = db.connect()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS server (
            server_name varchar(64) NOT NULL,
            server_password varchar(255) NOT NULL,
            server_port varchar(8) NOT NULL,
            server_ip varchar(16) NOT NULL
        );", ()
    ).await?;

    conn.execute(
        "INSERT INTO server (server_name, server_password, server_port, server_ip)
        VALUES (?, ?, ?, ?);
        ", params![server_name, server_password, "6767", "127.0.0.1"]
    ).await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            public_key TEXT PRIMARY KEY,
            permission INTEGER
        );", ()
    ).await?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY,
            sender BLOB,
            plaintext TEXT,
            timestamp INTEGER            
        );", ()
    ).await?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_time_id
        ON messages(timestamp DESC, id DESC);", ()
    ).await?;

    outro("Server has been made, please run --start_server")?;

    Ok(())
}
