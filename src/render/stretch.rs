/// Tipos de stretch disponíveis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StretchType {
    /// Sem stretch (linear)
    None,
    /// Gamma stretch
    Gamma(f64),
}

/// Aplica stretch (contraste) no valor usando gamma.
/// Gamma < 1 aumenta contraste em valores baixos.
pub fn apply_gamma_stretch(value: f64, gamma: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    value.powf(gamma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gamma_stretch() {
        assert!(apply_gamma_stretch(0.5, 0.6) > 0.5);
        assert_eq!(apply_gamma_stretch(0.0, 0.6), 0.0);
        assert_eq!(apply_gamma_stretch(1.0, 0.6), 1.0);
    }
}
