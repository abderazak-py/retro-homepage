use std::fs;
use std::time::Duration;
use std::path::PathBuf;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use tower_http::cors::CorsLayer;

#[derive(Serialize, Clone, Debug)]
struct BatteryInfo {
    percentage: i32,
    status: String,
    temperature: f32,
    health: String,
}

#[derive(Serialize, Clone, Debug)]
struct CpuInfo {
    usage_percent: f32,
    cores: usize,
    threads: usize,
    frequency_mhz: u32,
    architecture: String,
}

#[derive(Serialize, Clone, Debug)]
struct RamInfo {
    total_mb: u64,
    used_mb: u64,
    available_mb: u64,
}

#[derive(Serialize, Clone, Debug)]
struct StorageDetail {
    total_gb: f32,
    used_gb: f32,
    available_gb: f32,
}

#[derive(Serialize, Clone, Debug)]
struct StorageInfo {
    internal: StorageDetail,
}

#[derive(Serialize, Clone, Debug)]
struct SystemStats {
    battery: BatteryInfo,
    cpu: CpuInfo,
    ram: RamInfo,
    storage: StorageInfo,
    uptime_seconds: u64,
}

// Struct to parse JSON from termux-battery-status command
#[derive(serde::Deserialize, Debug)]
struct TermuxBatteryStatus {
    percentage: i32,
    status: String,
    temperature: f32,
    health: String,
}

