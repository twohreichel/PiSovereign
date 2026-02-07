# Monitoring

> 📊 Set up comprehensive monitoring with Prometheus and Grafana

This guide covers configuring monitoring for PiSovereign on Raspberry Pi.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Prometheus Metrics](#prometheus-metrics)
  - [Available Metrics](#available-metrics)
  - [Prometheus Configuration](#prometheus-configuration)
- [Grafana Dashboards](#grafana-dashboards)
  - [Installation](#grafana-installation)
  - [Dashboard Import](#dashboard-import)
  - [Dashboard Panels](#dashboard-panels)
- [Alerting](#alerting)
  - [Alert Rules](#alert-rules)
  - [Notification Channels](#notification-channels)
- [Log Aggregation](#log-aggregation)
  - [Loki Setup](#loki-setup)
  - [Promtail Configuration](#promtail-configuration)
- [Resource Optimization](#resource-optimization)

---

## Overview

PiSovereign monitoring stack:

```
┌─────────────────┐
│   PiSovereign   │
│  /metrics/      │
│  prometheus     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│   Prometheus    │────▶│    Grafana      │
│   (Metrics)     │     │  (Dashboards)   │
└─────────────────┘     └─────────────────┘
         │
         ▼
┌─────────────────┐
│  Alertmanager   │
│   (Alerts)      │
└─────────────────┘

┌─────────────────┐     ┌─────────────────┐
│    Promtail     │────▶│      Loki       │
│  (Log Shipper)  │     │  (Log Storage)  │
└─────────────────┘     └─────────────────┘
```

**Resource usage on Raspberry Pi 5**:

| Component | Memory | Storage/Day |
|-----------|--------|-------------|
| Prometheus | ~100 MB | ~50 MB |
| Grafana | ~150 MB | Minimal |
| Loki | ~200 MB | ~100 MB |
| Promtail | ~30 MB | - |
| **Total** | ~480 MB | ~150 MB |

---

## Quick Start

### Docker Compose (Recommended)

```yaml
# Add to docker-compose.yml
services:
  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    ports:
      - "127.0.0.1:9090:9090"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - ./prometheus/rules:/etc/prometheus/rules:ro
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=7d'
      - '--storage.tsdb.retention.size=1GB'
      - '--web.enable-lifecycle'
    networks:
      - pisovereign-net

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    restart: unless-stopped
    ports:
      - "127.0.0.1:3001:3000"
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/provisioning:/etc/grafana/provisioning:ro
      - ./grafana/dashboards:/var/lib/grafana/dashboards:ro
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD:-admin}
      - GF_USERS_ALLOW_SIGN_UP=false
    networks:
      - pisovereign-net

volumes:
  prometheus_data:
  grafana_data:
```

### Native Installation

```bash
# Install Prometheus
sudo apt update
sudo apt install -y prometheus

# Install Grafana
sudo apt install -y software-properties-common
wget -q -O - https://packages.grafana.com/gpg.key | sudo apt-key add -
echo "deb https://packages.grafana.com/oss/deb stable main" | sudo tee /etc/apt/sources.list.d/grafana.list
sudo apt update
sudo apt install -y grafana

# Start services
sudo systemctl enable prometheus grafana-server
sudo systemctl start prometheus grafana-server
```

---

## Prometheus Metrics

### Available Metrics

PiSovereign exposes metrics at `/metrics/prometheus`:

#### Application Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `app_uptime_seconds` | Counter | Application uptime |
| `app_version_info` | Gauge | Version information |

#### HTTP Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `http_requests_total` | Counter | `status` | Total HTTP requests |
| `http_requests_success_total` | Counter | - | 2xx responses |
| `http_requests_client_error_total` | Counter | - | 4xx responses |
| `http_requests_server_error_total` | Counter | - | 5xx responses |
| `http_requests_active` | Gauge | - | Active requests |
| `http_response_time_avg_ms` | Gauge | - | Average response time |
| `http_response_time_ms_bucket` | Histogram | `le` | Response time distribution |

#### Inference Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `inference_requests_total` | Counter | Total inference requests |
| `inference_requests_success_total` | Counter | Successful inferences |
| `inference_requests_failed_total` | Counter | Failed inferences |
| `inference_time_avg_ms` | Gauge | Average inference time |
| `inference_time_ms_bucket` | Histogram | Inference time distribution |
| `inference_tokens_total` | Counter | Total tokens generated |
| `inference_healthy` | Gauge | Health status (0/1) |

#### Cache Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `cache_hits_total` | Counter | Cache hits |
| `cache_misses_total` | Counter | Cache misses |
| `cache_size` | Gauge | Current cache size |

### Prometheus Configuration

```yaml
# prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - /etc/prometheus/rules/*.yml

scrape_configs:
  - job_name: 'pisovereign'
    static_configs:
      - targets: ['pisovereign:3000']
    metrics_path: /metrics/prometheus
    scrape_interval: 10s

  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['localhost:9100']

alerting:
  alertmanagers:
    - static_configs:
        - targets: ['alertmanager:9093']
```

---

## Grafana Dashboards

### Grafana Installation

If using native installation, configure data source:

1. Open Grafana: `http://<pi-ip>:3000`
2. Login: `admin` / `admin`
3. Go to **Configuration** → **Data Sources**
4. Add **Prometheus**:
   - URL: `http://localhost:9090`
   - Click **Save & Test**

### Dashboard Import

Import the pre-built dashboard:

**Option A: File Import**

1. Go to **Dashboards** → **Import**
2. Upload `grafana/dashboards/pisovereign.json`
3. Select Prometheus data source

**Option B: Provisioning**

```yaml
# grafana/provisioning/dashboards/dashboards.yml
apiVersion: 1

providers:
  - name: 'PiSovereign'
    orgId: 1
    folder: 'PiSovereign'
    type: file
    disableDeletion: false
    updateIntervalSeconds: 30
    options:
      path: /var/lib/grafana/dashboards
```

```yaml
# grafana/provisioning/datasources/datasources.yml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
```

### Dashboard Panels

The PiSovereign dashboard includes:

#### Overview Row

| Panel | Description |
|-------|-------------|
| **Uptime** | Application uptime counter |
| **Inference Status** | Hailo NPU health indicator |
| **Total Requests** | Cumulative request count |
| **Active Requests** | Current in-flight requests |
| **Avg Response Time** | Mean latency |
| **Total Tokens** | LLM tokens generated |

#### HTTP Requests Row

| Panel | Visualization | Description |
|-------|--------------|-------------|
| **Request Rate** | Time series | Requests/second over time |
| **Status Distribution** | Pie chart | Success/Error breakdown |
| **Response Time P50/P90/P99** | Stat | Latency percentiles |

#### Inference Row

| Panel | Visualization | Description |
|-------|--------------|-------------|
| **Inference Rate** | Time series | Inferences/second |
| **Inference Latency** | Gauge | Current avg latency |
| **Token Rate** | Time series | Tokens/second |
| **Model Usage** | Table | Per-model statistics |

#### System Row

| Panel | Description |
|-------|-------------|
| **CPU Usage** | System CPU utilization |
| **Memory Usage** | RAM usage |
| **Disk I/O** | Storage throughput |
| **Network I/O** | Network traffic |

---

## Alerting

### Alert Rules

```yaml
# prometheus/rules/pisovereign.yml
groups:
  - name: pisovereign
    rules:
      # Critical: Application down
      - alert: PiSovereignDown
        expr: up{job="pisovereign"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "PiSovereign is down"
          description: "PiSovereign has been unreachable for more than 1 minute."

      # Critical: Inference engine unhealthy
      - alert: InferenceEngineUnhealthy
        expr: inference_healthy == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Hailo NPU is unhealthy"
          description: "The inference engine has been unhealthy for 2 minutes."

      # Warning: High response time
      - alert: HighResponseTime
        expr: http_response_time_avg_ms > 5000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High API response time"
          description: "Average response time is {{ $value }}ms (threshold: 5000ms)"

      # Warning: High inference time
      - alert: HighInferenceTime
        expr: inference_time_avg_ms > 10000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High inference latency"
          description: "Average inference time is {{ $value }}ms (threshold: 10000ms)"

      # Warning: High error rate
      - alert: HighErrorRate
        expr: rate(http_requests_server_error_total[5m]) / rate(http_requests_total[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High server error rate"
          description: "Error rate is {{ $value | humanizePercentage }} (threshold: 5%)"

      # Warning: Inference failures
      - alert: InferenceFailures
        expr: rate(inference_requests_failed_total[5m]) / rate(inference_requests_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High inference failure rate"
          description: "Inference failure rate is {{ $value | humanizePercentage }}"

      # Info: No traffic
      - alert: NoTraffic
        expr: rate(http_requests_total[15m]) == 0
        for: 15m
        labels:
          severity: info
        annotations:
          summary: "No traffic"
          description: "No requests received in the last 15 minutes"

      # Info: High token usage
      - alert: HighTokenUsage
        expr: rate(inference_tokens_total[1h]) > 100000
        for: 1h
        labels:
          severity: info
        annotations:
          summary: "High token generation rate"
          description: "Token rate: {{ $value | humanize }}/hour"
```

### Notification Channels

Configure Alertmanager for notifications:

```yaml
# alertmanager/alertmanager.yml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'severity']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'critical'
    - match:
        severity: warning
      receiver: 'warning'

receivers:
  - name: 'default'
    # Add webhook, email, etc.

  - name: 'critical'
    webhook_configs:
      - url: 'http://localhost:3000/v1/alerts/webhook'
        send_resolved: true

  - name: 'warning'
    # Email or Slack configuration
```

---

## Log Aggregation

### Loki Setup

Add to Docker Compose:

```yaml
services:
  loki:
    image: grafana/loki:latest
    container_name: loki
    restart: unless-stopped
    ports:
      - "127.0.0.1:3100:3100"
    volumes:
      - ./loki:/etc/loki:ro
      - loki_data:/loki
    command: -config.file=/etc/loki/loki.yml
    networks:
      - pisovereign-net

volumes:
  loki_data:
```

```yaml
# loki/loki.yml
auth_enabled: false

server:
  http_listen_port: 3100

common:
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules
  replication_factor: 1
  ring:
    kvstore:
      store: inmemory

schema_config:
  configs:
    - from: 2020-10-24
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

limits_config:
  retention_period: 168h  # 7 days
```

### Promtail Configuration

```yaml
# promtail/promtail.yml
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: pisovereign
    static_configs:
      - targets:
          - localhost
        labels:
          job: pisovereign
          __path__: /var/log/pisovereign/*.log
    pipeline_stages:
      - json:
          expressions:
            level: level
            message: message
            target: target
            request_id: request_id
      - labels:
          level:
          target:
      - timestamp:
          source: timestamp
          format: RFC3339

  - job_name: journal
    journal:
      max_age: 12h
      labels:
        job: systemd-journal
    relabel_configs:
      - source_labels: ['__journal__systemd_unit']
        target_label: 'unit'
```

Add Loki data source to Grafana:

1. **Configuration** → **Data Sources** → **Add**
2. Select **Loki**
3. URL: `http://loki:3100`
4. **Save & Test**

---

## Resource Optimization

### Reduce Prometheus Memory

```yaml
# prometheus.yml
global:
  scrape_interval: 30s  # Increase from 15s

storage:
  tsdb:
    retention.time: 3d  # Reduce from 7d
    retention.size: 500MB  # Reduce from 1GB
```

### Reduce Grafana Memory

```ini
# /etc/grafana/grafana.ini
[database]
type = sqlite3
path = /var/lib/grafana/grafana.db

[session]
provider = memory

[caching]
enabled = false
```

### Optimize Loki

```yaml
# loki.yml
limits_config:
  retention_period: 72h  # 3 days instead of 7
  ingestion_rate_mb: 4
  ingestion_burst_size_mb: 6
  max_streams_per_user: 10000
```

---

## Troubleshooting

### Metrics not appearing

```bash
# Check endpoint
curl http://localhost:3000/metrics/prometheus

# Check Prometheus targets
curl http://localhost:9090/api/v1/targets
```

### Grafana dashboard empty

1. Check data source connection
2. Verify time range includes recent data
3. Check Prometheus has data: `http://localhost:9090/graph`

### High memory usage

```bash
# Check Prometheus memory
curl http://localhost:9090/api/v1/status/runtimeinfo

# Reduce retention
sudo systemctl edit prometheus
# Add: --storage.tsdb.retention.time=3d
```

---

## Next Steps

- [Backup & Restore](./backup-restore.md) - Protect your data
- [Security Hardening](../security/hardening.md) - Secure monitoring endpoints
