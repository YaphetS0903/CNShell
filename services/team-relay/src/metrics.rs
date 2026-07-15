use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

struct MetricsInner {
    started_at: Instant,
    http_requests: AtomicU64,
    http_2xx: AtomicU64,
    http_4xx: AtomicU64,
    http_5xx: AtomicU64,
    readiness_checks: AtomicU64,
    readiness_failures: AtomicU64,
    websocket_connections: AtomicU64,
    websocket_active: AtomicU64,
}

#[derive(Clone)]
pub struct RelayMetrics {
    inner: Arc<MetricsInner>,
}

impl Default for RelayMetrics {
    fn default() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                started_at: Instant::now(),
                http_requests: AtomicU64::new(0),
                http_2xx: AtomicU64::new(0),
                http_4xx: AtomicU64::new(0),
                http_5xx: AtomicU64::new(0),
                readiness_checks: AtomicU64::new(0),
                readiness_failures: AtomicU64::new(0),
                websocket_connections: AtomicU64::new(0),
                websocket_active: AtomicU64::new(0),
            }),
        }
    }
}

impl RelayMetrics {
    pub fn record_readiness(&self, ready: bool) {
        self.inner.readiness_checks.fetch_add(1, Ordering::Relaxed);
        if !ready {
            self.inner
                .readiness_failures
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn begin_websocket(&self) -> ActiveWebSocketGuard {
        self.inner
            .websocket_connections
            .fetch_add(1, Ordering::Relaxed);
        self.inner.websocket_active.fetch_add(1, Ordering::Relaxed);
        ActiveWebSocketGuard {
            metrics: self.clone(),
        }
    }

    fn record_http(&self, status: StatusCode) {
        self.inner.http_requests.fetch_add(1, Ordering::Relaxed);
        match status.as_u16() / 100 {
            2 => &self.inner.http_2xx,
            4 => &self.inner.http_4xx,
            5 => &self.inner.http_5xx,
            _ => return,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    pub fn render(&self, ready: bool) -> String {
        format!(
            concat!(
                "# HELP cnshell_relay_up Whether the relay process is running.\n",
                "# TYPE cnshell_relay_up gauge\n",
                "cnshell_relay_up 1\n",
                "# HELP cnshell_relay_ready Whether the relay database is ready.\n",
                "# TYPE cnshell_relay_ready gauge\n",
                "cnshell_relay_ready {}\n",
                "# HELP cnshell_relay_uptime_seconds Relay process uptime.\n",
                "# TYPE cnshell_relay_uptime_seconds gauge\n",
                "cnshell_relay_uptime_seconds {}\n",
                "# HELP cnshell_relay_http_requests_total HTTP responses by status class.\n",
                "# TYPE cnshell_relay_http_requests_total counter\n",
                "cnshell_relay_http_requests_total{{class=\"all\"}} {}\n",
                "cnshell_relay_http_requests_total{{class=\"2xx\"}} {}\n",
                "cnshell_relay_http_requests_total{{class=\"4xx\"}} {}\n",
                "cnshell_relay_http_requests_total{{class=\"5xx\"}} {}\n",
                "# HELP cnshell_relay_readiness_checks_total Database readiness checks.\n",
                "# TYPE cnshell_relay_readiness_checks_total counter\n",
                "cnshell_relay_readiness_checks_total {}\n",
                "# HELP cnshell_relay_readiness_failures_total Failed database readiness checks.\n",
                "# TYPE cnshell_relay_readiness_failures_total counter\n",
                "cnshell_relay_readiness_failures_total {}\n",
                "# HELP cnshell_relay_websocket_connections_total Authorized WebSocket connections.\n",
                "# TYPE cnshell_relay_websocket_connections_total counter\n",
                "cnshell_relay_websocket_connections_total {}\n",
                "# HELP cnshell_relay_websocket_active Active authorized WebSocket connections.\n",
                "# TYPE cnshell_relay_websocket_active gauge\n",
                "cnshell_relay_websocket_active {}\n"
            ),
            u8::from(ready),
            self.inner.started_at.elapsed().as_secs(),
            self.inner.http_requests.load(Ordering::Relaxed),
            self.inner.http_2xx.load(Ordering::Relaxed),
            self.inner.http_4xx.load(Ordering::Relaxed),
            self.inner.http_5xx.load(Ordering::Relaxed),
            self.inner.readiness_checks.load(Ordering::Relaxed),
            self.inner.readiness_failures.load(Ordering::Relaxed),
            self.inner.websocket_connections.load(Ordering::Relaxed),
            self.inner.websocket_active.load(Ordering::Relaxed),
        )
    }
}

pub struct ActiveWebSocketGuard {
    metrics: RelayMetrics,
}

impl Drop for ActiveWebSocketGuard {
    fn drop(&mut self) {
        self.metrics
            .inner
            .websocket_active
            .fetch_sub(1, Ordering::Relaxed);
    }
}

pub async fn track_http(
    State(metrics): State<RelayMetrics>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    metrics.record_http(response.status());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_bounded_and_active_websockets_release_on_drop() {
        let metrics = RelayMetrics::default();
        metrics.record_http(StatusCode::OK);
        metrics.record_http(StatusCode::NOT_FOUND);
        metrics.record_http(StatusCode::SERVICE_UNAVAILABLE);
        metrics.record_readiness(false);
        {
            let _first = metrics.begin_websocket();
            let _second = metrics.begin_websocket();
            let rendered = metrics.render(false);
            assert!(rendered.contains("cnshell_relay_websocket_active 2"));
        }
        let rendered = metrics.render(true);
        assert!(rendered.contains("class=\"all\"} 3"));
        assert!(rendered.contains("class=\"2xx\"} 1"));
        assert!(rendered.contains("class=\"4xx\"} 1"));
        assert!(rendered.contains("class=\"5xx\"} 1"));
        assert!(rendered.contains("cnshell_relay_readiness_failures_total 1"));
        assert!(rendered.contains("cnshell_relay_websocket_connections_total 2"));
        assert!(rendered.contains("cnshell_relay_websocket_active 0"));
        assert!(!rendered.contains("workspace"));
        assert!(!rendered.contains("device"));
        assert!(!rendered.contains("room"));
    }
}
