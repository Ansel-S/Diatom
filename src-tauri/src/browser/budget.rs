
use serde::{Deserialize, Serialize};

/// Default maximum number of tabs.  13 is the 7th Fibonacci number and maps
/// naturally to one focus row on a standard 13" laptop screen.
pub const DEFAULT_TAB_LIMIT: u32 = 13;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabBudgetConfig {
    /// Hard limit chosen by the user.  Range: 1–50.  Default: 13.
    pub max_tabs: u32,
}

impl Default for TabBudgetConfig {
    fn default() -> Self {
        TabBudgetConfig { max_tabs: DEFAULT_TAB_LIMIT }
    }
}

impl TabBudgetConfig {
    /// Load from DB settings ("tab_limit").  Falls back to DEFAULT_TAB_LIMIT.
    pub fn load(db: &crate::storage::db::Db) -> Self {
        let max_tabs = db.get_setting("tab_limit")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_TAB_LIMIT)
            .clamp(1, 50);
        TabBudgetConfig { max_tabs }
    }

    /// Persist to DB.
    pub fn save(&self, db: &crate::storage::db::Db) -> anyhow::Result<()> {
        db.set_setting("tab_limit", &self.max_tabs.to_string())
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabBudget {
    /// User-configured tab limit.
    pub t_max: u32,
    /// True if we are at or within 1 of t_max.
    pub pressure_high: bool,
    /// Auto-sleep life-value timer (seconds).  Shortened under pressure.
    pub sleep_timer_s: u64,
}

impl TabBudget {
    pub fn is_at_limit(&self, current_count: u32) -> bool {
        current_count >= self.t_max
    }
}


/// Compute the current tab budget from user config + current open-tab count.
pub fn compute_budget(cfg: &TabBudgetConfig, current_tab_count: u32) -> TabBudget {
    let t_max = cfg.max_tabs.max(1);

    let fill_ratio = current_tab_count as f64 / t_max as f64;
    let sleep_timer_s = if fill_ratio >= 1.0 {
        5 * 60
    } else if fill_ratio >= 0.8 {
        ((10.0 * 60.0 * (1.0 - fill_ratio)) / 0.2) as u64 + 5 * 60
    } else {
        10 * 60
    };

    let pressure_high = current_tab_count + 1 >= t_max;

    TabBudget { t_max, pressure_high, sleep_timer_s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_13() {
        let cfg = TabBudgetConfig::default();
        assert_eq!(cfg.max_tabs, 13);
    }

    #[test]
    fn compute_budget_reflects_config() {
        let cfg = TabBudgetConfig { max_tabs: 7 };
        let b = compute_budget(&cfg, 4);
        assert_eq!(b.t_max, 7);
    }

    #[test]
    fn pressure_high_at_limit() {
        let cfg = TabBudgetConfig { max_tabs: 5 };
        assert!(compute_budget(&cfg, 5).pressure_high);
        assert!(!compute_budget(&cfg, 3).pressure_high);
    }

    #[test]
    fn sleep_timer_shortens_under_load() {
        let cfg = TabBudgetConfig { max_tabs: 10 };
        let low  = compute_budget(&cfg, 2);
        let high = compute_budget(&cfg, 9);
        assert!(high.sleep_timer_s < low.sleep_timer_s);
    }

    #[test]
    fn clamp_max_tabs_range() {
        let cfg = TabBudgetConfig { max_tabs: 0 };
        let b = compute_budget(&cfg, 0);
        assert!(b.t_max >= 1);
    }
}

