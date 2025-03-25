use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
};

use gtk::prelude::*;
use relm4::prelude::*;

use crate::config::*;
use crate::utils::*;
use getifaddrs::{getifaddrs, InterfaceFlags};
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use wait_timeout::ChildExt;
#[derive(PartialEq)]
pub enum NetState {
    IPLINK_UP = 0x01,
    IPLINK_DOWN = 0x02,
    WG_QUICK_UP = 0x04,
    WG_QUICK_DOWN = 0x08,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct Tunnel {
    pub name: String,
    pub config: WireguardConfig,
    pub active: bool,
}

impl Tunnel {
    pub fn new(config: WireguardConfig) -> Self {
        let name = config.interface.name.clone().unwrap_or("unknown".into());

        let mut active = false;

        active = Self::is_wg_iface_running(&name) == NetState::WG_QUICK_UP;

        Self {
            name,
            active,
            config,
        }
    }

    /// Run a command with a timeout and return the exit status and stdout output.
    fn run_cmd_with_timeout(
        cmd: &mut Child,
        timeout: u64,
    ) -> Result<(Option<i32>, String), io::Error> {
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

    fn is_interface_up(interface_name: &str) -> Result<bool, std::io::Error> {
        let ifaddrs = getifaddrs().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to get interfaces")
        })?;

        for interface in ifaddrs {
            if interface.name == interface_name {
                return Ok(interface.flags.contains(InterfaceFlags::UP)
                    && interface.flags.contains(InterfaceFlags::RUNNING));
            }
        }

        Ok(false)
    }

    fn is_wg_iface_running(interface: &str) -> NetState {
        // Run `wg show <interface>`
        let mut wg_output = Command::new("wg")
            .arg("show")
            .arg(interface)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to execute wg show");
        println!("wg show {}", interface);
        if !Self::run_cmd_with_timeout(&mut wg_output, 5)
            .map(|(code, output)| code == Some(0) && !output.is_empty())
            .unwrap_or(false)
        {
            println!("Interface {} is not running", interface);
            return NetState::WG_QUICK_DOWN;
        }

        if !Self::is_interface_up(interface).unwrap_or(false) {
            return NetState::IPLINK_DOWN;
        }
        println!("Interface {} is running", interface);
        NetState::WG_QUICK_UP
    }

    pub fn path(&self) -> PathBuf {
        Path::new(TUNNELS_PATH).join(format!("{}.conf", self.name))
    }

    /// Toggle the WireGuard interface using wireguard-tools.
    pub fn try_toggle(&mut self) -> Result<(), io::Error> {
        
        let is_endpoint_valid = |config: &WireguardConfig| -> Result<(), io::Error> {
            for peer in config.peers.iter() {
                if let Some(endpoint) = peer.endpoint.as_ref() {
                    // Try to parse the endpoint into a SocketAddr
                    if SocketAddr::from_str(endpoint).is_err() {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Invalid endpoint format",
                        ));
                    }
                }
            }
            Ok(())
        }; 

        // Helper closure to run a command and check its success
        let run_wg_quick = |action: &str| -> Result<(), io::Error> {
            let mut cmd = Command::new("wg-quick")
                .args([action, &self.name])
                .spawn()?;
            println!("wg-quick {}", action);
            if !Self::run_cmd_with_timeout(&mut cmd, 3)
                .map(|(code, _)| code == Some(0))
                .unwrap_or(false)
            {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to execute wg-quick {}", action),
                ));
            }
            Ok(())
        };

        let state = Self::is_wg_iface_running(self.name.as_str());

        // Check if the endpoint is valid before wireguard inteface is up
        if state !=  NetState::WG_QUICK_UP {

            is_endpoint_valid(&self.config)?;
        }

        match state {
            NetState::IPLINK_DOWN => {
                run_wg_quick("down")?;
                run_wg_quick("up")?;
            }
            NetState::WG_QUICK_UP => {
                run_wg_quick("down")?;
            }
            NetState::WG_QUICK_DOWN => {
                run_wg_quick("up")?;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Unknown interface state",
                ))
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum TunnelMsg {
    Toggle,
}

#[derive(Debug)]
pub enum TunnelOutput {
    Remove(DynamicIndex),
    Error(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for Tunnel {
    type Init = WireguardConfig;
    type Input = TunnelMsg;
    type Output = TunnelOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 5,

            #[name(switch)]
            gtk::Switch {
                set_active: self.active,
                connect_state_notify => Self::Input::Toggle,
            },

            gtk::Label {
                set_label: &self.name,
            },

            gtk::Button::with_label("Remove") {
                connect_clicked[sender, index] => move |_| {
                    sender.output(Self::Output::Remove(index.clone())).unwrap();
                }
            },
        }
    }

    fn init_model(config: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self::new(config)
    }

    fn update_with_view(
        &mut self,
        _widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: relm4::FactorySender<Self>,
    ) {
        match msg {
            Self::Input::Toggle => {
                match self.try_toggle() {
                    Ok(_) => self.active = !self.active,
                    Err(err) => sender
                        .output_sender()
                        .emit(Self::Output::Error(err.to_string())),
                };
                println!("state: {}", self.active);
                _widgets.switch.set_state(self.active);
            }
        }
    }
}
