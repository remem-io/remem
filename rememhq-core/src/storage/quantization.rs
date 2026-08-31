//! Scalar Quantization (FP32 -> INT8) for compact vector storage and fast distance computation.

use serde::{Deserialize, Serialize};

/// Quantized vector representation using 8-bit signed integers and scale/offset parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedVector {
    pub data: Vec<i8>,
    pub min_val: f32,
    pub scale: f32,
    pub dimension: usize,
}

impl QuantizedVector {
    /// Quantize an FP32 vector into INT8.
    pub fn quantize(vector: &[f32]) -> Self {
        if vector.is_empty() {
            return Self {
                data: Vec::new(),
                min_val: 0.0,
                scale: 1.0,
                dimension: 0,
            };
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;

        for &val in vector {
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
        }

        let range = max_val - min_val;
        let scale = if range > 1e-6 { range / 255.0 } else { 1.0 };

        let data = vector
            .iter()
            .map(|&v| {
                let norm = (v - min_val) / scale;
                (norm.round().clamp(0.0, 255.0) as i16 - 128) as i8
            })
            .collect();

        Self {
            data,
            min_val,
            scale,
            dimension: vector.len(),
        }
    }

    /// Dequantize back to an FP32 vector approximation.
    pub fn dequantize(&self) -> Vec<f32> {
        self.data
            .iter()
            .map(|&i| {
                let norm = (i as i16 + 128) as f32;
                norm * self.scale + self.min_val
            })
            .collect()
    }

    /// Approximate cosine similarity directly between two quantized vectors.
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.dimension != other.dimension || self.dimension == 0 {
            return 0.0;
        }

        let v1 = self.dequantize();
        let v2 = other.dequantize();

        let mut dot = 0.0f32;
        let mut norm1 = 0.0f32;
        let mut norm2 = 0.0f32;

        for (a, b) in v1.iter().zip(v2.iter()) {
            dot += a * b;
            norm1 += a * a;
            norm2 += b * b;
        }

        if norm1 > 0.0 && norm2 > 0.0 {
            dot / (norm1.sqrt() * norm2.sqrt())
        } else {
            0.0
        }
    }

    /// Calculate storage compression ratio compared to raw FP32.
    pub fn compression_ratio(&self) -> f32 {
        // Raw FP32: dimension * 4 bytes
        // Quantized INT8: dimension * 1 byte + 8 bytes header (min_val + scale)
        let raw_bytes = self.dimension * 4;
        let quantized_bytes = self.dimension + 8;
        if quantized_bytes > 0 {
            raw_bytes as f32 / quantized_bytes as f32
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_roundtrip() {
        let original = vec![0.12, -0.45, 0.88, 0.03, -0.91, 0.55];
        let quantized = QuantizedVector::quantize(&original);
        assert_eq!(quantized.dimension, original.len());

        let reconstructed = quantized.dequantize();
        assert_eq!(reconstructed.len(), original.len());

        for (orig, recon) in original.iter().zip(reconstructed.iter()) {
            assert!((orig - recon).abs() < 0.02, "Error too large: orig={}, recon={}", orig, recon);
        }

        let sim = quantized.cosine_similarity(&quantized);
        assert!((sim - 1.0).abs() < 0.01);
    }
}
