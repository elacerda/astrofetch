/// Dense 2D scalar field stored in row-major order.
///
/// This type is used as the intermediate numerical representation for
/// procedural galaxy models before converting the field into terminal glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct DensityMap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f64>,
}

impl DensityMap {
    /// Creates a zero-filled density map.
    pub fn new(width: usize, height: usize) -> Self {
        assert!(width > 0, "density map width must be positive");
        assert!(height > 0, "density map height must be positive");

        Self {
            width,
            height,
            data: vec![0.0; width * height],
        }
    }

    /// Builds a map by evaluating a function at each integer cell.
    pub fn from_fn<F>(width: usize, height: usize, mut f: F) -> Self
    where
        F: FnMut(usize, usize) -> f64,
    {
        let mut map = Self::new(width, height);

        for y in 0..height {
            for x in 0..width {
                map.set(x, y, f(x, y));
            }
        }

        map
    }

    /// Returns the row-major vector index for a cell.
    #[inline]
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Reads a cell value.
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f64 {
        self.data[self.index(x, y)]
    }

    /// Writes a cell value.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, value: f64) {
        let idx = self.index(x, y);
        self.data[idx] = value;
    }

    /// Normalizes the map linearly to [0, 1].
    pub fn normalize(&self) -> Self {
        let min = self.data.iter().copied().fold(f64::INFINITY, f64::min);
        let max = self.data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = max - min;

        if range.abs() < f64::EPSILON {
            return Self::new(self.width, self.height);
        }

        Self {
            width: self.width,
            height: self.height,
            data: self.data.iter().map(|v| (v - min) / range).collect(),
        }
    }

    /// Applies a gamma stretch to normalized values.
    #[allow(dead_code)]
    pub fn gamma_stretch(&self, gamma: f64) -> Self {
        Self {
            width: self.width,
            height: self.height,
            data: self
                .data
                .iter()
                .map(|v| v.clamp(0.0, 1.0).powf(gamma))
                .collect(),
        }
    }

    /// Downsamples this map by averaging rectangular bins.
    pub fn downsample_average(&self, out_width: usize, out_height: usize) -> Self {
        let mut out = Self::new(out_width, out_height);

        for oy in 0..out_height {
            let y0 = oy * self.height / out_height;
            let y1 = ((oy + 1) * self.height / out_height).max(y0 + 1);

            for ox in 0..out_width {
                let x0 = ox * self.width / out_width;
                let x1 = ((ox + 1) * self.width / out_width).max(x0 + 1);

                let mut sum = 0.0;
                let mut count = 0usize;

                for y in y0..y1.min(self.height) {
                    for x in x0..x1.min(self.width) {
                        sum += self.get(x, y);
                        count += 1;
                    }
                }

                out.set(ox, oy, sum / count as f64);
            }
        }

        out
    }

    /// Converts the map into the legacy Vec<Vec<f64>> representation.
    pub fn into_rows(self) -> Vec<Vec<f64>> {
        self.data
            .chunks(self.width)
            .map(|row| row.to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_density_map_roundtrip() {
        let mut map = DensityMap::new(3, 2);
        map.set(2, 1, 0.75);

        assert_eq!(map.get(2, 1), 0.75);
        assert_eq!(map.into_rows()[1][2], 0.75);
    }

    #[test]
    fn test_downsample_average() {
        let map = DensityMap {
            width: 4,
            height: 2,
            data: vec![1.0, 1.0, 3.0, 3.0, 1.0, 1.0, 3.0, 3.0],
        };

        let out = map.downsample_average(2, 1);

        assert_eq!(out.width, 2);
        assert_eq!(out.height, 1);
        assert_eq!(out.data, vec![1.0, 3.0]);
    }
}
