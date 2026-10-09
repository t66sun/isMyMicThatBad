use crate::dsp::BLOCK;
use rubato::{
    Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
pub struct Converter {
    resampler: SincFixedOut<f32>,
}
impl Converter {
    pub fn new(input: u32, output: u32) -> Result<Self, String> {
        if input < 8000 || output < 8000 || input > 192000 || output > 192000 {
            return Err("仅支持 8–192 kHz 音频设备".into());
        }
        let parameters = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Cubic,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        Ok(Self {
            resampler: SincFixedOut::new(output as f64 / input as f64, 1.01, parameters, BLOCK, 1)
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn reset(&mut self) {
        self.resampler.reset();
    }
    pub fn input_frames(&self) -> usize {
        self.resampler.input_frames_next()
    }
    pub fn max_input_frames(&self) -> usize {
        self.resampler.input_frames_max()
    }
    pub fn set_relative(&mut self, relative: f64) -> Result<(), String> {
        self.resampler
            .set_resample_ratio_relative(relative, true)
            .map_err(|e| e.to_string())
    }
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), String> {
        if input.len() != self.input_frames() || output.len() != BLOCK {
            return Err("重采样块大小不匹配".into());
        }
        self.resampler
            .process_into_buffer(&[input], &mut [output], None)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub struct Drift {
    error: f64,
}
impl Default for Drift {
    fn default() -> Self {
        Self::new()
    }
}
impl Drift {
    pub fn new() -> Self {
        Self { error: 0.0 }
    }
    pub fn update(&mut self, filled: usize, target: usize) -> f64 {
        let deviation = (filled as f64 - target as f64) / target.max(1) as f64;
        self.error += 0.02 * (deviation - self.error);
        (1.0 - 0.002 * self.error).clamp(0.997, 1.003)
    }
}
