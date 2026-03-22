// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/war_report.rs  — v7
//
// Diatom War Report: anti-tracking metrics + narrative prose.
//
// Counters are written to privacy_stats by db.rs helpers called from:
//   • blocker.rs:    increment_block_count (every blocked request)
//   • privacy.rs:    increment_noise_count (every fingerprint noise injection)
//   • tabs.rs:       add_ram_saved (on deep-sleep compression)
//   • blocker.rs:    time_saved via heuristic (blocked request count × avg load time)
//
// The narrative layer is a pure Rust template engine.
// No LLM required. No network call.
// ─────────────────────────────────────────────────────────────────────────────

use crate::db::WarReportRow;
use serde::{Deserialize, Serialize};

/// Average time (seconds) a user would spend on a page with trackers before
/// they loaded / caused distractions. Conservative heuristic.
const AVG_TRACKER_TIME_SAVED_S: f64 = 0.9;

/// Average RAM per suppressed tracker payload (KB).
const AVG_TRACKER_RAM_KB: f64 = 12.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarReport {
    pub tracking_blocks: i64,
    pub noise_injections: i64,
    pub ram_saved_mb: f64,
    pub time_saved_min: f64,
    // Narrative strings
    pub block_narrative: String,
    pub noise_narrative: String,
    pub ram_narrative: String,
    pub time_narrative: String,
    pub summary_headline: String,
}

impl WarReport {
    pub fn from_row(row: &WarReportRow) -> Self {
        // Compute derived metrics
        let time_from_blocks = (row.tracking_block_count as f64 * AVG_TRACKER_TIME_SAVED_S) / 60.0;
        let time_saved_min = row.time_saved_min + time_from_blocks;
        let ram_from_blocks = row.tracking_block_count as f64 * AVG_TRACKER_RAM_KB / 1024.0;
        let ram_saved_mb = row.ram_saved_mb + ram_from_blocks;

        WarReport {
            tracking_blocks: row.tracking_block_count,
            noise_injections: row.fingerprint_noise_count,
            ram_saved_mb,
            time_saved_min,
            block_narrative: block_narrative(row.tracking_block_count),
            noise_narrative: noise_narrative(row.fingerprint_noise_count),
            ram_narrative: ram_narrative(ram_saved_mb),
            time_narrative: time_narrative(time_saved_min),
            summary_headline: summary_headline(
                row.tracking_block_count,
                row.fingerprint_noise_count,
            ),
        }
    }
}

fn block_narrative(n: i64) -> String {
    match n {
        0 => "The trackers are eerily quiet this week. Maybe they've given up?".to_owned(),
        1..=99 => format!("Diatom intercepted {n} monitoring probes this week, neutralizing them before they could reach the renderer."),
        100..=999 => {
            format!("本周共有 {n} 次追踪请求被截断于协议层。每一次都是你未曾经历过的数据收割。")
        }
        1000..=9999 => {
            format!("{n} 次。那是 {n} 次有人试图为你建立档案。Diatom 让每一次尝试都徒劳无功。")
        }
        _ => format!("{n} 次监控向量——全数抹除。数据经济本周在你的设备边界处遭遇了一堵墙。"),
    }
}

fn noise_narrative(n: i64) -> String {
    match n {
        0 => "本周指纹噪音注入暂停。如需启用，请检查隐私设置。".to_owned(),
        1..=999 => format!("你的设备本周向外界广播了 {n} 个合成身份。真实的你，从未出现。"),
        1000..=99_999 => format!(
            "{n} 次随机噪声注入。每次都意味着一份错误的 Canvas 指纹、一个虚假的 WebGL 签名。追踪者收获的，是一座幻影城市。"
        ),
        _ => format!("{n} 次噪声注入——你的数字指纹从未在两次请求间保持一致。这是隐私的最后防线。"),
    }
}

fn ram_narrative(mb: f64) -> String {
    if mb < 1.0 {
        return "本周 Deep Sleep 策略尚未产生显著内存节省。".to_owned();
    }
    if mb < 100.0 {
        return format!(
            "Deep Sleep 为你的设备回收了约 {mb:.0} MB RAM。\
            这些内存原本会因标签僵尸而永久燃烧。"
        );
    }
    if mb < 500.0 {
        return format!(
            "约 {mb:.0} MB 内存在本周得以复活。\
            {browser_name} 会让这些内存静静发热直至你关闭浏览器。Diatom 不会。",
            browser_name = "某些浏览器"
        );
    }
    format!(
        "{mb:.0} MB RAM——本周从标签地狱中被解救。\
        你的风扇没有因此多转一圈，你的电池也因此多撑了一会儿。"
    )
}

fn time_narrative(min: f64) -> String {
    if min < 1.0 {
        return "本周过滤节省的时间微乎其微——或许你的浏览本就相当克制。".to_owned();
    }
    if min < 10.0 {
        return format!("拦截追踪器和内容农场为你省下了约 {min:.0} 分钟加载与噪音时间。");
    }
    if min < 60.0 {
        return format!(
            "本周约 {min:.0} 分钟未被低质量内容消耗。\
            这些时间是你的。去做任何值得做的事。"
        );
    }
    let hrs = min / 60.0;
    format!(
        "{hrs:.1} 小时。那是你本周因 Diatom 过滤而未曾浪费的时间。\
        不是因为算法给了你更好的内容——而是因为垃圾从未进入。"
    )
}

fn summary_headline(blocks: i64, noise: i64) -> String {
    let total = blocks + noise;
    if total == 0 {
        return "本周清净。".to_owned();
    }
    if total < 500 {
        return format!("本周共发生 {total} 次对抗性事件。数字边界基本稳固。");
    }
    if total < 5000 {
        return format!("{total} 次拦截 · 噪声注入。数据经济未能触达你。");
    }
    format!("{total} 次。他们没有停止尝试。你也没有。")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_renders_without_panic() {
        let row = WarReportRow {
            tracking_block_count: 4200,
            fingerprint_noise_count: 150_000,
            ram_saved_mb: 1200.0,
            time_saved_min: 18.0,
        };
        let r = WarReport::from_row(&row);
        assert!(!r.block_narrative.is_empty());
        assert!(!r.summary_headline.is_empty());
        assert!(r.ram_saved_mb > 1200.0); // derived adds block-based estimate
    }

    #[test]
    fn zero_report_graceful() {
        let row = WarReportRow {
            tracking_block_count: 0,
            fingerprint_noise_count: 0,
            ram_saved_mb: 0.0,
            time_saved_min: 0.0,
        };
        let r = WarReport::from_row(&row);
        assert!(!r.block_narrative.is_empty());
    }
}
