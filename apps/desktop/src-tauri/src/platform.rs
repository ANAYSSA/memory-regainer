use crate::Source;
use serde_json::Value;
use std::{collections::BTreeSet, fs::File, path::Path, process::Command};

#[cfg(target_os = "macos")]
fn diskutil(args: &[&str]) -> Result<Value, String> {
    use std::io::Write;
    use std::process::Stdio;
    let out = Command::new("/usr/sbin/diskutil")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("No converter input")?
        .write_all(&out.stdout)
        .map_err(|e| e.to_string())?;
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    serde_json::from_slice(&result.stdout).map_err(|e| e.to_string())
}

pub fn sources() -> Result<Vec<Source>, String> {
    #[cfg(target_os = "macos")]
    {
        let v = diskutil(&["list", "-plist", "physical"])?;
        let mut result = Vec::new();
        for d in v["AllDisksAndPartitions"].as_array().into_iter().flatten() {
            let Some(id) = d["DeviceIdentifier"].as_str() else {
                continue;
            };
            let path = format!("/dev/{id}");
            let info = diskutil(&["info", "-plist", &path]).unwrap_or(Value::Null);
            let name = info["MediaName"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or(id);
            let accessible = File::open(&path).is_ok();
            let details = format!(
                "{} · {} · {}",
                info["BusProtocol"].as_str().unwrap_or("Unknown connection"),
                if info["SolidState"].as_bool() == Some(true) {
                    "SSD / TRIM may prevent recovery"
                } else {
                    "Read-only access"
                },
                if accessible {
                    "Ready"
                } else {
                    "OS permission required; scan an image instead"
                }
            );
            result.push(Source {
                id: format!("device:{id}"),
                name: name.into(),
                path,
                size: d["Size"].as_u64().unwrap_or(0),
                kind: "device".into(),
                accessible,
                detail: details,
            });
        }
        Ok(result)
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "Get-Disk | Select-Object Number,FriendlyName,Size,BusType,IsOffline | ConvertTo-Json -Compress"]).output().map_err(|e|e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
        let value: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        let values = if value.is_array() {
            value.as_array().cloned().unwrap_or_default()
        } else {
            vec![value]
        };
        Ok(values.iter().filter_map(|d|{
            let n=d["Number"].as_u64()?; let path=format!(r"\\.\PhysicalDrive{n}");
            Some(Source{id:format!("device:{n}"),name:d["FriendlyName"].as_str().unwrap_or("Physical drive").into(),accessible:File::open(&path).is_ok(),path,size:d["Size"].as_u64().unwrap_or(0),kind:"device".into(),detail:"Read-only device. Administrator permission may be required. No automatic elevation.".into()})
        }).collect())
    }
    #[cfg(target_os = "linux")]
    {
        let out = Command::new("lsblk")
            .args([
                "--json",
                "--bytes",
                "--nodeps",
                "--output",
                "NAME,PATH,SIZE,MODEL,TYPE",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        Ok(value["blockdevices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["type"] == "disk")
            .filter_map(|d| {
                let path = d["path"].as_str()?.to_owned();
                Some(Source {
                    id: format!("device:{path}"),
                    name: d["model"].as_str().unwrap_or("Block device").trim().into(),
                    accessible: File::open(&path).is_ok(),
                    path,
                    size: d["size"].as_u64().unwrap_or(0),
                    kind: "device".into(),
                    detail: "Read-only block device. Normal OS permissions apply.".into(),
                })
            })
            .collect())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Ok(Vec::new())
    }
}

/// Fail closed whenever we cannot resolve the physical media on BOTH sides.
pub fn ensure_separate_device(source: &Source, destination: &Path) -> Result<(), String> {
    if source.kind == "image" {
        return Ok(());
    }
    let from = physical_media(Path::new(&source.path))?;
    let to = physical_media(destination)?;
    if from.is_empty() || to.is_empty() {
        return Err("Cannot establish destination physical device. Use a disk image on separate healthy storage.".into());
    }
    if !from.is_disjoint(&to) {
        return Err("Destination is on the scanned physical device. Choose a different physical drive to avoid overwriting recoverable data.".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn physical_media(path: &Path) -> Result<BTreeSet<String>, String> {
    let s = path.to_str().ok_or("Invalid path")?;
    let device = if s.starts_with("/dev/") {
        s.to_owned()
    } else {
        let out = Command::new("/bin/df")
            .args(["-P", s])
            .output()
            .map_err(|e| e.to_string())?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .last()
            .and_then(|l| l.split_whitespace().next())
            .filter(|d| d.starts_with("/dev/"))
            .ok_or("Cannot determine destination volume")?
            .to_owned()
    };
    let v = diskutil(&["info", "-plist", &device])?;
    fn whole(id: &str) -> Option<String> {
        let raw = id.trim_start_matches("/dev/");
        let prefix = raw.strip_prefix("disk")?;
        let n: String = prefix.chars().take_while(|c| c.is_ascii_digit()).collect();
        if n.is_empty() {
            None
        } else {
            Some(format!("disk{n}"))
        }
    }
    let mut ids = BTreeSet::new();
    if let Some(stores) = v["APFSPhysicalStores"].as_array() {
        for store in stores {
            if let Some(id) = store["APFSPhysicalStore"].as_str().and_then(whole) {
                ids.insert(id);
            }
        }
    } else if let Some(id) = v["ParentWholeDisk"]
        .as_str()
        .or(v["DeviceIdentifier"].as_str())
        .and_then(whole)
    {
        ids.insert(id);
    }
    Ok(ids)
}
#[cfg(target_os = "windows")]
fn physical_media(path: &Path) -> Result<BTreeSet<String>, String> {
    let s = path.to_str().ok_or("Invalid path")?;
    let disk_script = if let Some(n) = s.strip_prefix(r"\\.\PhysicalDrive") {
        if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
            return Err("Invalid physical drive".into());
        }
        format!("$disks = @(Get-Disk -Number {n} -ErrorAction Stop);")
    } else {
        let p = s.strip_prefix(r"\\?\").unwrap_or(s);
        if p.starts_with(r"\\") {
            return Err("Network destinations cannot be mapped to physical media".into());
        }
        let quoted = p.replace('\'', "''");
        format!("$v = Get-Volume -FilePath '{quoted}' -ErrorAction Stop; $part = @(Get-Partition -Volume $v -ErrorAction Stop); $disks = @($part | Get-Disk -ErrorAction Stop);")
    };
    let script = format!("$ErrorActionPreference='Stop'; {disk_script} if ($disks.Count -eq 0) {{ throw 'No physical mapping' }}; foreach ($disk in $disks) {{ if ($disk.BusType -notin @('USB','SATA','SAS','SCSI','NVMe','SD','MMC','ATA')) {{ throw 'Virtual, RAID or unknown media is not supported for safe destination mapping' }} }}; @($disks | Select-Object -ExpandProperty Number) | ConvertTo-Json -Compress");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Cannot establish distinct physical disks, including volume mount points. Use disk images on healthy storage.".into());
    }
    let v: Value =
        serde_json::from_slice(&out.stdout).map_err(|_| "Cannot map destination to a disk")?;
    let ids = if let Some(n) = v.as_u64() {
        vec![n]
    } else {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect()
    };
    Ok(ids.into_iter().map(|n| n.to_string()).collect())
}
#[cfg(target_os = "linux")]
fn physical_media(path: &Path) -> Result<BTreeSet<String>, String> {
    let s = path.to_str().ok_or("Invalid path")?;
    let device = if s.starts_with("/dev/") {
        s.to_owned()
    } else {
        let out = Command::new("findmnt")
            .args(["--json", "--target", s, "--output", "SOURCE"])
            .output()
            .map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        v["filesystems"][0]["source"]
            .as_str()
            .ok_or("Cannot resolve destination volume")?
            .split('[')
            .next()
            .unwrap_or("")
            .to_owned()
    };
    let out = Command::new("lsblk")
        .args(["--json", "--inverse", "--output", "NAME,TYPE", &device])
        .output()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    fn walk(v: &Value, ids: &mut BTreeSet<String>) {
        if v["type"] == "disk" {
            if let Some(n) = v["name"].as_str() {
                ids.insert(n.to_owned());
            }
        }
        for c in v["children"].as_array().into_iter().flatten() {
            walk(c, ids);
        }
    }
    let mut ids = BTreeSet::new();
    for b in v["blockdevices"].as_array().into_iter().flatten() {
        walk(b, &mut ids)
    }
    Ok(ids)
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn physical_media(_: &Path) -> Result<BTreeSet<String>, String> {
    Err("Device mapping unavailable".into())
}

pub fn check_free_space(destination: &Path, required: u64) -> Result<(), String> {
    #[cfg(unix)]
    let available = {
        let out = Command::new("df")
            .arg("-Pk")
            .arg(destination)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("Cannot check destination free space".into());
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .last()
            .and_then(|line| line.split_whitespace().nth(3))
            .and_then(|n| n.parse::<u64>().ok())
            .and_then(|n| n.checked_mul(1024))
            .ok_or("Cannot read available destination space")?
    };
    #[cfg(windows)]
    let available = {
        let path = destination
            .to_str()
            .ok_or("Invalid destination")?
            .replace('\'', "''");
        let script = format!("$ErrorActionPreference='Stop'; (Get-Volume -FilePath '{path}' -ErrorAction Stop).SizeRemaining");
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|e| e.to_string())?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|_| "Cannot check destination free space")?
    };
    #[cfg(not(any(unix, windows)))]
    let available = 0;
    // Leave space for the report and filesystem metadata; write failures are still checked.
    if required.saturating_add(1024 * 1024) > available {
        return Err(format!(
            "Insufficient destination space: {required} bytes required, {available} available"
        ));
    }
    Ok(())
}
