use ftail::Ftail;
use log::{LevelFilter, debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{self, Duration};
use warp::{Filter, sse};
use zenoh::sample::Sample;

mod decoder;

type DecoderFn = Option<fn(Sample) -> String>;
const DECODER: DecoderFn = Some(decoder::simple_decoder); // Or None

const LOG_LEVEL: log::LevelFilter = LevelFilter::Warn;
const PORT: u16 = 8080;
const RELOAD_PERIOD_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TopicData {
    key_expr: String,
    last_data_size_bytes: u64,
    received_timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded_content: Option<String>,
    estimated_hz: f64,
}

#[derive(Debug, Serialize)]
struct DeltaUpdate {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    updated: Vec<TopicData>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    removed: Vec<String>,
}

type TopicCache = Arc<RwLock<HashMap<String, TopicData>>>;

const WINDOW_SIZE: usize = 20;
type IntervalHistory = Arc<RwLock<HashMap<String, (u64, Vec<u64>)>>>;

fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Converts a string to HTML-compliant format by escaping special characters
fn html_escape_string(input: &str) -> String {
    html_escape::encode_text(input).to_string()
}

async fn start_zenoh_subscriber(
    topic_cache: TopicCache,
    interval_history: IntervalHistory,
    decoder: DecoderFn,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Opening Zenoh session...");
    let mut config = zenoh::Config::default();
    config
        .insert_json5("connect/endpoints", "['tcp/127.0.0.1:7447']")
        .unwrap();
    config.insert_json5("mode", "'peer'").unwrap();
    let zenoh_session = zenoh::open(config).await.unwrap();

    let subscriber = zenoh_session
        .declare_subscriber("**")
        .await
        .map_err(|e| format!("Failed to declare subscriber: {}", e))?;

    info!("Zenoh subscriber started");
    while let Ok(sample) = subscriber.recv_async().await {
        let key_expr = sample.key_expr().as_str().to_string();
        let data_bytes = sample.payload().to_bytes().len() as u64;
        let timestamp = get_timestamp();

        let mut history = interval_history.write().await;
        let entry = history
            .entry(key_expr.clone())
            .or_insert((timestamp, Vec::new()));

        // compute delta against last timestamp
        let last_ts = entry.0;
        if timestamp > last_ts {
            let delta = timestamp - last_ts;
            entry.1.push(delta);
            if entry.1.len() > WINDOW_SIZE {
                entry.1.remove(0);
            }
        }
        entry.0 = timestamp; // update last seen timestamp

        let estimated_hz = if !entry.1.is_empty() {
            let avg_delta = entry.1.iter().sum::<u64>() as f64 / entry.1.len() as f64;
            if avg_delta > 0.0 {
                1000.0 / avg_delta
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Apply decoder if provided
        let decoded_content = decoder.map(|decode_fn| {
            let raw_decoded = decode_fn(sample.clone());
            html_escape_string(&raw_decoded)
        });

        let topic_data = TopicData {
            key_expr: key_expr.clone(),
            last_data_size_bytes: data_bytes,
            received_timestamp: timestamp,
            decoded_content,
            estimated_hz,
        };

        debug!("Received data for topic '{}'", key_expr);
        topic_cache.write().await.insert(key_expr, topic_data);
    }

    Ok(())
}

/// Generate HTML for the web UI.
/// `has_decoder`: whether to include the decoded-content column.
/// Returns the full HTML page as a `String`.
fn generate_html(has_decoder: bool) -> String {
    let decoder_column_header = if has_decoder {
        "<th>Decoded Content</th>"
    } else {
        ""
    };

    // let topic_column_width = if has_decoder { "25%" } else { "75%" };
    // let size_column_width = if has_decoder { "5%" } else { "5%" };
    // let freq_column_width = if has_decoder { "5%" } else { "5%" };
    // let timestamp_column_width = if has_decoder { "15%" } else { "15%" };
    // let decoder_column_width = if has_decoder { "50%" } else { "0%" };

    let topic_column_width = "25%";
    let size_column_width = "5%";
    let freq_column_width = "7%";
    let timestamp_column_width = "18%";
    let decoder_column_width = if has_decoder { "45%" } else { "0%" };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Zenoh DDS Topic Monitor</title>
<style>
    body {{
        display: flex;
        flex-direction: column;
        height: 100vh;
        margin: 0;
        font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
        background-color: #f5f7fa;
        color: #333;
    }}
    .header {{
        text-align: center;
        margin-bottom: 30px;
        flex-shrink: 0;
    }}
    .header h1 {{
        color: #2c3e50;
        margin: 0;
        font-size: 2.5rem;
        font-weight: 300;
    }}
    .header p {{
        color: #7f8c8d;
        margin: 10px 0 0 0;
        font-size: 1.1rem;
    }}
    .stats {{
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        color: white;
        padding: 20px;
        border-radius: 12px;
        margin-bottom: 25px;
        box-shadow: 0 4px 15px rgba(0,0,0,0.1);
        display: flex;
        justify-content: space-between;
        align-items: center;
        flex-shrink: 0;
        gap: 12px;
    }}
    .stat-item {{
        /* Keep items stacked (control on top, label/value under) and centred */
        text-align: center;
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 8px;
    }}
    .stat-value {{
        font-size: 2rem;
        font-weight: bold;
        display: block;
    }}
    .stat-label {{
        font-size: 0.9rem;
        opacity: 0.9;
    }}
    .controls {{
        display: flex;
        justify-content: center;
        align-items: center;
        gap: 15px;
        margin-bottom: 20px;
        flex-shrink: 0;
    }}
    .sort-toggle {{
        background: linear-gradient(135deg, #e17055 0%, #d63031 100%);
        color: white;
        border: none;
        padding: 12px 20px;
        border-radius: 8px;
        cursor: pointer;
        font-size: 0.9rem;
        font-weight: 600;
        box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        transition: all 0.3s ease;
        min-width: 180px;
    }}
    .sort-toggle:hover {{
        transform: translateY(-2px);
        box-shadow: 0 4px 15px rgba(0,0,0,0.15);
    }}
    .sort-toggle:active {{
        transform: translateY(0);
    }}

    /* Improved search input look */
    .filter-input {{
        padding: 8px 12px;
        border: 1px solid rgba(0,0,0,0.12);
        border-radius: 10px;
        font-size: 0.95rem;
        min-width: 220px;
        box-shadow: 0 2px 6px rgba(0,0,0,0.06);
        transition: box-shadow 0.15s ease, transform 0.08s ease, border-color 0.15s ease;
        background: white;
        color: #333;
    }}
    .filter-input::placeholder {{
        color: #9aa4b2;
    }}
    .filter-input:focus {{
        outline: none;
        box-shadow: 0 6px 18px rgba(102,126,234,0.12);
        transform: translateY(-1px);
        border-color: rgba(102,126,234,0.6);
    }}

    .container {{
        flex: 1 1 auto;
        display: flex;
        flex-direction: column;
        background: white;
        border-radius: 12px;
        box-shadow: 0 4px 20px rgba(0,0,0,0.08);
        overflow: hidden;
    }}
    table {{
        width: 100%;
        border-collapse: collapse;
        display: flex;
        flex-direction: column;
        height: 100%;
    }}
    thead {{
        display: table-header-group;
        background: linear-gradient(135deg, #4CAF50 0%, #45a049 100%);
        color: white;
    }}
    tbody {{
        display: block;
        overflow-y: auto;
        flex: 1 1 auto;
    }}
    tr {{
        display: table;
        width: 100%;
        table-layout: fixed;
    }}
    th {{
        background: linear-gradient(135deg, #4CAF50 0%, #45a049 100%);
        color: white;
        padding: 16px;
        text-align: left;
        font-weight: 600;
        font-size: 0.95rem;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        border-bottom: 1px solid #e8ecf0;
    }}
    th:nth-child(1) {{ width: {topic_width}; }}
    th:nth-child(2) {{ width: {size_width}; }}
    th:nth-child(3) {{ width: {freq_width}; }}
    th:nth-child(4) {{ width: {timestamp_width}; }}
    {decoder_th_style}
    td {{
        padding: 12px 16px;
        border-bottom: 1px solid #e8ecf0;
        vertical-align: top;
    }}
    td:nth-child(1) {{ width: {topic_width}; }}
    td:nth-child(2) {{ width: {size_width}; }}
    td:nth-child(3) {{ width: {freq_width}; }}
    td:nth-child(4) {{ width: {timestamp_width}; }}
    {decoder_td_style}
    tr:hover {{
        background-color: #f8f9fb;
        transition: background-color 0.2s ease;
    }}
    .topic-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        font-weight: 600;
        color: #3498db;
        word-break: break-all;
        position: relative;
    }}
    .size-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        word-break: break-word;
        background-color: transparent;
        border-radius: 4px;
        padding: 8px;
    }}
    .freq-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        word-break: break-word;
        background-color: transparent;
        border-radius: 4px;
        padding: 8px;
    }}
    .timestamp-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        white-space: nowrap;
        background-color: transparent;
        border-radius: 4px;
        padding: 8px;
    }}
    .decoded-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        background-color: transparent;
        border-radius: 4px;
        padding: 8px;
        max-height: 100px;
        overflow-y: auto;
        word-break: break-word;
        line-height: 1.3;
    }}
    .refresh-info {{
        text-align: center;
        margin-top: 25px;
        padding: 15px;
        background: linear-gradient(135deg, #74b9ff 0%, #0984e3 100%);
        color: white;
        border-radius: 8px;
        font-size: 0.9rem;
        flex-shrink: 0;
    }}
    .no-data {{
        text-align: center;
        padding: 40px;
        color: #6c757d;
        font-style: italic;
        font-size: 1.1rem;
    }}
    .updated-row {{
        animation: fade-highlight 0.5s ease-out;
    }}
    @keyframes fade-highlight {{
        from {{ background-color: #ffffa6; }}
        to {{ background-color: transparent; }}
    }}
    /* Responsive design */
    @media (max-width: 768px) {{
        .stats {{
            flex-direction: column;
            gap: 15px;
        }}
        .controls {{
            flex-direction: column;
            gap: 10px;
        }}
        .topic-cell, .size-cell, .decoded-cell {{
            max-width: none;
        }}
        th, td {{
            padding: 10px 8px;
            font-size: 0.9rem;
        }}
        .decoded-cell {{
            max-height: 80px;
        }}
        .filter-input {{
            min-width: 140px;
        }}
    }}
</style>
<script>
document.addEventListener("DOMContentLoaded", function() {{
    const tableBody = document.querySelector('tbody');
    const eventSource = new EventSource('/sse');
    const topics = new Map();

    // Explicit IDs for robustness
    const totalTopicsValue = document.getElementById('topic-count');
    const lastUpdatedTime = document.getElementById('last-updated-value');
    const sortButton = document.getElementById('sort-toggle-btn');
    const filterInput = document.getElementById('filter-input');
    const filteredCount = document.getElementById('filtered-count');
    const hasDecoder = {has_decoder_js};

    let sortMode = 'alphabetical'; // 'alphabetical' or 'timestamp'

    function updateStats() {{
        totalTopicsValue.textContent = topics.size;
        lastUpdatedTime.textContent = new Date().toLocaleTimeString();
    }}

    function getRowByKey(topicKey) {{
        const rows = tableBody.querySelectorAll('tr');
        for (const r of rows) {{
            if (r.dataset.key === topicKey) return r;
        }}
        return null;
    }}

    function sortTopics() {{
        const topicArray = Array.from(topics.values());
        if (sortMode === 'alphabetical') {{
            topicArray.sort((a, b) => a.key_expr.localeCompare(b.key_expr));
        }} else {{
            topicArray.sort((a, b) => b.received_timestamp - a.received_timestamp);
        }}
        return topicArray;
    }}

    function applyFilter() {{
        const filter = (filterInput.value || '').toLowerCase();
        let count = 0;
        const rows = tableBody.querySelectorAll('tr');
        rows.forEach(row => {{
            const topicCell = row.querySelector('.topic-cell');
            if (topicCell && topicCell.textContent.toLowerCase().includes(filter)) {{
                row.style.display = "";
                count++;
            }} else {{
                row.style.display = "none";
            }}
        }});
        filteredCount.textContent = `${{count}} Topics`;
    }}

    function rebuildTable() {{
        tableBody.innerHTML = '';
        sortTopics().forEach(topicData => createAndInsertRow(topicData));
        applyFilter();
    }}

    function createAndInsertRow(topicData) {{
        const timestampReadable = new Date(topicData.received_timestamp).toISOString().replace('T', ' ').replace('Z', ' UTC');
        const decodedContent = hasDecoder && topicData.decoded_content
            ? `<td class="decoded-cell">${{topicData.decoded_content}}</td>`
            : (hasDecoder ? '<td class="decoded-cell">-</td>' : '');

        const row = document.createElement('tr');
        row.dataset.key = topicData.key_expr;
        row.dataset.timestamp = topicData.received_timestamp;
        row.innerHTML = `
            <td class="topic-cell">${{topicData.key_expr}}</td>
            <td class="size-cell">${{topicData.last_data_size_bytes}}</td>
            <td class="freq-cell">${{topicData.estimated_hz}}</td>
            <td class="timestamp-cell">${{timestampReadable}}</td>
            ${{decodedContent}}
        `;
        tableBody.appendChild(row);
    }}

    function updateRow(topicData) {{
        const timestampReadable = new Date(topicData.received_timestamp).toISOString().replace('T', ' ').replace('Z', ' UTC');
        let row = getRowByKey(topicData.key_expr);

        if (row) {{
            row.querySelector('.size-cell').textContent = topicData.last_data_size_bytes ? topicData.last_data_size_bytes.toFixed(2) : "-";
            row.querySelector('.freq-cell').textContent = topicData.estimated_hz ? topicData.estimated_hz.toFixed(2) : "-";
            row.querySelector('.timestamp-cell').textContent = timestampReadable;
            row.dataset.timestamp = topicData.received_timestamp;

            if (hasDecoder) {{
                const decodedCell = row.querySelector('.decoded-cell');
                if (decodedCell) decodedCell.innerHTML = topicData.decoded_content || '-';
            }}

            row.classList.add('updated-row');
            setTimeout(() => row.classList.remove('updated-row'), 500);

            if (sortMode === 'timestamp') {{
                row.remove();
                insertRowInOrder(row, topicData);
            }}
        }} else {{
            createAndInsertRow(topicData);

            if (sortMode === 'alphabetical') {{
                row = getRowByKey(topicData.key_expr);
                if (row) {{
                    row.remove();
                    insertRowInOrder(row, topicData);
                }}
            }}
        }}

        // Reapply filter so new/updated row respects search
        applyFilter();
    }}

    function insertRowInOrder(row, topicData) {{
        const existingRows = tableBody.querySelectorAll('tr');
        let inserted = false;

        for (const existingRow of existingRows) {{
            let shouldInsertBefore = false;

            if (sortMode === 'alphabetical') {{
                const existingTopic = existingRow.querySelector('.topic-cell').textContent;
                shouldInsertBefore = topicData.key_expr.localeCompare(existingTopic) < 0;
            }} else {{
                const existingTimestamp = parseInt(existingRow.dataset.timestamp || '0', 10);
                shouldInsertBefore = topicData.received_timestamp > existingTimestamp;
            }}

            if (shouldInsertBefore) {{
                tableBody.insertBefore(row, existingRow);
                inserted = true;
                break;
            }}
        }}

        if (!inserted) {{
            tableBody.appendChild(row);
        }}
    }}

    function removeRow(topicKey) {{
        const row = getRowByKey(topicKey);
        if (row) row.remove();
        applyFilter();
    }}

    function toggleSort() {{
        if (sortMode === 'alphabetical') {{
            sortMode = 'timestamp';
            sortButton.textContent = 'Sort: Most Recent First';
        }} else {{
            sortMode = 'alphabetical';
            sortButton.textContent = 'Sort: Alphabetical';
        }}

        rebuildTable();
    }}

    function decayFrequencies() {{
        const now = Date.now();
        const rows = tableBody.querySelectorAll('tr');
        rows.forEach(row => {{
            const freqCell = row.querySelector('.freq-cell');
            const lastTs = parseInt(row.dataset.timestamp || '0', 10);
            if (freqCell && lastTs > 0) {{
                const elapsed = now - lastTs;
                if (elapsed > 5000) {{
                    let currentHz = parseFloat(freqCell.textContent) || 0;
                    // apply exponential decay factor for faster drop
                    currentHz *= 0.5; // halve every tick (1s)
                    freqCell.textContent = currentHz > 0.01 ? currentHz.toFixed(2) : "0.00";
                }}
            }}
        }});
    }}
    setInterval(decayFrequencies, 1000);

    // Event handlers
    sortButton.addEventListener('click', toggleSort);
    filterInput.addEventListener('input', applyFilter);

    eventSource.addEventListener("message", function(event) {{
        try {{
            const delta = JSON.parse(event.data);
            const updated = delta.updated || [];
            const removed = delta.removed || [];

            updated.forEach(topicData => {{
                topics.set(topicData.key_expr, topicData);
                updateRow(topicData);
            }});

            removed.forEach(topicKey => {{
                topics.delete(topicKey);
                removeRow(topicKey);
            }});

            updateStats();
        }} catch (error) {{
            console.error("Error processing SSE message:", error);
        }}
    }});

    // initial render state
    updateStats();
    applyFilter();
}});
</script>
</head>
<body>
<div class="header">
    <h1>Zenoh DDS Monitor</h1>
    <p>Real-time topic monitoring{decoder_subtitle}</p>
</div>
<div class="stats">
    <div class="stat-item">
        <span class="stat-value" id="topic-count">0</span>
        <span class="stat-label">Topics</span>
    </div>

    <div class="stat-item">
        <button id="sort-toggle-btn" class="sort-toggle">Alphabetical</button>
        <span class="stat-label">Sort Order</span>
    </div>

    <div class="stat-item">
        <!-- Search box above the filtered count (no extra label) -->
        <input
            type="text"
            id="filter-input"
            class="filter-input"
            placeholder="Filter topics..."
        />
        <span class="stat-label" id="filtered-count">0 Topics</span>
    </div>

    <div class="stat-item">
        <span class="stat-value" id="last-updated-value"></span>
        <span class="stat-label">Last Updated</span>
    </div>
</div>
<div class="container">
    <table>
        <thead>
            <tr>
                <th>Topic</th>
                <th>Message Size (B)</th>
                <th>Frequency (Hz)</th>
                <th>Received Timestamp</th>
                {decoder_header}
            </tr>
        </thead>
        <tbody></tbody>
    </table>
</div>
<div class="refresh-info">📊 Updates every {}ms | Built with Zenoh + Rust + Warp</div>
</body>
</html>"#,
        RELOAD_PERIOD_MS,
        topic_width = topic_column_width,
        size_width = size_column_width,
        freq_width = freq_column_width,
        timestamp_width = timestamp_column_width,
        decoder_th_style = if has_decoder {
            format!("th:nth-child(5) {{ width: {}; }}", decoder_column_width)
        } else {
            String::new()
        },
        decoder_td_style = if has_decoder {
            format!("td:nth-child(5) {{ width: {}; }}", decoder_column_width)
        } else {
            String::new()
        },
        has_decoder_js = if has_decoder { "true" } else { "false" },
        decoder_subtitle = if has_decoder {
            " with custom decoder"
        } else {
            ""
        },
        decoder_header = decoder_column_header,
    )
}

async fn sse_handler(
    cache: TopicCache,
    _has_decoder: bool,
) -> Result<impl warp::Reply, warp::Rejection> {
    let stream = futures::stream::unfold(
        (cache, HashMap::<String, TopicData>::new()),
        |(cache, mut last_snapshot)| async move {
            let (updated, removed) = {
                let mut interval = time::interval(Duration::from_millis(RELOAD_PERIOD_MS));
                interval.tick().await;

                let current_cache = cache.read().await;
                let mut updated: Vec<TopicData> = Vec::new();
                let mut removed: Vec<String> = Vec::new();

                let current_keys: HashSet<_> = current_cache.keys().collect();
                let last_keys: HashSet<_> = last_snapshot.keys().collect();

                // for (key, value) in current_cache.iter() {
                //     if !last_keys.contains(key) || last_snapshot.get(key) != Some(value) {
                //         updated.push(value.clone());
                //     }
                // }

                for (key, value) in current_cache.iter() {
                    let changed = match last_snapshot.get(key) {
                        Some(old) => {
                            old.received_timestamp != value.received_timestamp || old != value
                        }
                        None => true,
                    };
                    if changed {
                        updated.push(value.clone());
                    }
                }

                for key in last_keys.difference(&current_keys) {
                    removed.push(key.to_string());
                }

                last_snapshot.clear();
                last_snapshot.extend(current_cache.clone());

                (updated, removed)
            };

            let delta = DeltaUpdate { updated, removed };

            let event = sse::Event::default()
                .event("message")
                .data(serde_json::to_string(&delta).unwrap());

            Some((Ok::<_, warp::Error>(event), (cache, last_snapshot)))
        },
    );

    Ok(warp::sse::reply(warp::sse::keep_alive().stream(stream)))
}

async fn start_web_server(cache: TopicCache, has_decoder: bool) {
    let cache_filter = warp::any().map(move || cache.clone());
    let decoder_filter = warp::any().map(move || has_decoder);

    let index = warp::path::end()
        .and(decoder_filter)
        .map(|has_decoder| warp::reply::html(generate_html(has_decoder)))
        .boxed();

    let sse_route = warp::path("sse")
        .and(cache_filter)
        .and(decoder_filter)
        .and_then(sse_handler)
        .boxed();

    let routes = index.or(sse_route);

    info!("Starting web server on http://localhost:{}", PORT);
    warp::serve(routes).run(([127, 0, 0, 1], PORT)).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("logs")?;
    Ftail::new()
        .console(LOG_LEVEL)
        .daily_file(Path::new("logs"), LOG_LEVEL)
        .retention_days(3)
        .timezone(ftail::Tz::UTC)
        .datetime_format("%Y-%m-%d_%H:%M:%S%.3f")
        .init()
        .unwrap_or_else(|e| {
            error!("Failed to initialise logger: {}", e);
            std::process::exit(1);
        });

    info!("Starting Zenoh DDS Web Monitor...");

    // Determine if decoder should be used
    // Change this to Some(decoder) to enable the custom decoder
    let custom_decoder: DecoderFn = DECODER; // or Some(decoder)
    let has_decoder = custom_decoder.is_some();

    if has_decoder {
        info!("Custom decoder enabled");
    } else {
        info!("Running in standard mode (no custom decoder)");
    }

    let topic_cache: TopicCache = Arc::new(RwLock::new(HashMap::new()));
    let interval_history: IntervalHistory = Arc::new(RwLock::new(HashMap::new()));

    tokio::spawn({
        let cache_clone = topic_cache.clone();
        async move {
            if let Err(e) =
                start_zenoh_subscriber(cache_clone, interval_history, custom_decoder).await
            {
                error!("Zenoh subscriber error: {}", e);
            }
        }
    });

    tokio::spawn(start_web_server(topic_cache.clone(), has_decoder));

    tokio::signal::ctrl_c().await?;

    warn!("Zenoh DDS Web Monitor stopping.");

    Ok(())
}
