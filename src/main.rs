use axum::{
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File as StdFile, // Alias to avoid conflict with tokio::fs::File
    io::{BufRead, BufReader, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs as afs, // async file system operations
    process::Command as TokioCommand, // async command execution
    sync::RwLock,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, EnvFilter}; // Use EnvFilter for more flexible log level setting

// --- Configuration and State ---

#[derive(Parser, Debug)]
#[clap(
    name = "faddnsd-rust",
    version = env!("FADDNS_VERSION"), // Fetched by build.rs
    about = "Freakin' Awesome Dynamic DNS Server (Rust version)"
)]
struct Args {
    #[clap(value_parser)]
    zone: String,
    #[clap(value_parser)]
    zone_fn: PathBuf,
    #[clap(value_parser)]
    serial_fn: Option<PathBuf>,
    #[clap(long, short, value_parser, default_value_t = 80)]
    port: u16,
    #[clap(long)]
    no_zone_reload: bool,
    #[clap(long)]
    debug: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct Record {
    hostname: String,
    version: Option<String>,
    remote_addr: String,
    ether: Option<HashSet<String>>,
    inet: Option<HashSet<String>>,
    inet6: Option<HashSet<String>>,
}

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    records: Arc<RwLock<HashMap<String, Record>>>,
    datetimes: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    timestamps: Arc<RwLock<HashMap<String, i64>>>, // Unix timestamp
    changed_hosts: Arc<RwLock<HashSet<String>>>,
    unpaired_hosts: Arc<RwLock<HashSet<String>>>,
    do_pair_hosts: Arc<RwLock<HashSet<String>>>,
}

struct AppConfig {
    zone: String,
    zone_fn: PathBuf,
    serial_fn: PathBuf,
    out_fn: PathBuf, // Temporary file for zone updates
    no_zone_reload: bool,
}

// --- Utility Functions ---

fn dt_format(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn call_cmd(cmd_str: &str, args: &[&str], current_dir: Option<&Path>) -> Result<(), std::io::Error> {
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
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Command `{} {}` failed with status: {}", cmd_str, args.join(" "), status),
        ))
    }
}

async fn call_cmd_output(cmd_str: &str, args: &[&str]) -> Result<String, std::io::Error> {
    info!("+ {} {}", cmd_str, args.join(" "));
    let output = TokioCommand::new(cmd_str)
        .args(args)
        .output()
        .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command `{} {}` failed with status: {}. Stderr: {}",
                cmd_str, args.join(" "), output.status, stderr
            ),
        ))
    }
}

