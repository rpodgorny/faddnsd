pub mod web;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File as StdFile,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{process::Command as TokioCommand, sync::RwLock};
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Record {
    pub hostname: String,
    pub version: Option<String>,
    pub remote_addr: String,
    pub ether: Option<HashSet<String>>,
    pub inet: Option<HashSet<String>>,
    pub inet6: Option<HashSet<String>>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub records: Arc<RwLock<HashMap<String, Record>>>,
    pub datetimes: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    pub timestamps: Arc<RwLock<HashMap<String, i64>>>, // Unix timestamp
    pub changed_hosts: Arc<RwLock<HashSet<String>>>,
    pub unpaired_hosts: Arc<RwLock<HashSet<String>>>,
    pub do_pair_hosts: Arc<RwLock<HashSet<String>>>,
}

pub struct AppConfig {
    pub zone: String,
    pub zone_fn: PathBuf,
    pub serial_fn: PathBuf,
    pub out_fn: PathBuf, // Temporary file for zone updates
    pub no_zone_reload: bool,
}

pub fn dt_format(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn call_cmd(
    cmd_str: &str,
    args: &[&str],
    current_dir: Option<&Path>,
) -> Result<(), std::io::Error> {
    info!("+ {} {}", cmd_str, args.join(" "));
    let mut command = TokioCommand::new(cmd_str);
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    let status = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "Command `{} {}` failed with status: {}",
            cmd_str,
            args.join(" "),
            status
        )))
    }
}

async fn call_cmd_output(cmd_str: &str, args: &[&str]) -> Result<String, std::io::Error> {
    info!("+ {} {}", cmd_str, args.join(" "));
    let output = TokioCommand::new(cmd_str).args(args).output().await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::other(format!(
            "Command `{} {}` failed with status: {}. Stderr: {}",
            cmd_str,
            args.join(" "),
            output.status,
            stderr
        )))
    }
}

pub fn is_ip_restricted(ip_str: &str) -> bool {
    match ip_str.parse::<std::net::IpAddr>() {
        Ok(ip_addr) => {
            // Using std::net::IpAddr methods
            if ip_addr.is_loopback() || ip_addr.is_multicast() {
                return true;
            }
            match ip_addr {
                std::net::IpAddr::V4(ipv4) => {
                    // Link-local: 169.254.0.0/16
                    // Private: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                    ipv4.is_private() || ipv4.is_link_local()
                }
                std::net::IpAddr::V6(ipv6) => {
                    // Link-local: fe80::/10
                    // Unique Local Addresses (ULA): fc00::/7
                    // Site-local (deprecated but sometimes seen): fec0::/10
                    // Check for these common restricted ranges.
                    // is_unicast_link_local_strict() is not stable yet.
                    let segments = ipv6.segments();
                    (segments[0] & 0xffc0) == 0xfe80 // Link-local
                    || (segments[0] & 0xfe00) == 0xfc00 // ULA
                    || (segments[0] & 0xffc0) == 0xfec0 // Site-local (deprecated)
                }
            }
        }
        Err(_) => true, // If parsing fails, treat as restricted to be safe
    }
}

async fn check_zone_file(zone: &str, zone_fn: &Path) -> Result<bool, std::io::Error> {
    debug!("check_zone: {zone} {}", zone_fn.display());
    let zone_fn_str = zone_fn.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "zone_fn path is not valid UTF-8",
        )
    })?;
    match call_cmd_output("named-checkzone", &[zone, zone_fn_str]).await {
        Ok(output) => {
            debug!("{}", output);
            Ok(output.contains("OK"))
        }
        Err(e) => {
            error!("named-checkzone command failed: {}", e);
            Err(e)
        }
    }
}

async fn update_serial_in_file(serial_fn: &Path, out_fn: &Path) -> Result<(), std::io::Error> {
    debug!(
        "update_serial {} -> {}",
        serial_fn.display(),
        out_fn.display()
    );

    let serial_fn_str = serial_fn.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "serial_fn path is not valid UTF-8",
        )
    })?;
    let out_fn_str = out_fn.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "out_fn path is not valid UTF-8",
        )
    })?;
    call_cmd("cp", &["-a", serial_fn_str, out_fn_str], None).await?;

    let file = StdFile::open(serial_fn)?; // Read original after copy
    let reader = BufReader::new(file);
    let mut temp_lines = Vec::new();
    let mut serial_done = false;
    let serial_re = Regex::new(r"(\d+)").unwrap();

    for line_result in reader.lines() {
        let mut line = line_result?;
        if !serial_done && line.to_lowercase().contains("erial") {
            // Python: 'erial' in line
            if let Some(caps) = serial_re.find(&line) {
                // Python: re.search('(\d+)', line).group(0)
                let serial_str = caps.as_str();
                if let Ok(serial_val) = serial_str.parse::<u32>() {
                    let new_serial_val = serial_val + 1;
                    line = line.replace(serial_str, &new_serial_val.to_string());
                    serial_done = true;
                    info!(
                        "{} serial: {serial_val} -> {new_serial_val}",
                        serial_fn.display(),
                    );
                }
            }
        }
        temp_lines.push(line);
    }

    if !serial_done {
        error!("Failed to update serial in {}", serial_fn.display());
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Failed to find and update serial",
        ));
    }

    let mut outfile = StdFile::create(out_fn)?; // Overwrite out_fn
    for line in temp_lines {
        writeln!(outfile, "{line}")?;
    }
    Ok(())
}

