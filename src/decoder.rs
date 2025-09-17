use zenoh::sample::Sample;

/// A decoder function to convert the sample into a human-readable string
///
/// # Arguments
/// * `sample` - The sample to be decoded
///
/// # Returns
/// A human-readable string representation of the sample
#[allow(dead_code)]
pub fn decoder(sample: Sample) -> String {
    String::from(sample.key_expr().as_str())
}