fn is_ip_restricted(ip_str: &str) -> bool {
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

// --- DNS Update Logic ---

async fn check_zone_file(zone: &str, zone_fn: &Path) -> Result<bool, std::io::Error> {
    debug!("check_zone: {} {}", zone, zone_fn.display());
    let zone_fn_str = zone_fn.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "zone_fn path is not valid UTF-8"))?;
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
    debug!("update_serial {} -> {}", serial_fn.display(), out_fn.display());

    let serial_fn_str = serial_fn.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "serial_fn path is not valid UTF-8"))?;
    let out_fn_str = out_fn.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "out_fn path is not valid UTF-8"))?;
    call_cmd("cp", &["-a", serial_fn_str, out_fn_str], None).await?;

    let file = StdFile::open(serial_fn)?; // Read original after copy
    let reader = BufReader::new(file);
    let mut temp_lines = Vec::new();
    let mut serial_done = false;
    let serial_re = Regex::new(r"(\d+)").unwrap();

    for line_result in reader.lines() {
        let mut line = line_result?;
        if !serial_done && line.to_lowercase().contains("serial") { // Python: 'erial' in line
            if let Some(caps) = serial_re.find(&line) { // Python: re.search('(\d+)', line).group(0)
                let serial_str = caps.as_str();
                if let Ok(serial_val) = serial_str.parse::<u32>() {
                    let new_serial_val = serial_val + 1;
                    line = line.replace(serial_str, &new_serial_val.to_string());
                    serial_done = true;
                    info!(
                        "{} serial: {} -> {}",
                        serial_fn.display(),
                        serial_val,
                        new_serial_val
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
        writeln!(outfile, "{}", line)?;
    }
    Ok(())
}

fn generate_bind_lines_for_record(record: &Record, dt: &DateTime<Utc>) -> String {
    let mut ret = String::new();
    let hostname = record.hostname.to_lowercase();
    let ttl = "10M"; // Hardcoded in Python

    if let Some(inet_addrs) = &record.inet {
        for addr in inet_addrs {
            if !is_ip_restricted(addr) {
                ret.push_str(&format!(
                    "{}\t{}\tIN\tA\t{} ; @faddns {}\n",
                    hostname,
                    ttl,
                    addr,
                    dt_format(dt)
                ));
                debug!("{} IN A {}", hostname, addr);
            }
        }
    }
    if let Some(inet6_addrs) = &record.inet6 {
        for addr in inet6_addrs {
            if !is_ip_restricted(addr) {
                ret.push_str(&format!(
                    "{}\t{}\tIN\tAAAA\t{} ; @faddns {}\n",
                    hostname,
                    ttl,
                    addr,
                    dt_format(dt)
                ));
                debug!("{} IN AAAA {}", hostname, addr);
            }
        }
    }
    ret
}

async fn update_zone_file_content(
    zone_fn: &Path,
    out_fn: &Path,
    records_map: &HashMap<String, Record>,
    datetimes_map: &HashMap<String, DateTime<Utc>>,
    mut changed_hosts_snapshot: HashSet<String>, // Consumes and modifies this set
    do_pair_set: &HashSet<String>,
) -> Result<HashSet<String>, std::io::Error> { // Returns remaining changed (unpaired)
    debug!("update_zone_file: {} -> {}", zone_fn.display(), out_fn.display());

    let zone_fn_str = zone_fn.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "zone_fn path is not valid UTF-8"))?;
    let out_fn_str = out_fn.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "out_fn path is not valid UTF-8"))?;
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

        if written_hosts_this_pass.contains(&host_in_zone) { // Already processed this host (e.g. multiple old entries)
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
                    debug!("change for {} contains no usable data, keeping old record", host_in_zone);
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
                info!("adding new host {} to zone file", host_to_add);
                let bind_lines = generate_bind_lines_for_record(rec, dt);
                if !bind_lines.is_empty() {
                    temp_lines.push(bind_lines.trim_end().to_string());
                    written_hosts_this_pass.insert(host_to_add.clone());
                    changed_hosts_snapshot.remove(&host_to_add); // Processed
                } else {
                    debug!("new host {} change contains no usable data, not adding", host_to_add);
                    // Python code would write the previous line if it existed, but for new hosts, it just skips.
                    // This means if a new host has no public IPs, it's not added, and remains in 'changed'.
                }
            }
        }
    }
    
    let mut outfile = StdFile::create(out_fn)?; // Overwrite out_fn
    for line in temp_lines {
        writeln!(outfile, "{}", line)?;
    }
    
    // Python: return changed - written. `changed` here is `changed_hosts_snapshot` after removals.
    // `written` in python is `written_hosts_this_pass`.
    // The hosts remaining in `changed_hosts_snapshot` are the new unpaired hosts.
    Ok(changed_hosts_snapshot)
}

