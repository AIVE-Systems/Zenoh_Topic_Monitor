use ftail::Ftail;
use log::{LevelFilter, error, info};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{self, Duration};
use warp::{Filter, sse};

const LOG_LEVEL: log::LevelFilter = LevelFilter::Info;
const PORT: u16 = 8080;
const RELOAD_PERIOD_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct TopicData {
    key_expr: String,
    last_data_size_bytes: u64,
    received_timestamp: u64,
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

async fn start_zenoh_subscriber(cache: TopicCache) -> Result<(), Box<dyn std::error::Error>> {
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

    info!("Zenoh subscriber started, waiting for messages...");
    while let Ok(sample) = subscriber.recv_async().await {
        let key_expr = sample.key_expr().as_str().to_string();
        let data_bytes = sample.payload().to_bytes().len() as u64;
        let timestamp = get_timestamp();

        let topic_data = TopicData {
            key_expr: key_expr.clone(),
            last_data_size_bytes: data_bytes,
            received_timestamp: timestamp,
        };
        info!("Received data for topic '{}'", key_expr);

        cache.write().await.insert(key_expr, topic_data);
    }

    Ok(())
}

fn generate_html() -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"UTF-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
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
        flex: 1 1 auto;
    }}
    thead {{
        flex: 0 0 auto;
        background: linear-gradient(135deg, #4CAF50 0%, #45a049 100%);
        color: white;
    }}
    tbody {{
        flex: 1 1 auto;
        display: block;
        overflow-y: auto;
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
    td {{
        padding: 12px 16px;
        border-bottom: 1px solid #e8ecf0;
        vertical-align: top;
    }}
    tr:hover {{
        background-color: #f8f9fb;
        transition: background-color 0.2s ease;
    }}
    .topic-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        font-weight: 600;
        color: #3498db;
        max-width: 350px;
        word-break: break-all;
        position: relative;
    }}
    .size-cell {{
        font-family: 'Fira Code', 'Courier New', monospace;
        max-width: 450px;
        word-break: break-word;
        background-color: #f8f9fa;
        border-radius: 4px;
        padding: 8px;
        font-size: 0.9rem;
        line-height: 1.4;
    }}
    .timestamp-cell {{
        font-size: 0.85rem;
        color: #6c757d;
        white-space: nowrap;
        min-width: 180px;
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
        to {{ background-color: #f8f9fb; }}
    }}
    /* Responsive design */
    @media (max-width: 768px) {{
        .stats {{
            flex-direction: column;
            gap: 15px;
        }}
        .topic-cell, .size-cell {{
            max-width: none;
        }}
        th, td {{
            padding: 10px 8px;
            font-size: 0.9rem;
        }}
    }}
</style>
<script>
document.addEventListener("DOMContentLoaded", function() {{
    const tableBody = document.querySelector('tbody');
    const eventSource = new EventSource('/sse');
    const topics = new Map();
    const activeTopicsCount = document.querySelector('.stats .stat-value:first-child');
    const lastUpdatedTime = document.querySelector('.stats .stat-value:last-child');

    function updateStats() {{
        activeTopicsCount.textContent = topics.size;
        lastUpdatedTime.textContent = new Date().toLocaleTimeString();
    }}

    function updateRow(topicData) {{
        const rowId = `row-${{topicData.key_expr}}`;
        let row = document.getElementById(rowId);
        const timestampReadable = new Date(topicData.received_timestamp).toISOString().replace('T', ' ').replace('Z', ' UTC');

        if (row) {{
            row.querySelector('.size-cell').textContent = topicData.last_data_size_bytes;
            row.querySelector('.timestamp-cell').textContent = timestampReadable;
            row.classList.add('updated-row');
            setTimeout(() => row.classList.remove('updated-row'), 500);
        }} else {{
            row = document.createElement('tr');
            row.id = rowId;
            row.innerHTML = `
                <td class="topic-cell">${{topicData.key_expr}}</td>
                <td class="size-cell">${{topicData.last_data_size_bytes}}</td>
                <td class="timestamp-cell">${{timestampReadable}}</td>
            `;

            const sortedKeys = Array.from(topics.keys()).sort((a, b) => a.localeCompare(b));
            const newIndex = sortedKeys.indexOf(topicData.key_expr);
            const nextKey = sortedKeys[newIndex + 1];

            if (nextKey) {{
                const nextRow = document.getElementById(`row-${{nextKey}}`);
                if (nextRow) {{
                    tableBody.insertBefore(row, nextRow);
                }} else {{
                    tableBody.appendChild(row);
                }}
            }} else {{
                tableBody.appendChild(row);
            }}
        }}
    }}

    function removeRow(topicKey) {{
        const rowId = `row-${{topicKey}}`;
        const row = document.getElementById(rowId);
        if (row) {{
            row.remove();
        }}
    }}

    eventSource.addEventListener("message", function(event) {{
        const delta = JSON.parse(event.data);

        delta.updated.forEach(topicData => {{
            topics.set(topicData.key_expr, topicData);
            updateRow(topicData);
        }});

        delta.removed.forEach(topicKey => {{
            topics.delete(topicKey);
            removeRow(topicKey);
        }});

        updateStats();
    }});
}});
</script>

</head>
<body>
<div class="header">
    <h1>Zenoh DDS Monitor</h1>
    <p>Real-time topic monitoring</p>
</div>
<div class="stats">
    <div class="stat-item">
        <span class="stat-value">0</span>
        <span class="stat-label">Active Topics</span>
    </div>
    <div class="stat-item">
        <span class="stat-value">{}</span>
        <span class="stat-label">Last Updated</span>
    </div>
</div>
    <div class="container">
    <table>
        <thead>
            <tr>
            <th>Topic</th>
            <th>Latest Message Data Size (B)</th>
            <th>Received Timestamp</th>
            </tr>
        </thead>
        <tbody> </tbody>
    </table>
</div>
<div class="refresh-info"> 🔄 Updates every {}ms | Built with Zenoh + Rust + Warp </div>
</body>
</html>"#,
        chrono::Utc::now().format("%H:%M:%S"),
        RELOAD_PERIOD_MS
    )
}

async fn sse_handler(cache: TopicCache) -> Result<impl warp::Reply, warp::Rejection> {
    let stream = futures::stream::unfold(
        (cache, HashMap::<String, TopicData>::new()),
        |(cache, last_snapshot)| async move {
            let (updated, removed, new_last_snapshot) = {
                let mut interval = time::interval(Duration::from_millis(RELOAD_PERIOD_MS));
                interval.tick().await;

                let current_cache = cache.read().await;

                let mut updated: Vec<TopicData> = Vec::new();
                let mut removed: Vec<String> = Vec::new();

                let current_keys: HashSet<&String> = current_cache.keys().collect();
                let last_keys: HashSet<&String> = last_snapshot.keys().collect();

                for (key, value) in current_cache.iter() {
                    if last_snapshot.get(key) != Some(value) {
                        updated.push(value.clone());
                    }
                }

                for key in last_keys.difference(&current_keys) {
                    removed.push(key.to_string());
                }

                let new_last_snapshot = current_cache.clone();

                (updated, removed, new_last_snapshot)
            };

            let delta = DeltaUpdate { updated, removed };

            let event = sse::Event::default()
                .event("message")
                .data(serde_json::to_string(&delta).unwrap());

            Some((Ok::<_, warp::Error>(event), (cache, new_last_snapshot)))
        },
    );

    Ok(warp::sse::reply(warp::sse::keep_alive().stream(stream)))
}

async fn start_web_server(cache: TopicCache) {
    let cache_filter = warp::any().map(move || cache.clone());

    let index = warp::path::end()
        .map(|| warp::reply::html(generate_html()))
        .boxed();

    let sse_route = warp::path("sse")
        .and(cache_filter)
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

    let topic_cache: TopicCache = Arc::new(RwLock::new(HashMap::new()));

    tokio::spawn({
        let cache_clone = topic_cache.clone();
        async move {
            if let Err(e) = start_zenoh_subscriber(cache_clone).await {
                error!("Zenoh subscriber error: {}", e);
            }
        }
    });

    tokio::spawn(start_web_server(topic_cache.clone()));

    tokio::signal::ctrl_c().await?;

    info!("Zenoh DDS Web Monitor stopped.");

    Ok(())
}
