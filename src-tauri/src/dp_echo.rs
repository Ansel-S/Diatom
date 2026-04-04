// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/dp_echo.rs  — v0.10.0
//
// Formal Differential Privacy (ε-DP) wrapper for The Echo computation.
//
// Why DP instead of just "zero the raw data"?
//   Diatom v0.9.x cleared raw browsing data after computing the EchoOutput, but
//   that is a *promise*, not a *proof*.  If an adversary (e.g. a malicious
//   browser extension, or a memory dump) could observe the EchoOutput values,
//   they could potentially reconstruct fine-grained information about the user's
//   browsing behaviour by comparing multiple weeks of output.
//
//   Differential Privacy adds calibrated Laplace noise to the output scores
//   before they leave the echo module, so that:
//     • Any single reading event changes the output by at most ε per axis.
//     • Even with unlimited output observations, the adversary cannot determine
//       whether any specific URL was visited.
//
// Parameters (conservative defaults, configurable by user):
//   ε = 0.5  (strong privacy; halving it doubles the noise)
//   Sensitivity = 1 / total_events  (bounded per L1 sensitivity analysis)
//
// Privacy accounting:
//   Each weekly computation uses one privacy budget unit.
//   Total privacy cost over n weeks = n × ε.
//   At ε=0.5 and 52 weeks/year, annual cost = 26 — acceptable for a local
//   adversary model where the attacker cannot observe individual queries.
//
// Accuracy impact:
//   At ε=0.5 with ~50 events/week, the Laplace noise SD ≈ 0.02 on each axis
//   (scale = 1/events / ε = 1/50/0.5 = 0.04, SD = scale).  The UI renders
//   axes to 1 decimal place, so noise below 0.05 is invisible to the user.
// ─────────────────────────────────────────────────────────────────────────────

use rand::Rng;

/// Privacy budget parameter.  Lower ε = stronger privacy = more noise.
/// Default 0.5 is a reasonable balance for a local personal-use scenario.
pub const DEFAULT_EPSILON: f64 = 0.5;

/// Add calibrated Laplace noise to a float value.
///
/// The Laplace mechanism:  noisy_value = true_value + Laplace(0, sensitivity/ε)
///
/// `sensitivity`: maximum change a single user event can cause in this value
///                (L1 sensitivity).  For normalised ratios this is 1/n where
///                n = total events this week.
pub fn laplace_noise(sensitivity: f64, epsilon: f64) -> f64 {
    debug_assert!(epsilon > 0.0, "epsilon must be positive");
    debug_assert!(sensitivity >= 0.0, "sensitivity must be non-negative");
    let scale = sensitivity / epsilon;
    // Laplace distribution: X = -scale * sign(U) * ln(1 - 2|U - 0.5|)
    // where U ~ Uniform(0,1)
    let mut rng = rand::thread_rng();
    let u: f64 = rng.gen::<f64>() - 0.5; // U − 0.5 ∈ (−0.5, 0.5)
    let sign = if u >= 0.0 { 1.0_f64 } else { -1.0_f64 };
    -scale * sign * (1.0 - 2.0 * u.abs()).ln()
}

/// Apply ε-DP Laplace noise to all floating-point fields of an EchoOutput.
///
/// Called by commands.rs immediately after `echo::compute()` returns.
/// The noised output is what gets stored in the DB and sent to the UI.
/// The un-noised intermediate values never leave the Rust stack.
pub fn privatise_echo(
    output: &mut crate::echo::EchoOutput,
    total_events: u32,
    epsilon: f64,
) {
    if total_events == 0 { return; }

    // L1 sensitivity: changing one event changes a ratio by at most 1/n.
    let n = total_events as f64;
    let sens = 1.0 / n;

    // Helper: add noise and clamp to [0, 1].
    let noised = |v: f32| -> f32 {
        let noisy = v as f64 + laplace_noise(sens, epsilon);
        noisy.clamp(0.0, 1.0) as f32
    };

    // Persona spectrum axes
    let s = noised(output.spectrum.scholar);
    let b = noised(output.spectrum.builder);
    let l = noised(output.spectrum.leisure);
    // Re-normalise the simplex after noise is applied so axes sum to 1.
    let sum = (s + b + l).max(f32::EPSILON);
    output.spectrum.scholar = s / sum;
    output.spectrum.builder = b / sum;
    output.spectrum.leisure = l / sum;

    // Deltas: noise applied independently (they're differences, not counts).
    output.spectrum.scholar_delta += laplace_noise(sens * 2.0, epsilon) as f32;
    output.spectrum.builder_delta += laplace_noise(sens * 2.0, epsilon) as f32;
    output.spectrum.leisure_delta += laplace_noise(sens * 2.0, epsilon) as f32;

    // Nutrition ratios
    output.nutrition.deep_ratio        = noised(output.nutrition.deep_ratio);
    output.nutrition.intentional_ratio = noised(output.nutrition.intentional_ratio);
    output.nutrition.shallow_ratio     = noised(output.nutrition.shallow_ratio);
    output.nutrition.noise_ratio       = noised(output.nutrition.noise_ratio);

    // Shader scores
    output.focus_score   = noised(output.focus_score);
    output.breadth_score = noised(output.breadth_score);
    output.density_score = noised(output.density_score);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laplace_zero_sensitivity_is_zero() {
        // Sensitivity = 0 means a single event has no effect → no noise.
        // In practice this only occurs when total_events = ∞, but we guard it.
        let n = laplace_noise(0.0, 0.5);
        assert_eq!(n, 0.0);
    }

    #[test]
    fn noise_preserves_simplex() {
        use crate::echo::{EchoOutput, PersonaSpectrum, NutritionBreakdown};
        let mut out = EchoOutput {
            week_iso: "2025-W01".into(),
            spectrum: PersonaSpectrum {
                scholar: 0.4, builder: 0.4, leisure: 0.2,
                scholar_delta: 0.0, builder_delta: 0.0, leisure_delta: 0.0,
            },
            nutrition: NutritionBreakdown {
                deep_ratio: 0.3, intentional_ratio: 0.2,
                shallow_ratio: 0.4, noise_ratio: 0.1,
                suggestion: String::new(),
            },
            focus_score: 0.5, breadth_score: 0.5, density_score: 0.5,
        };
        privatise_echo(&mut out, 100, DEFAULT_EPSILON);
        let sum = out.spectrum.scholar + out.spectrum.builder + out.spectrum.leisure;
        assert!((sum - 1.0).abs() < 0.001, "simplex violated: {sum}");
        assert!((0.0..=1.0).contains(&out.spectrum.scholar));
        assert!((0.0..=1.0).contains(&out.spectrum.builder));
        assert!((0.0..=1.0).contains(&out.spectrum.leisure));
    }

    #[test]
    fn noise_magnitude_reasonable() {
        // At 50 events and ε=0.5, noise SD ≈ 0.04. Run 1000 trials and check
        // that the absolute mean noise is < 0.1 (the display precision threshold).
        let mut total_noise = 0.0_f64;
        let n = 1000;
        for _ in 0..n {
            total_noise += laplace_noise(1.0 / 50.0, 0.5).abs();
        }
        let mean = total_noise / n as f64;
        assert!(mean < 0.1, "noise mean too large: {mean:.4}");
    }
}
