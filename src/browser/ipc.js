
const invoke = window.__TAURI__?.invoke ?? (() => Promise.reject(new Error('Tauri not available')));

export const preprocessUrl   = (url)              => invoke('cmd_preprocess_url', { url });
export const fetch            = (url, method)      => invoke('cmd_fetch', { url, method });

export const tabCreate        = (url)              => invoke('cmd_tab_create', { url });
export const tabClose         = (tabId)            => invoke('cmd_tab_close', { tabId });
export const tabActivate      = (tabId)            => invoke('cmd_tab_activate', { tabId });
export const tabUpdate        = (tabId, url, title, dwellMs) =>
    invoke('cmd_tab_update', { tabId, url, title, dwellMs });
export const tabSleep         = (tabId, deep, snapshot) =>
    invoke('cmd_tab_sleep', { tabId, deep, snapshot });
export const tabWake          = (tabId)            => invoke('cmd_tab_wake', { tabId });
export const tabsState        = ()                 => invoke('cmd_tabs_state');

export const tabBudget        = (screenWidth)      => invoke('cmd_tab_budget', { screenWidth });
export const tabBudgetConfigSet = (cfg)            => invoke('cmd_tab_budget_config_set', cfg);

export const workspacesList   = ()                 => invoke('cmd_workspaces_list');
export const workspaceCreate  = (name, color, isPrivate) =>
    invoke('cmd_workspace_create', { name, color, isPrivate });
export const workspaceSwitch  = (workspaceId)      => invoke('cmd_workspace_switch', { workspaceId });
export const workspaceFire    = (workspaceId)      => invoke('cmd_workspace_fire', { workspaceId });

export const historySearch    = (query, limit)     => invoke('cmd_history_search', { query, limit });
export const historyClear     = ()                 => invoke('cmd_history_clear');
export const bookmarkAdd      = (url, title, tags, ephemeral) =>
    invoke('cmd_bookmark_add', { url, title, tags, ephemeral });
export const bookmarkList     = ()                 => invoke('cmd_bookmark_list');
export const bookmarkRemove   = (id)               => invoke('cmd_bookmark_remove', { id });

export const settingGet       = (key)              => invoke('cmd_setting_get', { key });
export const settingSet       = (key, value)       => invoke('cmd_setting_set', { key, value });

export const isBlocked        = (url)              => invoke('cmd_is_blocked', { url });
export const cleanUrl         = (url)              => invoke('cmd_clean_url', { url });
export const noiseSeed        = ()                 => invoke('cmd_noise_seed');

export const recordReading    = (evt)              => invoke('cmd_record_reading', { evt });
export const echoCompute      = ()                 => invoke('cmd_echo_compute');

export const warReport        = ()                 => invoke('cmd_war_report');

export const freezePage       = (payload)          => invoke('cmd_freeze_page', { payload });
export const museumList       = (limit)            => invoke('cmd_museum_list', { limit });
export const museumSearch     = (query)            => invoke('cmd_museum_search', { query });
export const museumDelete     = (id)               => invoke('cmd_museum_delete', { id });
export const museumThaw       = (id)               => invoke('cmd_museum_thaw', { id });

export const domCrush         = (domain, selector) => invoke('cmd_dom_crush', { domain, selector });
export const domBlocksFor     = (domain)           => invoke('cmd_dom_blocks_for', { domain });
export const domBlockRemove   = (id)               => invoke('cmd_dom_block_remove', { id });

export const zenActivate      = ()                 => invoke('cmd_zen_activate');
export const zenDeactivate    = ()                 => invoke('cmd_zen_deactivate');
export const zenState         = ()                 => invoke('cmd_zen_state');
export const zenSetAphorism   = (aphorism)         => invoke('cmd_zen_set_aphorism', { aphorism });

export const threatCheck      = (domain)           => invoke('cmd_threat_check', { domain });
export const threatRefresh    = ()                 => invoke('cmd_threat_list_refresh');

export const compatHandoff    = (url)              => invoke('cmd_compat_handoff', { url });
export const compatPageReport = (report)           => invoke('cmd_compat_page_report', { report });
export const compatIsLegacy   = (domain)           => invoke('cmd_compat_is_legacy', { domain });
export const compatIsPayment  = (domain)           => invoke('cmd_compat_is_payment', { domain });

export const labsList         = ()                 => invoke('cmd_labs_list');
export const labSet           = (id, enabled)      => invoke('cmd_lab_set', { id, enabled });
export const labIsEnabled     = (id)               => invoke('cmd_lab_is_enabled', { id });

export const slmStatus        = ()                 => invoke('cmd_slm_status');
export const slmChat          = (messages, model, maxTokens, temperature) =>
    invoke('cmd_slm_chat', { payload: { messages, model, maxTokens, temperature } });
export const slmModels        = ()                 => invoke('cmd_slm_models');
export const slmSetModel      = (modelId)          => invoke('cmd_slm_set_model', { modelId });
export const slmServerToggle  = (enable)           => invoke('cmd_slm_server_toggle', { enable });

export const totpList         = ()                 => invoke('cmd_totp_list');
export const totpAdd          = (issuer, account, secret, domains) =>
    invoke('cmd_totp_add', { issuer, account, secret, domains });
export const totpGenerate     = (entryId)          => invoke('cmd_totp_generate', { entryId });
export const totpMatch        = (domain)           => invoke('cmd_totp_match', { domain });
export const totpRemove       = (entryId)          => invoke('cmd_totp_remove', { entryId });

export const trustGet         = (domain)           => invoke('cmd_trust_get', { domain });
export const trustSet         = (domain, level)    => invoke('cmd_trust_set', { domain, level });
export const trustList        = ()                 => invoke('cmd_trust_list');

export const rssFeeds         = ()                 => invoke('cmd_rss_feeds');
export const rssAdd           = (url, category)    => invoke('cmd_rss_add', { url, category });
export const rssFetch         = (feedId)           => invoke('cmd_rss_fetch', { feedId });
export const rssItems         = (feedId, unreadOnly, limit) =>
    invoke('cmd_rss_items', { feedId, unreadOnly, limit });
export const rssMarkRead      = (itemId)           => invoke('cmd_rss_mark_read', { itemId });

export const systemInfo       = ()                 => invoke('cmd_system_info');
export const devtoolsOpen     = ()                 => invoke('cmd_devtools_open');

const _batchQueue = new Map(); // commandName → { args, resolve, reject }
let _batchRaf = null;

function batchInvoke(command, args) {
  return new Promise((resolve, reject) => {
    _batchQueue.set(command, { args, resolve, reject });
    if (!_batchRaf) {
      _batchRaf = requestAnimationFrame(async () => {
        _batchRaf = null;
        const pending = [..._batchQueue.entries()];
        _batchQueue.clear();
        for (const [cmd, { args: a, resolve: res, reject: rej }] of pending) {
          try {
    res(await invoke(cmd, a));
  } catch (e) {
    rej(e);
  }
        }
      });
    }
  });
}

export { batchInvoke };