async fn perform_dns_update_cycle(state: AppState) -> Result<(), String> {
    let config = state.config.clone();

    let mut changed_hosts_guard = state.changed_hosts.write().await;
    let current_changed_snapshot = changed_hosts_guard.clone(); // Clone the set of changes
    
    let do_pair_snapshot = state.do_pair_hosts.read().await.clone();
    let unpaired_snapshot = state.unpaired_hosts.read().await.clone();

    if current_changed_snapshot.is_empty() {
        debug!("No changes found, doing nothing.");
        return Ok(());
    }
    // Python: if not do_pair and changed == unpaired:
    if do_pair_snapshot.is_empty() && current_changed_snapshot == unpaired_snapshot {
        debug!("Only unforced hosts in changes, doing nothing.");
        return Ok(());
    }

    let records_snapshot = state.records.read().await.clone();
    let datetimes_snapshot = state.datetimes.read().await.clone();

    // update_zone_file_content modifies a copy of current_changed_snapshot and returns the new set of unpaired hosts
    let newly_unpaired = match update_zone_file_content(
        &config.zone_fn,
        &config.out_fn,
        &records_snapshot,
        &datetimes_snapshot,
        current_changed_snapshot.clone(), // Pass a clone
        &do_pair_snapshot,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => return Err(format!("Failed to update zone file content: {}", e)),
    };

    // Update global unpaired_hosts set
    *state.unpaired_hosts.write().await = newly_unpaired.clone(); // Store the new unpaired set

    // Zone file check (only if zone_fn is same as serial_fn as per Python logic)
    if config.zone_fn == config.serial_fn {
        if !check_zone_file(&config.zone, &config.out_fn).await.unwrap_or(false) {
            return Err("Zone check error!".to_string());
        }
    } else {
        debug!("Zone file and serial file are not the same, skipping check of main zone file content before serial update.");
    }

    if afs::metadata(&config.out_fn).await.map_err(|e| e.to_string())?.len() < 10 { // Python: assert os.path.getsize(out_fn) > 10
        return Err(format!("Temporary zone file {} is too small after update.", config.out_fn.display()));
    }
    
    // Python: call('mv %s %s' % (out_fn, zone_fn))
    let out_fn_str = config.out_fn.to_str().ok_or("out_fn path invalid UTF-8")?;
    let zone_fn_str = config.zone_fn.to_str().ok_or("zone_fn path invalid UTF-8")?;
    call_cmd("mv", &[out_fn_str, zone_fn_str], None).await.map_err(|e| format!("Failed to move {} to {}: {}", config.out_fn.display(), config.zone_fn.display(), e))?;
    info!("Moved {} to {}", config.out_fn.display(), config.zone_fn.display());

    // Update serial
    if let Err(e) = update_serial_in_file(&config.serial_fn, &config.out_fn).await { // out_fn is reused as temp for serial
         return Err(format!("Failed to update serial: {}", e));
    }
    if afs::metadata(&config.out_fn).await.map_err(|e| e.to_string())?.len() < 10 {
        return Err(format!("Temporary serial file {} is too small after update.", config.out_fn.display()));
    }

    // Python: call('mv %s %s' % (out_fn, serial_fn))
    let serial_fn_str = config.serial_fn.to_str().ok_or("serial_fn path invalid UTF-8")?;
    call_cmd("mv", &[out_fn_str, serial_fn_str], None).await.map_err(|e| format!("Failed to move {} to {}: {}", config.out_fn.display(), config.serial_fn.display(), e))?;
    info!("Moved {} to {}", config.out_fn.display(), config.serial_fn.display());

    // Sign zone: call('cd %s; dnssec-signzone -o %s %s' % (os.path.dirname(serial_fn), zone, serial_fn))
    let zone_dir = config.serial_fn.parent().ok_or("Cannot get parent directory of serial_fn")?;
    call_cmd("dnssec-signzone", &["-o", &config.zone, serial_fn_str], Some(zone_dir))
        .await.map_err(|e| format!("dnssec-signzone failed for zone {}: {}", config.zone, e))?;
    info!("dnssec-signzone successful for {}", config.zone);

    // Python: for host in changed.copy(): logging.warning('%s not processed!' % host)
    // 'changed' at this point in Python is the set of unpaired hosts.
    for host in newly_unpaired.iter() {
        warn!("Host {} was not processed and is now unpaired.", host);
    }
    
    // Clear processed hosts from global `changed_hosts` and `do_pair_hosts`
    // Hosts that were successfully written are NOT in `newly_unpaired`.
    // So, `current_changed_snapshot` - `newly_unpaired` = processed hosts.
    let processed_hosts: HashSet<_> = current_changed_snapshot.difference(&newly_unpaired).cloned().collect();
    let mut do_pair_hosts_guard = state.do_pair_hosts.write().await;
    for host in processed_hosts {
        changed_hosts_guard.remove(&host);
        if do_pair_snapshot.contains(&host) {
             do_pair_hosts_guard.remove(&host);
        }
    }
    // Drop the guard to release the lock
    drop(changed_hosts_guard);
    drop(do_pair_hosts_guard);


    if !config.no_zone_reload {
        if let Err(e) = call_cmd("rndc", &["reload", &config.zone], None).await {
            error!("rndc reload {} failed: {}", config.zone, e);
        } else {
            info!("rndc reload {} successful", config.zone);
        }
    }
    
    Ok(())
}

async fn dns_update_background_loop(state: AppState) {
    // Python: if t - t_last > 30
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        debug!("DNS update loop tick");
        // Python: print("HOVNO", t, t_last, ts_max) - this was just a debug print
        if let Err(e) = perform_dns_update_cycle(state.clone()).await {
            error!("Error in DNS update cycle: {}", e);
        }
    }
}

// --- Axum Handlers ---

#[derive(Deserialize, Debug)]
struct UpdateRequestParams {
    version: Option<String>,
    host: Option<String>,
    // Axum Query extractor handles multiple params with same name into Vec
    // Python code handles single string or list of strings for ether, inet, inet6
    // For simplicity, we'll expect them as Vec<String> if multiple, or String if single.
    // Serde can deserialize "val" into Some(vec!["val"]) or ["val1", "val2"] into Some(vec!["val1", "val2"])
    // if the field is Option<Vec<String>>. But kwargs in python is more flexible.
    // Let's define helper structs for query parameters to handle single or multiple values.
    #[serde(alias = "ether[]", default)]
    ether: Option<Vec<String>>,
    #[serde(alias = "inet[]", default)]
    inet: Option<Vec<String>>,
    #[serde(alias = "inet6[]", default)]
    inet6: Option<Vec<String>>,
}

