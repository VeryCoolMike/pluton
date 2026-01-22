use cliclack::{input, intro, outro, select, confirm};
use console::style;
use rand::{distributions::Alphanumeric, Rng};
use libsql::{Builder, params};
use pluton_core::account_management::check_password_strength;
use std::net::IpAddr;

pub async fn configure_server() -> anyhow::Result<()> {
    let server_data_path = std::path::Path::new("server_data.db");
    if !server_data_path.exists() {
        println!("Please create a server first");
        return Ok(());
    }

    intro(style(" Pluton Server Configuration ").on_blue().black())?;

    let db = Builder::new_local("server_data.db").build().await?;
    let conn = db.connect()?;

    let option_selection = choose_option(&conn).await?;

    match option_selection {
        "password" => {
            return change_password(conn).await;
        }
        "name" => {
            return change_server_name(conn).await; 
        }
        "port" => {
            return change_server_port(conn).await;
        }
        "ip" => {
            return change_server_ip(conn).await;
        }
        _ => { }
    }

    Ok(())
}

pub async fn choose_option(conn: &libsql::Connection) -> anyhow::Result<&str> {
    let mut password_rows = conn.query("SELECT server_password FROM server", params![]).await?;
    let password = if let Some(row) = password_rows.next().await? {
        row.get::<String>(0)?
    } else {
        "<no password>".to_string()
    };

    let mut server_name_rows = conn.query("SELECT server_name FROM server", params![]).await?;
    let server_name = if let Some(row) = server_name_rows.next().await? {
        row.get::<String>(0)?
    } else {
        "<no server name>".to_string()
    };

    let mut server_port_rows = conn.query("SELECT server_port FROM server", params![]).await?;
    let server_port = if let Some(row) = server_port_rows.next().await? {
        row.get::<String>(0)?
    } else {
        "<no port>".to_string()
    };

    let mut server_ip_rows = conn.query("SELECT server_ip FROM server", params![]).await?;
    let server_ip = if let Some(row) = server_ip_rows.next().await? {
        row.get::<String>(0)?
    } else {
        "<no port>".to_string()
    };

    let option_selection = select("What setting would you like to modify")
        .item("password", "Server Password", password)
        .item("name", "Server Name", server_name)
        .separator("Advanced")
        .item("port", "Port", server_port)
        .item("ip", "IP Adress", server_ip)
        .interact()?;

    Ok(option_selection)
}

pub async fn change_password(conn: libsql::Connection) -> anyhow::Result<()> {
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
        outro("Password has not been changed. Exiting!");
        return Ok(());
    }

    conn.execute(
        "
            UPDATE server
            SET server_password = (?);
        ", params![server_password]
    ).await?;
    
    let mut new_password_row = conn.query("SELECT server_password FROM server", ()).await?;
    if let Some(row) = new_password_row.next().await? {
        let new_password: String = row.get(0)?;
        outro(&format!("Password updated to {}", new_password))?;
    } else {
        outro("[ERROR] No password found in database")?;
    }

    Ok(())
}

pub async fn change_server_name(conn: libsql::Connection) -> anyhow::Result<()> {
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

    conn.execute(
        "
            UPDATE server
            SET server_name = (?);
        ", params![server_name]
    ).await?;

    let mut new_server_name_row = conn.query("SELECT server_name FROM server", ()).await?;
    if let Some(row) = new_server_name_row.next().await? {
        let new_server_name: String = row.get(0)?;
        outro(&format!("Server Name updated to {}", new_server_name))?;
    } else {
        outro("[ERROR] No password found in database")?;
    }

    Ok(())
}

pub async fn change_server_port(conn: libsql::Connection) -> anyhow::Result<()> {
    let server_port: String = input("What would you like to change the port to?")
        .placeholder("")
        .validate(|input: &String| {
            match input.parse::<u16>() {
                Ok(value) => {
                    if value == 0 {
                        Err("Port number out of range (1-65535)")
                    } else {
                        Ok(())
                    }
                },
                Err(_) => Err("Please enter a valid port number")
            }
        })
        .interact()?;

    conn.execute(
        "
            UPDATE server
            SET server_port = (?);
        ", params![server_port]
    ).await?;

    let mut new_server_port_row = conn.query("SELECT server_port FROM server", ()).await?;
    if let Some(row) = new_server_port_row.next().await? {
        let new_server_port: String = row.get(0)?;
        outro(&format!("Server Port updated to {}", new_server_port))?;
    } else {
        outro("[ERROR] No server port found in database")?;
    }

    Ok(())
}

pub async fn change_server_ip(conn: libsql::Connection) -> anyhow::Result<()> {
    let server_port: String = input("What would you like to change the IP address to?")
        .placeholder("")
        .validate(|input: &String| {
            match input.parse::<IpAddr>() {
                Ok(_) => {
                    Ok(())
                },
                Err(_) => Err("Please enter a valid IP address")
            }
        })
        .interact()?;

    conn.execute(
        "
            UPDATE server
            SET server_ip = (?);
        ", params![server_port]
    ).await?;

    let mut new_server_ip_row = conn.query("SELECT server_ip FROM server", ()).await?;
    if let Some(row) = new_server_ip_row.next().await? {
        let new_server_ip: String = row.get(0)?;
        outro(&format!("Server IP address updated to {}", new_server_ip))?;
    } else {
        outro("[ERROR] No server IP found in database")?;
    }

    Ok(())
}
