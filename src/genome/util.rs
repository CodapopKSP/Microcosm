//! Small genome-agnostic helpers (shared by sim systems).

/// Whether an agent attempts movement this tick (`roll` in `[0, 1)`).
pub fn wants_to_move_this_tick(desire_to_move: f32, roll: f32) -> bool {
    if desire_to_move <= 0.0 {
        false
    } else if desire_to_move >= 1.0 {
        true
    } else {
        roll < desire_to_move
    }
}

/// Mitigated damage: `raw * (1 - mitigation)`.
pub fn mitigated_damage(raw: f32, mitigation: f32) -> f32 {
    raw.max(0.0) * (1.0 - mitigation.clamp(0.0, 1.0))
}