async fn root_handler(
    State(state): State<AppState>,
    Query(params): Query<UpdateRequestParams>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let host_name = match params.host {
        Some(h) => h.to_lowercase(),
        None => {
            debug!("No host specified, ignoring");
            return Html(
                "<html><body><p>no host specified</p>
                <p><a href=\"/listhosts\">listhosts</a></p>
                <p><a href=\"/dump\">dump</a></p></body></html>",
            )
            .into_response();
        }
    };

    let client_ip_str = if let Some(x_forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = x_forwarded_for.to_str() {
            // X-Forwarded-For can contain multiple IPs separated by commas, take the first one
            xff_str.split(',').next().unwrap_or("").trim().to_string()
        } else {
            addr.ip().to_string()
        }
    } else {
        addr.ip().to_string()
    };

    let current_record = Record {
        hostname: host_name.clone(),
        version: params.version,
        remote_addr: client_ip_str,
        ether: params.ether.map(|v| v.into_iter().collect()),
        inet: params.inet.map(|v| v.into_iter().collect()),
        inet6: params.inet6.map(|v| v.into_iter().collect()),
    };

    debug!("Received record: {:?}", current_record);

    let mut records_guard = state.records.write().await;
    let mut datetimes_guard = state.datetimes.write().await;
    let mut timestamps_guard = state.timestamps.write().await;
    let mut changed_hosts_guard = state.changed_hosts.write().await;

    let previous_record = records_guard.get(&host_name);

    if previous_record.map_or(true, |pr| pr != &current_record) {
        debug!("Record change for {}: {:?}", host_name, current_record);
        records_guard.insert(host_name.clone(), current_record); // This clones current_record
        changed_hosts_guard.insert(host_name.clone());
    }

    let now = Utc::now();
    datetimes_guard.insert(host_name.clone(), now);
    timestamps_guard.insert(host_name.clone(), now.timestamp());

    "OK".into_response()
}

#[derive(Serialize)]
struct DumpEntry {
    #[serde(flatten)]
    record: Record, // Will be cloned
    datetime: String,
    t: i64,
}

// Corresponds to Python's dump2 (JSON array)
async fn dump_handler(State(state): State<AppState>) -> Json<Vec<DumpEntry>> {
    let records_guard = state.records.read().await;
    let datetimes_guard = state.datetimes.read().await;
    let timestamps_guard = state.timestamps.read().await;

    let mut result_list = Vec::new();

    for (host, rec_ref) in records_guard.iter() {
        // Clone the record for modification and inclusion in DumpEntry
        let rec_clone = rec_ref.clone(); 
        
        // Convert HashSets to Vecs for JSON, as Python code does list(v)
        // Serde can serialize HashSet to array, but to match Python's explicit list:
        // The previous `if let Some(ether_set) = &rec_clone.ether { ... }` block was empty
        // and `ether_set` was unused. Serde handles HashSet serialization to JSON array.

        result_list.push(DumpEntry {
            record: rec_clone, // Use the cloned record
            datetime: datetimes_guard.get(host).map_or_else(String::new, dt_format),
            t: timestamps_guard.get(host).copied().unwrap_or(0),
        });
    }
    Json(result_list)
}


#[derive(Deserialize, Debug)]
struct AddHostParams {
    host: String,
}

async fn addhost_handler(
    State(state): State<AppState>,
    Query(params): Query<AddHostParams>,
) -> Html<String> {
    let host_to_add = params.host.to_lowercase();
    info!("Forced addition of {}", host_to_add);
    state.do_pair_hosts.write().await.insert(host_to_add.clone());
    Html(format!("will add {}", host_to_add))
}

