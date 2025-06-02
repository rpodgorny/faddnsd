use faddnsd::*;
use clap::Parser;
use std::{
    collections::{HashMap, HashSet},
    fs::File as StdFile, // Alias to avoid conflict with tokio::fs::File
    io::{BufRead, Write},
    net::SocketAddr,
    path::{PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs as afs,                        // async file system operations
    process::Command as TokioCommand, // async command execution
    sync::RwLock,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, EnvFilter}; // Use EnvFilter for more flexible log level setting

// --- Configuration and State ---

#[derive(Parser, Debug)]
#[clap(
    name = "faddnsd-rust",
    version = env!("CARGO_PKG_VERSION"),
    about = "Freakin' Awesome Dynamic DNS Server (Rust version)"
)]
struct Args {
    #[clap(value_parser)]
    zone: String,
    #[clap(value_parser)]
    zone_fn: PathBuf,
    #[clap(value_parser)]
    serial_fn: Option<PathBuf>,
    #[clap(long, short, value_parser, default_value_t = 8765)]
    port: u16,
    #[clap(long)]
    no_zone_reload: bool,
    #[clap(long)]
    no_zone_sign: bool,
    #[clap(long)]
    debug: bool,
}


// --- Utility Functions ---

async fn call_cmd(
    cmd_str: &str,
    args: &[&str],
    current_dir: Option<&std::path::Path>,
) -> Result<(), std::io::Error> {
    info!("+ {} {}", cmd_str, args.join(" "));
    let mut command = TokioCommand::new(cmd_str);
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    let status = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            format!(
                "Command `{} {}` failed with status: {}",
                cmd_str,
                args.join(" "),
                status
            ),
        ))
    }
}

async fn call_cmd_output(cmd_str: &str, args: &[&str]) -> Result<String, std::io::Error> {
    info!("+ {} {}", cmd_str, args.join(" "));
    let output = TokioCommand::new(cmd_str).args(args).output().await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::other(
            format!(
                "Command `{} {}` failed with status: {}. Stderr: {}",
                cmd_str,
                args.join(" "),
                output.status,
                stderr
            ),
        ))
    }
}

// --- DNS Update Logic ---

async fn check_zone_file(zone: &str, zone_fn: &std::path::Path) -> Result<bool, std::io::Error> {
    debug!("check_zone: {} {}", zone, zone_fn.display());
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

async fn update_serial_in_file(serial_fn: &std::path::Path, out_fn: &std::path::Path) -> Result<(), std::io::Error> {
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
    let reader = std::io::BufReader::new(file);
    let mut temp_lines = Vec::new();
    let mut serial_done = false;
    let serial_re = regex::Regex::new(r"(\d+)").unwrap();

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
    let newly_unpaired = match faddnsd::update_zone_file_content(
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
        if !check_zone_file(&config.zone, &config.out_fn)
            .await
            .unwrap_or(false)
        {
            return Err("Zone check error!".to_string());
        }
    } else {
        debug!("Zone file and serial file are not the same, skipping check of main zone file content before serial update.");
    }

    if afs::metadata(&config.out_fn)
        .await
        .map_err(|e| e.to_string())?
        .len()
        < 10
    {
        // Python: assert os.path.getsize(out_fn) > 10
        return Err(format!(
            "Temporary zone file {} is too small after update.",
            config.out_fn.display()
        ));
    }

    // Python: call('mv %s %s' % (out_fn, zone_fn))
    let out_fn_str = config.out_fn.to_str().ok_or("out_fn path invalid UTF-8")?;
    let zone_fn_str = config
        .zone_fn
        .to_str()
        .ok_or("zone_fn path invalid UTF-8")?;
    call_cmd("mv", &[out_fn_str, zone_fn_str], None)
        .await
        .map_err(|e| {
            format!(
                "Failed to move {} to {}: {}",
                config.out_fn.display(),
                config.zone_fn.display(),
                e
            )
        })?;
    info!(
        "Moved {} to {}",
        config.out_fn.display(),
        config.zone_fn.display()
    );

    // Update serial
    if let Err(e) = update_serial_in_file(&config.serial_fn, &config.out_fn).await {
        // out_fn is reused as temp for serial
        return Err(format!("Failed to update serial: {}", e));
    }
    if afs::metadata(&config.out_fn)
        .await
        .map_err(|e| e.to_string())?
        .len()
        < 10
    {
        return Err(format!(
            "Temporary serial file {} is too small after update.",
            config.out_fn.display()
        ));
    }

    // Python: call('mv %s %s' % (out_fn, serial_fn))
    let serial_fn_str = config
        .serial_fn
        .to_str()
        .ok_or("serial_fn path invalid UTF-8")?;
    call_cmd("mv", &[out_fn_str, serial_fn_str], None)
        .await
        .map_err(|e| {
            format!(
                "Failed to move {} to {}: {}",
                config.out_fn.display(),
                config.serial_fn.display(),
                e
            )
        })?;
    info!(
        "Moved {} to {}",
        config.out_fn.display(),
        config.serial_fn.display()
    );

    // Sign zone: call('cd %s; dnssec-signzone -o %s %s' % (os.path.dirname(serial_fn), zone, serial_fn))
    if !config.no_zone_sign {
        let zone_dir = config
            .serial_fn
            .parent()
            .ok_or("Cannot get parent directory of serial_fn")?;
        call_cmd(
            "dnssec-signzone",
            &["-o", &config.zone, serial_fn_str],
            Some(zone_dir),
        )
        .await
        .map_err(|e| format!("dnssec-signzone failed for zone {}: {}", config.zone, e))?;
        info!("dnssec-signzone successful for {}", config.zone);
    } else {
        info!("Zone signing skipped due to --no-zone-sign flag");
    }

    // Python: for host in changed.copy(): logging.warning('%s not processed!' % host)
    // 'changed' at this point in Python is the set of unpaired hosts.
    for host in newly_unpaired.iter() {
        warn!("Host {} was not processed and is now unpaired.", host);
    }

    // Clear processed hosts from global `changed_hosts` and `do_pair_hosts`
    // Hosts that were successfully written are NOT in `newly_unpaired`.
    // So, `current_changed_snapshot` - `newly_unpaired` = processed hosts.
    let processed_hosts: HashSet<_> = current_changed_snapshot
        .difference(&newly_unpaired)
        .cloned()
        .collect();
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

    info!("Starting faddnsd-rust version {}", env!("CARGO_PKG_VERSION"));
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
        no_zone_sign: args.no_zone_sign,
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

    let app = faddnsd::web::create_router(shared_state.clone());

    let listener_addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("Listening on http://{}", listener_addr);

    let listener = tokio::net::TcpListener::bind(listener_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    // Python: _run = 0; thr.join()
    // Tokio tasks are typically managed by the runtime. Graceful shutdown can be added here
    // by listening for signals (e.g., Ctrl-C) and coordinating task termination.
    // For now, when main exits, Tokio runtime shuts down.

    Ok(())
}