pub fn generate_bind_lines_for_record(record: &Record, dt: &DateTime<Utc>) -> String {
    let mut ret = String::new();
    let hostname = record.hostname.to_lowercase();
    let ttl = "10M"; // Hardcoded in Python

    if let Some(inet_addrs) = &record.inet {
        for addr in inet_addrs {
            if is_ip_restricted(addr) {
                continue;
            }
            ret.push_str(&format!(
                "{hostname}\t{ttl}\tA\t{addr} ; @faddns {}\n",
                dt_format(dt)
            ));
            debug!("{hostname} IN A {addr}");
        }
    }
    if let Some(inet6_addrs) = &record.inet6 {
        for addr in inet6_addrs {
            if is_ip_restricted(addr) {
                continue;
            }
            ret.push_str(&format!(
                "{hostname}\t{ttl}\tAAAA\t{addr} ; @faddns {}\n",
                dt_format(dt)
            ));
            debug!("{hostname} IN AAAA {addr}");
        }
    }
    ret
}

pub async fn update_zone_file_content(
    zone_fn: &Path,
    out_fn: &Path,
    records_map: &HashMap<String, Record>,
    datetimes_map: &HashMap<String, DateTime<Utc>>,
    mut changed_hosts_snapshot: HashSet<String>, // Consumes and modifies this set
    do_pair_set: &HashSet<String>,
) -> Result<HashSet<String>, std::io::Error> {
    // Returns remaining changed (unpaired)
    debug!(
        "update_zone_file: {} -> {}",
        zone_fn.display(),
        out_fn.display()
    );

    let zone_fn_str = zone_fn.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "zone_fn path is not valid UTF-8",
        )
    })?;
    let out_fn_str = out_fn.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "out_fn path is not valid UTF-8",
        )
    })?;
    call_cmd("cp", &["-a", zone_fn_str, out_fn_str], None).await?;

    let input_file = StdFile::open(zone_fn)?; // Read original after copy
    let reader = BufReader::new(input_file);
    let mut temp_lines = Vec::new();
    let mut written_hosts_this_pass = HashSet::new();

    for line_result in reader.lines() {
        let line = line_result?;
        if !line.contains("@faddns") {
            temp_lines.push(line);
            continue;
        }

        let host_in_zone = line.split_whitespace().next().unwrap_or("").to_lowercase();
        if host_in_zone.is_empty() {
            temp_lines.push(line); // Should not happen with @faddns
            continue;
        }

        if written_hosts_this_pass.contains(&host_in_zone) {
            // Already processed this host (e.g. multiple old entries)
            continue;
        }

        if !changed_hosts_snapshot.contains(&host_in_zone) {
            debug!("{} not in changes, skipping", host_in_zone);
            temp_lines.push(line);
            continue;
        }

        // Host is in changed_hosts_snapshot, needs update
        if let Some(rec) = records_map.get(&host_in_zone) {
            if let Some(dt) = datetimes_map.get(&host_in_zone) {
                info!("updating {}", host_in_zone);
                let bind_lines = generate_bind_lines_for_record(rec, dt);
                if !bind_lines.is_empty() {
                    temp_lines.push(bind_lines.trim_end().to_string());
                } else {
                    debug!("change for {host_in_zone} contains no usable data, keeping old record");
                    temp_lines.push(line); // Keep original line
                }
                written_hosts_this_pass.insert(host_in_zone.clone());
                changed_hosts_snapshot.remove(&host_in_zone); // Processed
            }
        } else {
            // Host in changes but not in records_map? Should not happen. Keep original.
            temp_lines.push(line);
        }
    }

    // Process new hosts (in changed_hosts_snapshot but not in original zone file, AND in do_pair)
    // Python: for host in changed.copy(): if host not in do_pair: continue ...
    let hosts_to_potentially_add: Vec<String> = changed_hosts_snapshot.iter().cloned().collect();
    for host_to_add in hosts_to_potentially_add {
        if !do_pair_set.contains(&host_to_add) {
            continue;
        }
        if let Some(rec) = records_map.get(&host_to_add) {
            if let Some(dt) = datetimes_map.get(&host_to_add) {
                info!("adding new host {host_to_add} to zone file");
                let bind_lines = generate_bind_lines_for_record(rec, dt);
                if !bind_lines.is_empty() {
                    temp_lines.push(bind_lines.trim_end().to_string());
                    written_hosts_this_pass.insert(host_to_add.clone());
                    changed_hosts_snapshot.remove(&host_to_add); // Processed
                } else {
                    debug!("new host {host_to_add} change contains no usable data, not adding");
                    // Python code would write the previous line if it existed, but for new hosts, it just skips.
                    // This means if a new host has no public IPs, it's not added, and remains in 'changed'.
                }
            }
        }
    }

    let mut outfile = StdFile::create(out_fn)?; // Overwrite out_fn
    for line in temp_lines {
        writeln!(outfile, "{line}")?;
    }

    // Python: return changed - written. `changed` here is `changed_hosts_snapshot` after removals.
    // `written` in python is `written_hosts_this_pass`.
    // The hosts remaining in `changed_hosts_snapshot` are the new unpaired hosts.
    Ok(changed_hosts_snapshot)
}

