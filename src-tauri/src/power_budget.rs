// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/power_budget.rs  — v0.11.0
//
// Power-aware background task scheduling.
//
// Problem:
//   Diatom runs several periodic background loops (Sentinel, tab-budget, threat-
//   refresh, PIR cover traffic). On battery, these should run less aggressively
//   to avoid shortening the user's session. On AC power, full frequency is fine.
//
// Solution — three tiers:
//   AC power   → standard intervals (sentinel 60 min, tab-budget 60 s)
//   Battery    → conservative intervals (sentinel 3 h, tab-budget 5 min)
//   Low battery (≤20%) → minimal intervals (sentinel 6 h, tab-budget 15 min),
//                         PIR cover traffic completely suppressed
//
// Detection:
//   macOS: IOKit battery query via `pmset -g batt` subprocess
//   Windows: GetSystemPowerStatus() WIN32 API via `wmic path Win32_Battery`
//   Linux: /sys/class/power_supply/BAT0/status + capacity
//   Fallback: assume AC power (safe default — never degrade performance
//   conservatively when power state is unknown)
//
// Integration:
//   main.rs calls PowerBudget::current() at startup and every 5 minutes.
//   Background loops use PowerBudget::sentinel_interval_secs() etc.
//   IPC: cmd_power_budget_status exposes state to the UI (displayed in Labs).
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Battery power state detected at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerState {
    /// Plugged in or power state unknown (conservative assumption).
    Ac,
    /// On battery with > 20% charge.
    Battery,
    /// On battery with ≤ 20% charge.
    LowBattery,
}

/// Task scheduling intervals based on power state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerBudget {
    pub state: PowerState,
    pub battery_pct: Option<u8>,
    /// Sentinel UA-refresh loop interval.
    pub sentinel_interval_secs: u64,
    /// Tab-budget recalculation loop interval.
    pub tab_budget_interval_secs: u64,
    /// Threat-list refresh interval.
    pub threat_refresh_interval_secs: u64,
    /// Whether PIR cover traffic should fire (suppressed on low battery).
    pub pir_enabled: bool,
    /// Whether decoy requests should fire.
    pub decoy_enabled: bool,
}

impl PowerBudget {
    pub fn for_state(state: PowerState, pct: Option<u8>) -> Self {
        match state {
            PowerState::Ac => Self {
                state,
                battery_pct: pct,
                sentinel_interval_secs:       60 * 60,      // 1 h
                tab_budget_interval_secs:     60,            // 1 min
                threat_refresh_interval_secs: 7 * 24 * 3600,// 7 days
                pir_enabled:   true,
                decoy_enabled: true,
            },
            PowerState::Battery => Self {
                state,
                battery_pct: pct,
                sentinel_interval_secs:       3 * 60 * 60,  // 3 h
                tab_budget_interval_secs:     5 * 60,        // 5 min
                threat_refresh_interval_secs: 7 * 24 * 3600,// 7 days (unchanged)
                pir_enabled:   true,
                decoy_enabled: false, // decoy traffic disabled on battery
            },
            PowerState::LowBattery => Self {
                state,
                battery_pct: pct,
                sentinel_interval_secs:       6 * 60 * 60,  // 6 h
                tab_budget_interval_secs:     15 * 60,       // 15 min
                threat_refresh_interval_secs: 7 * 24 * 3600,
                pir_enabled:   false, // all cover traffic suppressed
                decoy_enabled: false,
            },
        }
    }

    /// Detect current power state from the host OS.
    /// Returns PowerBudget::for_state(Ac, None) if detection fails (safe default).
    pub fn current() -> Self {
        let (state, pct) = detect_power_state();
        Self::for_state(state, pct)
    }

    pub fn sentinel_sleep(&self) -> Duration {
        Duration::from_secs(self.sentinel_interval_secs)
    }

    pub fn tab_budget_sleep(&self) -> Duration {
        Duration::from_secs(self.tab_budget_interval_secs)
    }
}