fn get_thermal_zone_temperature() -> Option<f32> {
    let mut best_zone_temp = None;
    let mut fallback_zone_temp = None;
    if let Ok(thermal_entries) = fs::read_dir("/sys/class/thermal") {
        for entry in thermal_entries.flatten() {
            let tz_path = entry.path();
            let tz_name = tz_path.file_name().unwrap_or_default().to_string_lossy();
            if tz_name.starts_with("thermal_zone") {
                let tz_type = fs::read_to_string(tz_path.join("type"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if let Ok(temp_str) = fs::read_to_string(tz_path.join("temp")) {
                    if let Ok(raw_temp) = temp_str.trim().parse::<f32>() {
                        let temp_c = raw_temp / 1000.0;
                        let lower_type = tz_type.to_lowercase();
                        if lower_type.contains("x86_pkg_temp") || 
                           lower_type.contains("coretemp") || 
                           lower_type.contains("cpu-thermal") {
                            best_zone_temp = Some(temp_c);
                            break; // Found preferred CPU zone
                        } else if lower_type.contains("acpitz") {
                            if fallback_zone_temp.is_none() {
                                fallback_zone_temp = Some(temp_c);
                            }
                        } else if fallback_zone_temp.is_none() {
                            fallback_zone_temp = Some(temp_c);
                        }
                    }
                }
            }
        }
    }
    best_zone_temp.or(fallback_zone_temp)
}

fn get_system_temperature(battery_path: Option<&std::path::PathBuf>) -> f32 {
    // 1. Try reading from /sys/class/thermal/thermal_zone* first (more accurate CPU temperature on Linux)
    if let Some(temp) = get_thermal_zone_temperature() {
        return temp;
    }

    // 2. Fallback to battery-specific temperature file under /sys/class/power_supply/BAT/temp
    if let Some(path) = battery_path {
        if let Ok(temp_str) = fs::read_to_string(path.join("temp")) {
            if let Ok(raw_temp) = temp_str.trim().parse::<f32>() {
                // sysfs temperature is usually in tenths of a degree Celsius (e.g. 352 for 35.2 C)
                // or sometimes in micro-degrees (e.g., 35200).
                return if raw_temp > 1000.0 { raw_temp / 1000.0 } else { raw_temp / 10.0 };
            }
        }
    }

    // 3. General fallback
    35.0
}

fn get_battery_info() -> BatteryInfo {
    // 1. Try to find a battery supply directory under /sys/class/power_supply
    let mut battery_path = None;
    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(type_str) = fs::read_to_string(path.join("type")) {
                if type_str.trim().eq_ignore_ascii_case("battery") {
                    battery_path = Some(path);
                    break;
                }
            }
        }
    }

    if let Some(ref path) = battery_path {
        let pct_res = fs::read_to_string(path.join("capacity"));
        let status_res = fs::read_to_string(path.join("status"));

        if let (Ok(pct_str), Ok(status_str)) = (pct_res, status_res) {
            let percentage = pct_str.trim().parse::<i32>().unwrap_or(0);
            let status = status_str.trim().to_string();
            let temperature = get_system_temperature(Some(path));

            // Health determination
            // Android battery sysfs has a direct "health" file (e.g. contains "Good")
            // Linux laptop can compute wear via charge_full / charge_full_design or energy_full / energy_full_design
            let mut health = String::new();

            if let Ok(health_str) = fs::read_to_string(path.join("health")) {
                health = health_str.trim().to_string();
            }

            if health.is_empty() {
                // Try computing wear/health
                let charge_full = fs::read_to_string(path.join("charge_full"))
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok());
                let charge_full_design = fs::read_to_string(path.join("charge_full_design"))
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok());

                let energy_full = fs::read_to_string(path.join("energy_full"))
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok());
                let energy_full_design = fs::read_to_string(path.join("energy_full_design"))
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok());

                let (full, design) = if let (Some(f), Some(d)) = (charge_full, charge_full_design) {
                    (Some(f), Some(d))
                } else if let (Some(f), Some(d)) = (energy_full, energy_full_design) {
                    (Some(f), Some(d))
                } else {
                    (None, None)
                };

                if let (Some(f), Some(d)) = (full, design) {
                    if d > 0.0 {
                        let health_pct = (f / d) * 100.0;
                        let health_status = if health_pct >= 80.0 {
                            "Good"
                        } else if health_pct >= 50.0 {
                            "Fair"
                        } else {
                            "Poor"
                        };
                        health = format!("{} ({:.0}%)", health_status, health_pct);
                    }
                }
            }

            if health.is_empty() {
                if let Ok(cap_lvl_str) = fs::read_to_string(path.join("capacity_level")) {
                    health = cap_lvl_str.trim().to_string();
                }
            }

            if health.is_empty() {
                health = "Good".to_string();
            }

            return BatteryInfo {
                percentage,
                status,
                temperature,
                health,
            };
        }
    }

    // 2. Fallback: try executing termux-battery-status command
    if let Ok(output) = std::process::Command::new("termux-battery-status").output() {
        if output.status.success() {
            if let Ok(status) = serde_json::from_slice::<TermuxBatteryStatus>(&output.stdout) {
                let temperature = get_thermal_zone_temperature().unwrap_or(status.temperature);
                return BatteryInfo {
                    percentage: status.percentage,
                    status: status.status,
                    temperature,
                    health: status.health,
                };
            }
        }
    }

    // 3. Fallback: Standard Linux (e.g. server/desktop) without battery
    let temperature = get_system_temperature(None);
    BatteryInfo {
        percentage: 100,
        status: "N/A".to_string(),
        temperature,
        health: "N/A".to_string(),
    }
}

struct CpuTicks {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

fn read_cpu_ticks() -> Option<CpuTicks> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let first_line = stat.lines().next()?;
    let mut parts = first_line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    
    let user = parts.next()?.parse::<u64>().ok()?;
    let nice = parts.next()?.parse::<u64>().ok()?;
    let system = parts.next()?.parse::<u64>().ok()?;
    let idle = parts.next()?.parse::<u64>().ok()?;
    let iowait = parts.next()?.parse::<u64>().ok()?;
    let irq = parts.next()?.parse::<u64>().ok()?;
    let softirq = parts.next()?.parse::<u64>().ok()?;
    let steal = parts.next()?.parse::<u64>().ok()?;

    Some(CpuTicks {
        user,
        nice,
        system,
        idle,
        iowait,
        irq,
        softirq,
        steal,
    })
}

