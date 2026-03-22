// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/state.rs  — v0.9.0
// ─────────────────────────────────────────────────────────────────────────────

use crate::{
    db::Db, privacy::PrivacyConfig, rss::RssStore, tab_budget::TabBudgetConfig, tabs::TabStore,
    totp::TotpStore, trust::TrustStore, zen::ZenConfig,
};
use anyhow::Result;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, atomic::AtomicBool},
};

pub struct AppState {
    pub db: Db,
    pub ws_id: Mutex<String>,
    pub tabs: Mutex<TabStore>,
    pub privacy: RwLock<PrivacyConfig>,
    pub trust: Mutex<TrustStore>,
    pub totp: Mutex<TotpStore>,
    pub rss: Mutex<RssStore>,
    pub data_dir: PathBuf,

    // v7
    pub noise_seed: Mutex<u64>,
    pub zen: Mutex<ZenConfig>,
    pub threat_list: RwLock<HashSet<String>>,
    pub master_key: Mutex<[u8; 32]>,
    pub quad9_enabled: AtomicBool,
    pub age_heuristic_enabled: AtomicBool,

    // v7.2
    pub compat: Mutex<crate::compat::CompatStore>,
    pub storage_budget: Mutex<crate::storage_guard::StorageBudget>,

    // v0.9.0
    pub tab_budget_cfg: Mutex<TabBudgetConfig>,
    pub screen_width_px: Mutex<u32>,
    /// SLM server shutdown handle (None if server not running).
    pub slm_shutdown: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let db = Db::open(&data_dir.join("diatom.db"))?;
        std::fs::create_dir_all(data_dir.join("bundles"))?;

        let master_key = crate::freeze::get_or_init_master_key(&db)?;
        let quad9 = db
            .get_setting("quad9_enabled")
            .map(|v| v == "true")
            .unwrap_or(true);
        let age_h = db
            .get_setting("age_heuristic_enabled")
            .map(|v| v == "true")
            .unwrap_or(true);
        let threat_list: HashSet<String> = db
            .get_setting("threat_list_json")
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let ws_id = db
            .get_setting("active_workspace_id")
            .unwrap_or_else(|| "default".to_owned());
        let noise_seed: u64 = rand::random();

        let mut compat_store = crate::compat::CompatStore::default();
        if let Some(json) = db.get_setting("compat_legacy_domains") {
            if let Ok(domains) = serde_json::from_str::<Vec<String>>(&json) {
                for d in domains {
                    compat_store.add_legacy(&d);
                }
            }
        }

        let storage_budget: crate::storage_guard::StorageBudget = db
            .get_setting("storage_budget")
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        let tab_budget_cfg: TabBudgetConfig = db
            .get_setting("tab_budget_config")
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        Ok(AppState {
            db,
            ws_id: Mutex::new(ws_id),
            tabs: Mutex::new(TabStore::default()),
            privacy: RwLock::new(PrivacyConfig::default()),
            trust: Mutex::new(TrustStore::default()),
            totp: Mutex::new(TotpStore::default()),
            rss: Mutex::new(RssStore::default()),
            data_dir,
            noise_seed: Mutex::new(noise_seed),
            zen: Mutex::new(ZenConfig::default()),
            threat_list: RwLock::new(threat_list),
            master_key: Mutex::new(master_key),
            quad9_enabled: AtomicBool::new(quad9),
            age_heuristic_enabled: AtomicBool::new(age_h),
            compat: Mutex::new(compat_store),
            storage_budget: Mutex::new(storage_budget),
            tab_budget_cfg: Mutex::new(tab_budget_cfg),
            screen_width_px: Mutex::new(1440), // default desktop
            slm_shutdown: Mutex::new(None),
        })
    }

    pub fn workspace_id(&self) -> String {
        self.ws_id.lock().unwrap().clone()
    }

    pub fn switch_workspace(&self, ws_id: &str) -> anyhow::Result<()> {
        *self.ws_id.lock().unwrap() = ws_id.to_owned();
        self.db.set_setting("active_workspace_id", ws_id)?;
        use rand::Rng;
        *self.noise_seed.lock().unwrap() = rand::random();
        Ok(())
    }

    pub fn fire_workspace(&self, ws_id: &str) -> anyhow::Result<()> {
        self.tabs.lock().unwrap().close_workspace(ws_id);
        self.db.clear_history(ws_id)?;
        Ok(())
    }

    pub fn bundles_dir(&self) -> PathBuf {
        self.data_dir.join("bundles")
    }
    pub fn master_key(&self) -> [u8; 32] {
        *self.master_key.lock().unwrap()
    }
}
