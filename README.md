# Zenoh DDS Web Monitor 🌐

A lightweight, real-time web monitor for Zenoh, built with Rust. This tool subscribes to all topics (`**`) in a Zenoh network and displays them on a clean, responsive web interface, providing a convenient way to observe swarm robotics data streams.

---

## ✨ Features

- **Real-time Monitoring**: Automatically updates the list of active topics as new data arrives.
- **Topic Details**: Displays the key expression, the size of the latest message in bytes, and its reception timestamp.
- **Lightweight**: Built with Rust and the Warp framework for excellent performance and low resource consumption.
- **Server-Sent Events (SSE)**: Uses a persistent connection for efficient, real-time data push from the server to the browser without polling.
- **Automatic Sorting**: New topics are inserted into the table in alphabetical order.
- **Responsive Design**: The web interface is designed to work well on both desktop and mobile devices.
- **Integrated Logging**: Utilises the `ftail` crate to provide both console and file logging.

---

## 🛠️ Usage

### Run the Monitor

1.  Clone this repository:
    ```bash
    git clone https://github.com/your-username/zenoh-dds-web-monitor.git
    cd zenoh-dds-web-monitor
    ```
2.  Install the environment with Pixi
    ```bash
    pixi install
    ```
3.  Run the application with Cargo:
    ```bash
    pixi run server
    ```
4.  Open your web browser and navigate to `http://localhost:8080`.

You'll see a web page that automatically populates with topics as messages are published on the Zenoh network.

---

## 📚 Technical Overview

The application is structured around a few key components:

1.  **Zenoh Subscriber (`start_zenoh_subscriber`)**: This asynchronous function opens a Zenoh session and subscribes to all key expressions (`**`). It listens for incoming messages and updates a shared `HashMap` data structure (`TopicCache`) with the latest information for each topic.
2.  **Shared State (`TopicCache`)**: An `Arc<RwLock<HashMap<String, TopicData>>>` is used to safely share the topic data between the Zenoh subscriber and the web server. The `RwLock` ensures concurrent read/write access is handled correctly, preventing data races.
3.  **Web Server (`start_web_server`)**: A simple web server built with the `warp` crate. It serves a static HTML page for the main view and a dedicated route (`/sse`) for the real-time data stream.
4.  **Server-Sent Events (`sse_handler`)**: This handler uses a `futures::stream::unfold` to continuously check for updates in the `TopicCache` every `RELOAD_PERIOD_MS`. It calculates the **delta** (topics that are new/updated or removed) and sends this as a JSON payload to the connected clients via SSE.
5.  **Front-end (HTML/CSS/JS)**: The HTML page connects to the `/sse` endpoint and uses JavaScript to parse the incoming delta updates. It dynamically adds, removes, and updates rows in the table to reflect the current state of the Zenoh network. The client-side code handles the alphabetical sorting, so the server only needs to send the data changes, not the entire list.

This architecture ensures a **decoupled, efficient, and responsive system** for monitoring Zenoh topics in real time.