fn get_cpu_cores_and_threads() -> (usize, usize) {
    let mut threads = 0;
    let mut physical_cores = 0;

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let mut core_keys = std::collections::HashSet::new();
        let mut current_phys_id = None;
        let mut current_core_id = None;
        let mut max_cpu_cores = 0;

        for line in cpuinfo.lines() {
            let mut parts = line.splitn(2, ':');
            let key = parts.next().map(|s| s.trim());
            let val = parts.next().map(|s| s.trim());

            if let (Some(k), Some(v)) = (key, val) {
                match k {
                    "processor" => {
                        threads += 1;
                        if let (Some(phys), Some(core)) = (current_phys_id, current_core_id) {
                            core_keys.insert((phys, core));
                        }
                        current_phys_id = None;
                        current_core_id = None;
                    }
                    "physical id" => {
                        current_phys_id = v.parse::<i32>().ok();
                    }
                    "core id" => {
                        current_core_id = v.parse::<i32>().ok();
                    }
                    "cpu cores" => {
                        if let Ok(c) = v.parse::<usize>() {
                            if c > max_cpu_cores {
                                max_cpu_cores = c;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let (Some(phys), Some(core)) = (current_phys_id, current_core_id) {
            core_keys.insert((phys, core));
        }

        if !core_keys.is_empty() {
            physical_cores = core_keys.len();
        } else if max_cpu_cores > 0 {
            physical_cores = max_cpu_cores;
        }
    }

    if threads == 0 {
        let sys_threads = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize };
        threads = if sys_threads == 0 { 8 } else { sys_threads };
    }

    if physical_cores == 0 {
        physical_cores = threads;
    }

    (physical_cores, threads)
}

async fn get_cpu_info() -> CpuInfo {
    let (cores, threads) = get_cpu_cores_and_threads();

    // Calculate CPU usage percent over 100ms
    let mut usage_percent = 0.0;
    if let Some(t1) = read_cpu_ticks() {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(t2) = read_cpu_ticks() {
            let total1 = t1.user + t1.nice + t1.system + t1.idle + t1.iowait + t1.irq + t1.softirq + t1.steal;
            let total2 = t2.user + t2.nice + t2.system + t2.idle + t2.iowait + t2.irq + t2.softirq + t2.steal;
            
            let idle1 = t1.idle + t1.iowait;
            let idle2 = t2.idle + t2.iowait;

            let total_delta = total2.saturating_sub(total1);
            let idle_delta = idle2.saturating_sub(idle1);

            if total_delta > 0 {
                usage_percent = 100.0 * (1.0 - (idle_delta as f32) / (total_delta as f32));
            }
        }
    }

    // Read CPU frequency
    // Check multiple possible sysfs files for current scaling frequency
    let mut frequency_mhz = 1600; // default nominal
    for cpu_idx in 0..threads {
        let freq_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", cpu_idx);
        if let Ok(freq_str) = fs::read_to_string(freq_path) {
            if let Ok(val) = freq_str.trim().parse::<u32>() {
                frequency_mhz = val / 1000; // Convert kHz to MHz
                break;
            }
        }
    }

    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64".to_string(),
        "aarch64" => "ARM64".to_string(),
        "arm" => "ARM".to_string(),
        "x86" => "x86".to_string(),
        arch => arch.to_uppercase(),
    };

    CpuInfo {
        usage_percent,
        cores,
        threads,
        frequency_mhz,
        architecture,
    }
}

fn get_ram_info() -> RamInfo {
    let mut total_mb = 2048; // nominal default
    let mut available_mb = 1024; // nominal default

    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let mut total_kb = None;
        let mut avail_kb = None;
        let mut free_kb = None;

        for line in meminfo.lines() {
            let mut parts = line.split_whitespace();
            let key = parts.next();
            let val = parts.next().and_then(|v| v.parse::<u64>().ok());

            match key {
                Some("MemTotal:") => total_kb = val,
                Some("MemAvailable:") => avail_kb = val,
                Some("MemFree:") => free_kb = val,
                _ => {}
            }
        }

        if let Some(total) = total_kb {
            total_mb = total / 1024;
            // Use MemAvailable if present, else fallback to MemFree
            if let Some(avail) = avail_kb {
                available_mb = avail / 1024;
            } else if let Some(free) = free_kb {
                available_mb = free / 1024;
            }
        }
    }

    let used_mb = total_mb.saturating_sub(available_mb);

    RamInfo {
        total_mb,
        used_mb,
        available_mb,
    }
}

fn get_storage_detail(path: &str) -> StorageDetail {
    unsafe {
        let mut stats: libc::statvfs = std::mem::zeroed();
        let c_path = std::ffi::CString::new(path).unwrap();
        if libc::statvfs(c_path.as_ptr(), &mut stats) == 0 {
            let total_bytes = stats.f_blocks as u64 * stats.f_frsize as u64;
            let available_bytes = stats.f_bavail as u64 * stats.f_frsize as u64;
            let used_bytes = total_bytes.saturating_sub(available_bytes);

            let bytes_to_gb = 1024.0 * 1024.0 * 1024.0;
            StorageDetail {
                total_gb: (total_bytes as f32) / bytes_to_gb,
                used_gb: (used_bytes as f32) / bytes_to_gb,
                available_gb: (available_bytes as f32) / bytes_to_gb,
            }
        } else {
            // Default nominal values
            StorageDetail {
                total_gb: 128.0,
                used_gb: 75.0,
                available_gb: 53.0,
            }
        }
    }
}

fn get_storage_info() -> StorageInfo {
    // Standard Linux first: check root "/" first, unless we are on Termux/Android.
    // We detect Termux by checking if "/data/data/com.termux/files/home" exists.
    let path = if fs::metadata("/data/data/com.termux/files/home").is_ok() {
        "/data/data/com.termux/files/home"
    } else {
        "/"
    };

    StorageInfo {
        internal: get_storage_detail(path),
    }
}

fn get_uptime() -> u64 {
    if let Ok(uptime_str) = fs::read_to_string("/proc/uptime") {
        if let Some(first_num) = uptime_str.split_whitespace().next() {
            if let Ok(val) = first_num.parse::<f64>() {
                return val as u64;
            }
        }
    }
    0
}

async fn handler() -> Json<SystemStats> {
    let battery = get_battery_info();
    let cpu = get_cpu_info().await;
    let ram = get_ram_info();
    let storage = get_storage_info();
    let uptime_seconds = get_uptime();

    Json(SystemStats {
        battery,
        cpu,
        ram,
        storage,
        uptime_seconds,
    })
}

async fn webui_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../index.html"))
}

fn get_config_path() -> PathBuf {
    let mut path = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    path.push(".retro-homepage");
    path.push("config.json");
    path
}

async fn get_config() -> Json<serde_json::Value> {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                return Json(serde_json::json!({
                    "configured": true,
                    "config": json
                }));
            }
        }
    }
    Json(serde_json::json!({
        "configured": false
    }))
}

