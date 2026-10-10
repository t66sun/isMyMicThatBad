use crate::dsp::{Filter, Smooth};
use std::collections::BTreeMap;

/// Copy this processor, implement your algorithm, then register its metadata and constructor.
pub struct TemplateFilter {
    amount: Smooth,
}
impl TemplateFilter {
    pub fn new(amount_db: f32) -> Self {
        Self {
            amount: Smooth::new(10.0_f32.powf(amount_db / 20.0)),
        }
    }
}
impl Default for TemplateFilter {
    fn default() -> Self {
        Self::new(0.0)
    }
}
impl Filter for TemplateFilter {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (x, y) in input.iter().zip(output) {
            *y = *x * self.amount.next();
        }
    }
    fn set_params(&mut self, params: &BTreeMap<String, f32>) {
        self.amount.set(10.0_f32.powf(params["amount_db"] / 20.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn template_parameter_changes_are_smoothed_and_audible() {
        let mut filter = TemplateFilter::default();
        let input = [0.1; 256];
        let mut output = [0.0; 256];
        filter.process(&input, &mut output);
        assert_eq!(input, output);
        filter.set_params(&[("amount_db".into(), -6.0206)].into_iter().collect());
        filter.process(&input, &mut output);
        assert!(output[0] > 0.099 && output[255] < 0.08);
        filter.process(&input, &mut output);
        assert!((output[255] - 0.05).abs() < 0.0001);
    }
}