async fn listhosts_handler(State(state): State<AppState>) -> Html<String> {
    let records_guard = state.records.read().await;
    let datetimes_guard = state.datetimes.read().await;
    let unpaired_guard = state.unpaired_hosts.read().await;

    let mut ret = String::from("<html><body><table>");
    ret.push_str(
        "<tr><th>hostname</th><th>datetime</th><th>version</th>
        <th>ether</th><th>inet</th><th>inet6</th>
        <th>remote_addr</th><th>ops</th></tr>",
    );

    let mut sorted_hosts: Vec<String> = records_guard.keys().cloned().collect();
    sorted_hosts.sort(); // Python sorts keys

    for host_name in sorted_hosts {
        if let Some(rec) = records_guard.get(&host_name) {
            ret.push_str("<tr>");
            ret.push_str(&format!("<td>{}</td>", rec.hostname));
            ret.push_str(&format!(
                "<td>{}</td>",
                datetimes_guard.get(&host_name).map_or_else(String::new, dt_format)
            ));
            ret.push_str(&format!("<td>{}</td>", rec.version.as_deref().unwrap_or("")));

            for af_val_opt in [&rec.ether, &rec.inet, &rec.inet6] {
                ret.push_str("<td>");
                if let Some(vals_set) = af_val_opt {
                    // Sort for consistent output, though Python's set join order is arbitrary
                    let mut vals_vec: Vec<String> = vals_set.iter().cloned().collect();
                    vals_vec.sort();
                    ret.push_str(&vals_vec.join("<br/>"));
                }
                ret.push_str("</td>");
            }
            ret.push_str(&format!("<td>{}</td>", rec.remote_addr));

            if unpaired_guard.contains(&host_name) {
                ret.push_str(&format!(
                    "<td><a href=\"/addhost?host={}\">add</a></td>",
                    host_name // Already percent-encoded by format! if needed, but hostnames are usually safe
                ));
            } else {
                ret.push_str("<td></td>");
            }
            ret.push_str("</tr>");
        }
    }

    ret.push_str("</table></body></html>");
    Html(ret)
}

// --- Main Function ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let log_level_filter = if args.debug {
        EnvFilter::new("info,faddns_rust=debug") // Show info for all, debug for our crate
    } else {
        EnvFilter::new("info") // Show info for all
    };
    fmt().with_env_filter(log_level_filter).init();
    
    // Python: if not debug: cherrypy.log.access_log.propagate = False ...
    // Axum access logging is typically handled by Tower layers like TraceLayer if detailed logs are needed.
    // The default tracing setup will log basic request info if spans are used.

    info!("Starting faddnsd-rust version {}", env!("FADDNS_VERSION"));
    debug!("Arguments: {:?}", args);

    let final_serial_fn = match args.serial_fn.clone() {
        Some(sf) => sf,
        None => {
            info!(
                "No serial_fn specified, assuming it to be the same as zone_fn: {}",
                args.zone_fn.display()
            );
            args.zone_fn.clone()
        }
    };

    // Validate file paths (Python: try open().close())
    if StdFile::open(&args.zone_fn).is_err() {
        error!("Unable to open {} for reading", args.zone_fn.display());
        return Ok(()); // Exits with 0 like Python script
    }
    if StdFile::open(&final_serial_fn).is_err() {
        error!("Unable to open {} for reading", final_serial_fn.display());
        return Ok(());
    }
    
    // Python: out_fn = '/tmp/%s.zone_tmp' % zone
    let out_fn_path = std::env::temp_dir().join(format!("{}.zone_tmp", args.zone));

    let app_config = Arc::new(AppConfig {
        zone: args.zone.clone(),
        zone_fn: args.zone_fn.clone(),
        serial_fn: final_serial_fn,
        out_fn: out_fn_path,
        no_zone_reload: args.no_zone_reload,
    });

    let shared_state = AppState {
        config: app_config,
        records: Arc::new(RwLock::new(HashMap::new())),
        datetimes: Arc::new(RwLock::new(HashMap::new())),
        timestamps: Arc::new(RwLock::new(HashMap::new())),
        changed_hosts: Arc::new(RwLock::new(HashSet::new())),
        unpaired_hosts: Arc::new(RwLock::new(HashSet::new())),
        do_pair_hosts: Arc::new(RwLock::new(HashSet::new())),
    };

    let state_clone_for_loop = shared_state.clone();
    tokio::spawn(async move {
        dns_update_background_loop(state_clone_for_loop).await;
    });

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/dump", get(dump_handler)) // Corresponds to Python's dump2
        .route("/addhost", get(addhost_handler))
        .route("/listhosts", get(listhosts_handler))
        .with_state(shared_state);

    let listener_addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("Listening on http://{}", listener_addr);
    
    let listener = tokio::net::TcpListener::bind(listener_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>()
    )
    .await?;

    // Python: _run = 0; thr.join()
    // Tokio tasks are typically managed by the runtime. Graceful shutdown can be added here
    // by listening for signals (e.g., Ctrl-C) and coordinating task termination.
    // For now, when main exits, Tokio runtime shuts down.

    Ok(())
}