async fn save_config(Json(payload): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    
    if let Ok(content) = serde_json::to_string_pretty(&payload) {
        if fs::write(&path, content).is_ok() {
            return Json(serde_json::json!({
                "success": true
            }));
        }
    }
    
    Json(serde_json::json!({
        "success": false,
        "error": "Failed to write config file"
    }))
}

#[tokio::main]
async fn main() {
    let mut port = 3000;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "--port" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u16>() {
                        Ok(p) => {
                            port = p;
                            i += 2;
                        }
                        Err(_) => {
                            eprintln!("Error: Invalid port number '{}'", args[i + 1]);
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Error: Missing port number after '{}'", args[i]);
                    std::process::exit(1);
                }
            }
            "-h" | "--help" => {
                println!("Retro Homepage Backend Server");
                println!();
                println!("Usage: {} [OPTIONS]", args[0]);
                println!();
                println!("Options:");
                println!("  -p, --port <PORT>  Set the port to listen on [default: 3000]");
                println!("  -h, --help         Print help information");
                std::process::exit(0);
            }
            unknown => {
                eprintln!("Error: Unknown option '{}'", unknown);
                eprintln!("Usage: {} [-p <PORT>] [-h]", args[0]);
                std::process::exit(1);
            }
        }
    }

    let app = Router::new()
        .route("/", get(webui_handler))
        .route("/index.html", get(webui_handler))
        .route("/api/stats", get(handler))
        .route("/api/config", get(get_config).post(save_config))
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", port);
    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            println!("Retro Homepage listening on port {}...", port);
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Server error: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: Failed to bind to {} - {}", addr, e);
            std::process::exit(1);
        }
    }
}
