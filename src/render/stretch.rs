pub fn apply_gamma_stretch(value: f64, gamma: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    value.powf(gamma)
}

/// Aplica stretch logarítmico.
pub fn apply_log_stretch(value: f64, base: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    (value * base + 1.0).log(base + 1.0)
}

/// Aplica stretch asinh (arco-seno-hiperbólico).
pub fn apply_asinh_stretch(value: f64, scale: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    (value * scale).asinh() / scale.asinh()
}

/// Normaliza valores de um canvas para o intervalo [0, 1].
pub fn normalize_canvas(canvas: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;

    for row in canvas.iter() {
        for &val in row.iter() {
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
        }
    }

    if (max_val - min_val).abs() < f64::EPSILON {
        return canvas
            .iter()
            .map(|row| row.iter().map(|_| 0.0).collect())
            .collect();
    }

    let range = max_val - min_val;
    canvas
        .iter()
        .map(|row| row.iter().map(|&v| (v - min_val) / range).collect())
        .collect()
}

/// Normaliza e aplica stretch ao canvas.
pub fn normalize_with_stretch(canvas: &[Vec<f64>], stretch: StretchType) -> Vec<Vec<f64>> {
    let normalized = normalize_canvas(canvas);
    apply_stretch(&normalized, stretch)
}

/// Tipos de stretch disponíveis.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum StretchType {
    /// Sem stretch (linear)
    None,
    /// Gamma stretch
    Gamma(f64),
    /// Log stretch
    Log(f64),
    /// Asinh stretch
    Asinh(f64),
}

impl Default for StretchType {
    fn default() -> Self {
        StretchType::Gamma(0.6)
    }
}

/// Aplica stretch a um canvas normalizado.
pub fn apply_stretch(canvas: &[Vec<f64>], stretch: StretchType) -> Vec<Vec<f64>> {
    match stretch {
        StretchType::None => canvas.to_vec(),
        StretchType::Gamma(gamma) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_gamma_stretch(v, gamma)).collect())
            .collect(),
        StretchType::Log(base) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_log_stretch(v, base)).collect())
            .collect(),
        StretchType::Asinh(scale) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_asinh_stretch(v, scale)).collect())
            .collect(),
    }
}
