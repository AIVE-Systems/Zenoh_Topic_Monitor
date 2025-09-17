# Zenoh DDS Web Monitor 🌐

A lightweight, real-time web monitor for Zenoh, built with Rust. This tool subscribes to all topics (`**`) in a Zenoh network and displays them on a clean, responsive web interface, providing a convenient way to observe swarm robotics data streams with optional custom message decoding.

---

## ✨ Features

- **Real-time Monitoring**: Automatically updates the list of active topics as new data arrives.
- **Topic Details**: Displays the key expression, the size of the latest message in bytes, and its reception timestamp.
- **Custom Message Decoding**: Optional decoder system allows users to implement custom message interpretation for human-readable content display.
- **Lightweight**: Built with Rust and the Warp framework for excellent performance and low resource consumption.
- **Server-Sent Events (SSE)**: Uses a persistent connection for efficient, real-time data push from the server to the browser without polling.
- **Automatic Sorting**: New topics are inserted into the table in alphabetical order.
- **Responsive Design**: The web interface is designed to work well on both desktop and mobile devices.
- **Integrated Logging**: Utilises the `ftail` crate to provide both console and file logging.
- **HTML Safety**: Automatic HTML escaping of decoded content to prevent XSS attacks.

---

## 🛠️ Usage

### Run the Monitor

1.  Clone this repository:
    ```bash
    git clone git@github.com:tgodfrey0/Zenoh_Topic_Monitor.git
    cd Zenoh_Topic_Monitor
    ```
2.  Install the environment with Pixi:
    ```bash
    pixi install
    ```
3.  Configure the decoder (optional):
    ```bash
    # Edit main.rs to enable/disable custom decoder
    # Set custom_decoder = None for standard mode
    # Set custom_decoder = Some(decoder) to enable decoding
    ```
4.  Run the application with Cargo:
    ```bash
    pixi run server
    ```
5.  Open your web browser and navigate to `http://localhost:8080`.

You'll see a web page that automatically populates with topics as messages are published on the Zenoh network. If a custom decoder is enabled, decoded message content will appear in an additional column.

---

## 🔧 Custom Decoder Configuration

### Standard Mode (Default)

The monitor works when no decoder is specified:

```rust
// In main.rs
const DECODER: DecoderFn = None;
```

### With Custom Decoder

Implement your own decoder for domain-specific messages:

```rust
fn my_custom_decoder(sample: Sample) -> String {
    let payload = sample.payload().to_bytes();
    let topic = sample.key_expr().as_str();

    // Your custom decoding logic here
    // Return a human-readable string representation
    match topic {
        s if s.contains("/pose") => decode_pose_data(&payload),
        s if s.contains("/sensor") => decode_sensor_data(&payload),
        _ => format!("Data: {} bytes", payload.len()),
    }
}

// Then enable it in main.rs:
const DECODER: DecoderFn = Some(decoder::my_custom_decoder);
```

### Decoder Function Requirements

- **Input**: `sample: Sample` - Complete Zenoh sample with payload and metadata
- **Output**: `String` - Human-readable representation (automatically HTML-escaped)

---

## 📚 Technical Overview

The application is structured around several key components:

1.  **Zenoh Subscriber (`start_zenoh_subscriber`)**: This asynchronous function opens a Zenoh session and subscribes to all key expressions (`**`). It listens for incoming messages, optionally decodes them using the provided decoder function, and updates a shared `HashMap` data structure (`TopicCache`) with the latest information for each topic.

2.  **Optional Decoder System**: When enabled, each received message is passed through a user-defined decoder function that converts the raw payload into a human-readable string representation. The output is automatically HTML-escaped for security.

3.  **Shared State (`TopicCache`)**: An `Arc<RwLock<HashMap<String, TopicData>>>` is used to safely share the topic data between the Zenoh subscriber and the web server. The `RwLock` ensures concurrent read/write access is handled correctly, preventing data races.

4.  **Web Server (`start_web_server`)**: A simple web server built with the `warp` crate. It serves a dynamically generated HTML page that adapts based on whether a decoder is enabled, and provides a dedicated route (`/sse`) for the real-time data stream.

5.  **Server-Sent Events (`sse_handler`)**: This handler uses a `futures::stream::unfold` to continuously check for updates in the `TopicCache` every `RELOAD_PERIOD_MS`. It calculates the **delta** (topics that are new/updated or removed) and sends this as a JSON payload to the connected clients via SSE.

6.  **Adaptive Front-end (HTML/CSS/JS)**: The HTML page is dynamically generated based on decoder configuration. When enabled, it includes an additional "Decoded Content" column with appropriate styling. The client-side JavaScript connects to the `/sse` endpoint and dynamically updates the table, handling both standard and decoded content whilst maintaining alphabetical sorting.