/// Platform-specific battery detection.
fn detect_power_state() -> (PowerState, Option<u8>) {
    #[cfg(target_os = "linux")]
    {
        if let Some(result) = linux_battery() {
            return result;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(result) = macos_battery() {
            return result;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(result) = windows_battery() {
            return result;
        }
    }

    // Fallback: assume AC — never pessimistically throttle when state unknown.
    (PowerState::Ac, None)
}

#[cfg(target_os = "linux")]
fn linux_battery() -> Option<(PowerState, Option<u8>)> {
    // Try /sys/class/power_supply/BAT0 first, then BAT1
    for bat in &["BAT0", "BAT1", "BAT"] {
        let status_path = format!("/sys/class/power_supply/{bat}/status");
        let cap_path    = format!("/sys/class/power_supply/{bat}/capacity");
        let status = std::fs::read_to_string(&status_path).ok()?;
        let pct: u8 = std::fs::read_to_string(&cap_path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(100);
        let on_battery = status.trim() == "Discharging" || status.trim() == "Not charging";
        if on_battery {
            let state = if pct <= 20 { PowerState::LowBattery } else { PowerState::Battery };
            return Some((state, Some(pct)));
        }
        return Some((PowerState::Ac, Some(pct)));
    }
    None
}

#[cfg(target_os = "macos")]
fn macos_battery() -> Option<(PowerState, Option<u8>)> {
    // `pmset -g batt` output: "Now drawing from 'Battery Power'" = discharging
    let out = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let on_battery = text.contains("Battery Power");
    let pct: Option<u8> = text.lines()
        .find(|l| l.contains('%'))
        .and_then(|l| {
            let p = l.find('%')?;
            let start = l[..p].rfind('\t').or_else(|| l[..p].rfind(' ')).map(|i| i + 1).unwrap_or(0);
            l[start..p].trim().parse().ok()
        });
    if on_battery {
        let state = match pct {
            Some(p) if p <= 20 => PowerState::LowBattery,
            _ => PowerState::Battery,
        };
        Some((state, pct))
    } else {
        Some((PowerState::Ac, pct))
    }
}

#[cfg(target_os = "windows")]
fn windows_battery() -> Option<(PowerState, Option<u8>)> {
    // GetSystemPowerStatus via wmic (no unsafe code required)
    let out = std::process::Command::new("wmic")
        .args(["path", "Win32_Battery", "get", "BatteryStatus,EstimatedChargeRemaining", "/format:csv"])
        .output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // BatteryStatus: 1=Other, 2=Unknown, 3=Fully Charged, 4=Low, 5=Critical, 6=Charging, 7=Charging+High
    // Values 1,2,4,5 indicate discharging/battery
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 { continue; }
        let status: u8 = cols[1].trim().parse().unwrap_or(2);
        let pct:    u8 = cols[2].trim().parse().unwrap_or(100);
        let on_battery = matches!(status, 1 | 4 | 5);
        if on_battery {
            let state = if pct <= 20 { PowerState::LowBattery } else { PowerState::Battery };
            return Some((state, Some(pct)));
        }
        return Some((PowerState::Ac, Some(pct)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_intervals_are_shortest() {
        let ac  = PowerBudget::for_state(PowerState::Ac, Some(100));
        let bat = PowerBudget::for_state(PowerState::Battery, Some(50));
        let low = PowerBudget::for_state(PowerState::LowBattery, Some(10));
        assert!(ac.sentinel_interval_secs  < bat.sentinel_interval_secs);
        assert!(bat.sentinel_interval_secs < low.sentinel_interval_secs);
        assert!(ac.tab_budget_interval_secs < bat.tab_budget_interval_secs);
    }

    #[test]
    fn low_battery_disables_cover_traffic() {
        let low = PowerBudget::for_state(PowerState::LowBattery, Some(15));
        assert!(!low.pir_enabled);
        assert!(!low.decoy_enabled);
    }

    #[test]
    fn ac_enables_all_traffic() {
        let ac = PowerBudget::for_state(PowerState::Ac, None);
        assert!(ac.pir_enabled);
        assert!(ac.decoy_enabled);
    }
}
