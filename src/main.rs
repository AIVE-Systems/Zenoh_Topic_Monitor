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
const DECODER: DecoderFn = Some(decoder::decoder); // Or None;

const LOG_LEVEL: log::LevelFilter = LevelFilter::Warn;
const PORT: u16 = 8080;
const RELOAD_PERIOD_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct TopicData {
    key_expr: String,
    last_data_size_bytes: u64,
    received_timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded_content: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeltaUpdate {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    updated: Vec<TopicData>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    removed: Vec<String>,
}

type TopicCache = Arc<RwLock<HashMap<String, TopicData>>>;

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
    cache: TopicCache,
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
        };

        debug!("Received data for topic '{}'", key_expr);
        cache.write().await.insert(key_expr, topic_data);
    }

    Ok(())
}

fn generate_html(has_decoder: bool) -> String {
    let decoder_column_header = if has_decoder {
        "<th>Decoded Content</th>"
    } else {
        ""
    };

    let topic_column_width = if has_decoder { "30%" } else { "80%" };
    let size_column_width = if has_decoder { "5%" } else { "5%" };
    let timestamp_column_width = if has_decoder { "15%" } else { "15%" };
    let decoder_column_width = if has_decoder { "50%" } else { "0%" };

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
    }}
    .stat-item {{
        text-align: center;
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
    th:nth-child(3) {{ width: {timestamp_width}; }}
    {decoder_th_style}
    td {{
        padding: 12px 16px;
        border-bottom: 1px solid #e8ecf0;
        vertical-align: top;
    }}
    td:nth-child(1) {{ width: {topic_width}; }}
    td:nth-child(2) {{ width: {size_width}; }}
    td:nth-child(3) {{ width: {timestamp_width}; }}
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
    }}
</style>
<script>
document.addEventListener("DOMContentLoaded", function() {{
    const tableBody = document.querySelector('tbody');
    const eventSource = new EventSource('/sse');
    const topics = new Map();
    const activeTopicsCount = document.querySelector('.stats .stat-value:first-child');
    const lastUpdatedTime = document.getElementById('last-updated-value');
    const sortButton = document.getElementById('sort-toggle-btn');
    const hasDecoder = {has_decoder_js};

    let sortMode = 'alphabetical'; // 'alphabetical' or 'timestamp'

    // updateStats(): updates topic count and last-updated display
    function updateStats() {{
        activeTopicsCount.textContent = topics.size;
        lastUpdatedTime.textContent = new Date().toLocaleTimeString();
    }}

    // escapeForSelector(s): return an escaped string safe for CSS selectors
    function escapeForSelector(s) {{
        if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {{
            return CSS.escape(s);
        }}
        // escape " ' and backslash by prefixing with backslash
        return s.replace(/["'\\]/g, '\\$&');
    }}

    // getRowByKey(topicKey): find a <tr> whose data-key matches topicKey
    function getRowByKey(topicKey) {{
        const rows = tableBody.querySelectorAll('tr');
        for (const r of rows) {{
            if (r.dataset.key === topicKey) return r;
        }}
        return null;
    }}

    // sortTopics(): sort all topics based on current sort mode
    function sortTopics() {{
        const topicArray = Array.from(topics.values());

        if (sortMode === 'alphabetical') {{
            topicArray.sort((a, b) => a.key_expr.localeCompare(b.key_expr));
        }} else {{
            // Sort by timestamp, most recent first
            topicArray.sort((a, b) => b.received_timestamp - a.received_timestamp);
        }}

        return topicArray;
    }}

    // rebuildTable(): completely rebuild the table with current sort order
    function rebuildTable() {{
        // Clear existing rows
        tableBody.innerHTML = '';

        // Get sorted topics and rebuild
        const sortedTopics = sortTopics();
        sortedTopics.forEach(topicData => {{
            createAndInsertRow(topicData);
        }});
    }}

    // createAndInsertRow(topicData): create a new row for the topic
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
            <td class="timestamp-cell">${{timestampReadable}}</td>
            ${{decodedContent}}
        `;

        tableBody.appendChild(row);
    }}

    // updateRow(topicData): insert or update a table row for the topic
    function updateRow(topicData) {{
        const timestampReadable = new Date(topicData.received_timestamp).toISOString().replace('T', ' ').replace('Z', ' UTC');

        let row = getRowByKey(topicData.key_expr);

        if (row) {{
            // Update existing row
            row.querySelector('.size-cell').textContent = topicData.last_data_size_bytes;
            row.querySelector('.timestamp-cell').textContent = timestampReadable;
            row.dataset.timestamp = topicData.received_timestamp;

            if (hasDecoder) {{
                const decodedCell = row.querySelector('.decoded-cell');
                if (decodedCell) decodedCell.innerHTML = topicData.decoded_content || '-';
            }}

            row.classList.add('updated-row');
            setTimeout(() => row.classList.remove('updated-row'), 500);

            // If sorting by timestamp, we may need to move the row
            if (sortMode === 'timestamp') {{
                row.remove();
                insertRowInOrder(row, topicData);
            }}
        }} else {{
            // Create new row
            createAndInsertRow(topicData);

            // If we're in alphabetical mode, we need to reposition
            if (sortMode === 'alphabetical') {{
                row = getRowByKey(topicData.key_expr);
                row.remove();
                insertRowInOrder(row, topicData);
            }}
        }}
    }}

    // insertRowInOrder(row, topicData): insert row in correct position based on sort mode
    function insertRowInOrder(row, topicData) {{
        const existingRows = tableBody.querySelectorAll('tr');
        let inserted = false;

        for (const existingRow of existingRows) {{
            let shouldInsertBefore = false;

            if (sortMode === 'alphabetical') {{
                const existingTopic = existingRow.querySelector('.topic-cell').textContent;
                shouldInsertBefore = topicData.key_expr.localeCompare(existingTopic) < 0;
            }} else {{
                // timestamp mode - most recent first
                const existingTimestamp = parseInt(existingRow.dataset.timestamp);
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

    // removeRow(topicKey): remove a row corresponding to topicKey
    function removeRow(topicKey) {{
        const row = getRowByKey(topicKey);
        if (row) row.remove();
    }}

    // toggleSort(): switch between sorting modes
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

    // Set up sort button click handler
    sortButton.addEventListener('click', toggleSort);

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

    updateStats();
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
        timestamp_width = timestamp_column_width,
        decoder_th_style = if has_decoder {
            format!("th:nth-child(4) {{ width: {}; }}", decoder_column_width)
        } else {
            String::new()
        },
        decoder_td_style = if has_decoder {
            format!("td:nth-child(4) {{ width: {}; }}", decoder_column_width)
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

                for (key, value) in current_cache.iter() {
                    if !last_keys.contains(key) || last_snapshot.get(key) != Some(value) {
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

    tokio::spawn({
        let cache_clone = topic_cache.clone();
        async move {
            if let Err(e) = start_zenoh_subscriber(cache_clone, custom_decoder).await {
                error!("Zenoh subscriber error: {}", e);
            }
        }
    });

    tokio::spawn(start_web_server(topic_cache.clone(), has_decoder));

    tokio::signal::ctrl_c().await?;

    warn!("Zenoh DDS Web Monitor stopping.");

    Ok(())
}
