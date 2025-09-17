use log::{error, warn};
use msg_utils::get_decode_handler;
use zenoh::sample::Sample;

/// A decoder function to convert the sample into a human-readable string
///
/// # Arguments
/// * `sample` - The sample to be decoded
///
/// # Returns
/// A human-readable string representation of the sample
#[allow(dead_code)]
pub fn flatbuffer_decoder(sample: Sample) -> String {
    let payload_bytes = sample.payload().to_bytes().into_owned();
    let key_str = format!("{}", sample.key_expr());
    let s: String;

    if let Some(decode_fn) = get_decode_handler(&key_str) {
        match decode_fn(payload_bytes) {
            Ok(decoded_msg) => s = format!("{:?}", decoded_msg),
            Err(err) => {
                error!("Error decoding message on {}: {}", key_str, err);
                s = format!("Error decoding message on {}: {}", key_str, err);
            }
        }
    } else {
        warn!("No handler found for message on {}", key_str);
        s = format!("No handler found for message on {}", key_str);
    }
    s
}
