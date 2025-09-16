use ftail::Ftail;
use log::{LevelFilter, error, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use warp::Filter;

const LOG_LEVEL: log::LevelFilter = LevelFilter::Info;
const REFRESH_PERIOD_MS: u16 = 1000;
const PORT: u16 = 8080;

// Data structure to hold topic information
#[derive(Debug, Clone)]
struct TopicData {
    key_expr: String,
    last_data_size_bytes: u64,
    received_timestamp: u64,
}

// Shared state for storing topic data
type SharedTopics = Arc<Mutex<HashMap<String, TopicData>>>;

// Get current timestamp in milliseconds
fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// Start Zenoh subscriber for all topics
async fn start_zenoh_subscriber(topics: SharedTopics) -> Result<(), Box<dyn std::error::Error>> {
    info!("Opening Zenoh session...");

    // Configure Zenoh similar to your main application
    let mut config = zenoh::Config::default();
    config
        .insert_json5("connect/endpoints", "['tcp/127.0.0.1:7447']")
        .unwrap();
    config.insert_json5("mode", "'peer'").unwrap();
    let zenoh_session = zenoh::open(config).await.unwrap();
    info!("Zenoh session opened.");

    info!("Subscribing to all topics (**)");
    let subscriber = zenoh_session
        .declare_subscriber("**")
        .await
        .map_err(|e| format!("Failed to declare subscriber: {}", e))?;

    info!("Zenoh subscriber started, waiting for messages...");

    while let Ok(sample) = subscriber.recv_async().await {
        let key_expr = sample.key_expr().as_str();
        let data_bytes = *(&sample.payload().to_bytes().len()) as u64;
        let timestamp = get_timestamp();

        let topic_data = TopicData {
            key_expr: key_expr.to_string(),
            last_data_size_bytes: data_bytes,
            received_timestamp: timestamp,
        };

        {
            let mut topics_map = topics.lock().unwrap();
            topics_map.insert(key_expr.to_string(), topic_data);
        }

        info!(
            "Received {}B on topic '{}' at {}",
            data_bytes, key_expr, timestamp
        );
    }

    Ok(())
}

// Generate HTML page with improved styling
fn generate_html(topics: &HashMap<String, TopicData>) -> String {
    let mut rows = String::new();

    // Sort topics alphabetically for consistent display
    let mut sorted_topics: Vec<_> = topics.iter().collect();
    sorted_topics.sort_by_key(|(k, _)| k.as_str());

    for (_, topic_data) in sorted_topics {
        let timestamp_readable =
            chrono::DateTime::from_timestamp_millis(topic_data.received_timestamp as i64)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string())
                .unwrap_or_else(|| "Invalid timestamp".to_string());

        rows.push_str(&format!(
            r#"
            <tr>
                <td class="topic-cell">{}</td>
                <td class="data-cell">{}</td>
                <td class="timestamp-cell">{}</td>
            </tr>"#,
            html_escape::encode_text(&topic_data.key_expr),
            topic_data.last_data_size_bytes,
            timestamp_readable
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Zenoh DDS Topic Monitor</title>
    <style>
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f5f7fa;
            color: #333;
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
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
            background: white;
            border-radius: 12px;
            box-shadow: 0 4px 20px rgba(0,0,0,0.08);
            overflow: hidden;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
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
        .data-cell {{
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
        }}
        .no-data {{
            text-align: center;
            padding: 40px;
            color: #6c757d;
            font-style: italic;
            font-size: 1.1rem;
        }}
        /* Responsive design */
        @media (max-width: 768px) {{
            .stats {{
                flex-direction: column;
                gap: 15px;
            }}
            .topic-cell, .data-cell {{
                max-width: none;
            }}
            th, td {{
                padding: 10px 8px;
                font-size: 0.9rem;
            }}
        }}
    </style>
    <script>
        // Auto-refresh
        setTimeout(function() {{
            location.reload();
        }}, {});

        // Add loading indicator
        window.addEventListener('beforeunload', function() {{
            document.body.style.opacity = '0.7';
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
            <span class="stat-value">{}</span>
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
            <tbody>
                {}
            </tbody>
        </table>
    </div>

    <div class="refresh-info">
        🔄 Page refreshes automatically every {}ms | Built with Zenoh + Rust + Warp
    </div>
</body>
</html>"#,
        REFRESH_PERIOD_MS,
        topics.len(),
        chrono::Utc::now().format("%H:%M:%S"),
        if rows.is_empty() {
            r#"<tr><td colspan="3" class="no-data">No topics received yet... Waiting for Zenoh messages</td></tr>"#
        } else {
            &rows
        },
        REFRESH_PERIOD_MS,
    )
}

/// Start the web server.
///
/// # Arguments
/// * `topics` - Shared topics state wrapped in Arc<Mutex<_>>.
async fn start_web_server(topics: SharedTopics) {
    let topics_filter = warp::any().map(move || topics.clone());

    let routes = warp::path::end()
        .and(topics_filter)
        .map(|topics: SharedTopics| {
            let topics_map = topics.lock().unwrap();
            let html = generate_html(&topics_map);
            warp::reply::html(html)
        });

    let port: u16 = PORT as u16;

    info!("Starting web server on http://localhost:{}", port);
    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("logs").unwrap();
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

    if REFRESH_PERIOD_MS == 0 {
        error!("Refresh period cannot be zero");
        std::process::exit(1);
    }

    // Shared state for storing topic data
    let topics: SharedTopics = Arc::new(Mutex::new(HashMap::new()));

    info!("Starting Zenoh DDS Web Monitor...");

    // Clone for different tasks
    let topics_subscriber = topics.clone();
    let topics_server = topics.clone();

    // Start Zenoh subscriber task
    let subscriber_task = tokio::spawn(async move {
        if let Err(e) = start_zenoh_subscriber(topics_subscriber).await {
            error!("Zenoh subscriber error: {}", e);
        }
    });

    // Start web server task
    let server_task = tokio::spawn(async move {
        start_web_server(topics_server).await;
    });

    // Wait for either task to complete (or both)
    tokio::select! {
        _ = subscriber_task => warn!("Zenoh subscriber task completed"),
        _ = server_task => warn!("Web server task completed"),
        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-C received. Shutting down gracefully...");
        }
    }

    info!("Zenoh DDS Web Monitor stopped.");
    Ok(())
}
