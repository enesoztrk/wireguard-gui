use std::fs;
use std::io::{self, Error, Result, Write,Read};
use std::process::*;
use std::time::Duration;
use wait_timeout::ChildExt;
use crate::config::{parse_config, WireguardConfig};

pub const TUNNELS_PATH: &str = "/etc/wireguard";

pub fn load_existing_configurations() -> Result<Vec<WireguardConfig>> {
    let mut cfgs = vec![];

    for entry in fs::read_dir(TUNNELS_PATH)? {
        let file = entry?;
        if file.file_type()?.is_file() && file.path().extension().map_or(false, |e| e == "conf") {
            let file_path = file.path();
            let file_content = fs::read_to_string(&file_path)?;
            let mut cfg = parse_config(&file_content).map_err(Error::other)?;
            if cfg.interface.name.is_none() {
                if let Some(file_name) = file_path.file_stem().and_then(|n| n.to_str()) {
                    cfg.interface.name = Some(file_name.to_string());
                }
            }
            cfgs.push(cfg);
        }
    }

    Ok(cfgs)
}

pub fn generate_private_key() -> Result<String> {
    let output = Command::new("wg")
        .arg("genkey")
        .stdout(Stdio::piped())
        .output()?;

    String::from_utf8(output.stdout)
        .map(|s| s.trim().into())
        .map_err(|_| io::Error::other("Could not convert output of `wg genkey` to utf-8 string."))
}

pub fn generate_public_key(priv_key: String) -> Result<String> {
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn child process");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin
            .write_all(priv_key.trim().as_bytes())
            .expect("Failed to write to stdin");
    });

    let output = child.wait_with_output().expect("Failed to read stdout");

    if output.stdout.is_empty() {
        return Err(io::Error::other("Failed to generate public key"));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().into())
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::Other,
                "Could not convert output of `wg pubkey` to utf-8 string.",
            )
        })
}


    /// Run a command with a timeout and return the exit status and stdout output.
pub fn run_cmd_with_timeout(
        cmd: &mut Child,
        timeout: u64,
    ) -> io::Result<(Option<i32>, String)> {
      
        let one_sec = Duration::from_secs(timeout);
        println!("Running command: {:?}", cmd);
        // Wait for the process to exit or timeout
        let status_code = match cmd.wait_timeout(one_sec)? {
            Some(status) => {
                cmd.kill()?;
                status.code()
            }
            None => {
                // Process hasn't exited yet, kill it and get the status code
                println!("Killing the process: {:?}", cmd);
                cmd.kill()?;
                cmd.wait()?.code()
            }
        };

        // Read stdout after killing the process
        let mut s = String::new();

        // Borrow stdout and read the output into `s`
        if let Some(stdout) = cmd.stdout.as_mut() {
            stdout.read_to_string(&mut s)?;
        }

        // Print the output line-by-line
        println!("Status code: {:?},output:{:?}", status_code, s);
        // Return both the status code and the output
        Ok((status_code, s))
    }