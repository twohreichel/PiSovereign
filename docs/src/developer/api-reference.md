# API Reference

> 📡 REST API documentation for PiSovereign

This document provides complete REST API documentation including authentication, endpoints, and the OpenAPI specification.

## Table of Contents

- [Overview](#overview)
- [Authentication](#authentication)
  - [API Key Authentication](#api-key-authentication)
  - [Error Responses](#authentication-errors)
- [Rate Limiting](#rate-limiting)
- [Endpoints](#endpoints)
  - [Health & Status](#health--status)
  - [Chat](#chat)
  - [Commands](#commands)
  - [System](#system)
  - [Webhooks](#webhooks)
  - [Metrics](#metrics)
- [Error Handling](#error-handling)
- [OpenAPI Specification](#openapi-specification)

---

## Overview

### Base URL

```
http://localhost:3000      # Development
https://your-domain.com    # Production (behind Traefik)
```

### Content Type

All requests and responses use JSON:

```
Content-Type: application/json
Accept: application/json
```

### Request ID

Every response includes a correlation ID for debugging:

```
X-Request-Id: 550e8400-e29b-41d4-a716-446655440000
```

Include this when reporting issues.

---

## Authentication

### API Key Authentication

Protected endpoints require an API key in the `Authorization` header:

```http
Authorization: Bearer sk-your-api-key
```

#### Configuration

API keys are mapped to user IDs in `config.toml`:

```toml
[security.api_key_users]
"sk-abc123def456" = "550e8400-e29b-41d4-a716-446655440000"
"sk-xyz789ghi012" = "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
```

#### Example Request

```bash
curl -X POST http://localhost:3000/v1/chat \
  -H "Authorization: Bearer sk-abc123def456" \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello"}'
```

### Authentication Errors

| Status | Code | Description |
|--------|------|-------------|
| 401 | `UNAUTHORIZED` | Missing or invalid API key |
| 403 | `FORBIDDEN` | Valid key, but action not allowed |

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Invalid or missing API key",
    "request_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

---

## Rate Limiting

Rate limiting is applied per IP address.

| Configuration | Default |
|---------------|---------|
| `rate_limit_rpm` | 60 requests/minute |

### Headers

```http
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1707321600
```

### Rate Limited Response

```http
HTTP/1.1 429 Too Many Requests
Retry-After: 30
```

```json
{
  "error": {
    "code": "RATE_LIMITED",
    "message": "Too many requests. Please retry after 30 seconds.",
    "retry_after": 30
  }
}
```

---

## Endpoints

### Health & Status

#### GET /health

Liveness probe. Returns 200 if the server is running.

**Authentication**: None required

**Response**: `200 OK`

```json
{
  "status": "ok"
}
```

---

#### GET /ready

Readiness probe with inference engine status.

**Authentication**: None required

**Response**: `200 OK` (healthy) or `503 Service Unavailable`

```json
{
  "status": "ready",
  "inference": {
    "healthy": true,
    "model": "qwen2.5-1.5b-instruct",
    "latency_ms": 45
  }
}
```

---

#### GET /ready/all

Extended health check with all service statuses.

**Authentication**: None required

**Response**: `200 OK`

```json
{
  "status": "ready",
  "services": {
    "inference": { "healthy": true, "latency_ms": 45 },
    "database": { "healthy": true, "latency_ms": 2 },
    "cache": { "healthy": true },
    "whatsapp": { "healthy": true, "latency_ms": 120 },
    "email": { "healthy": true, "latency_ms": 89 },
    "calendar": { "healthy": true, "latency_ms": 35 },
    "weather": { "healthy": true, "latency_ms": 180 }
  },
  "latency_percentiles": {
    "p50_ms": 45,
    "p90_ms": 120,
    "p99_ms": 250
  }
}
```

---

### Chat

#### POST /v1/chat

Send a message and receive a response.

**Authentication**: Required

**Request Body**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | Yes | User message |
| `conversation_id` | string | No | Continue existing conversation |
| `system_prompt` | string | No | Override system prompt |
| `model` | string | No | Override default model |
| `temperature` | float | No | Sampling temperature (0.0-2.0) |
| `max_tokens` | integer | No | Maximum response tokens |

```json
{
  "message": "What's the weather in Berlin?",
  "conversation_id": "conv-123",
  "temperature": 0.7
}
```

**Response**: `200 OK`

```json
{
  "id": "msg-456",
  "conversation_id": "conv-123",
  "role": "assistant",
  "content": "Currently in Berlin, it's 15°C with partly cloudy skies...",
  "model": "qwen2.5-1.5b-instruct",
  "tokens": {
    "prompt": 45,
    "completion": 128,
    "total": 173
  },
  "created_at": "2026-02-07T10:30:00Z"
}
```

---

#### POST /v1/chat/stream

Streaming chat using Server-Sent Events (SSE).

**Authentication**: Required

**Request Body**: Same as `/v1/chat`

**Response**: `200 OK` (text/event-stream)

```
event: message
data: {"delta": "Currently"}

event: message
data: {"delta": " in Berlin"}

event: message
data: {"delta": ", it's 15°C"}

event: done
data: {"tokens": {"prompt": 45, "completion": 128, "total": 173}}
```

**Example (JavaScript)**:

```javascript
const eventSource = new EventSource('/v1/chat/stream', {
  method: 'POST',
  headers: {
    'Authorization': 'Bearer sk-...',
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({ message: 'Hello' })
});

eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  process.stdout.write(data.delta);
};
```

---

### Commands

#### POST /v1/commands

Execute a command and get the result.

**Authentication**: Required

**Request Body**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string | Yes | Command to execute |
| `args` | object | No | Command arguments |

```json
{
  "command": "briefing"
}
```

**Response**: `200 OK`

```json
{
  "command": "MorningBriefing",
  "status": "completed",
  "result": {
    "weather": "15°C, partly cloudy",
    "calendar": [
      {"time": "09:00", "title": "Team standup"},
      {"time": "14:00", "title": "Client meeting"}
    ],
    "emails": {
      "unread": 5,
      "important": 2
    }
  },
  "executed_at": "2026-02-07T07:00:00Z"
}
```

**Available Commands**:

| Command | Description | Arguments |
|---------|-------------|-----------|
| `briefing` | Morning briefing | None |
| `weather` | Current weather | `location` (optional) |
| `calendar` | Today's events | `days` (default: 1) |
| `emails` | Email summary | `count` (default: 10) |
| `help` | List commands | None |

---

#### POST /v1/commands/parse

Parse a command without executing it.

**Authentication**: Required

**Request Body**:

```json
{
  "input": "create meeting tomorrow at 3pm"
}
```

**Response**: `200 OK`

```json
{
  "parsed": true,
  "command": {
    "type": "CreateCalendarEvent",
    "title": "meeting",
    "start": "2026-02-08T15:00:00Z",
    "end": "2026-02-08T16:00:00Z"
  },
  "confidence": 0.92,
  "requires_approval": true
}
```

---

### System

#### GET /v1/system/status

Get system status and resource usage.

**Authentication**: Required

**Response**: `200 OK`

```json
{
  "version": "0.1.0",
  "uptime_seconds": 86400,
  "environment": "production",
  "resources": {
    "memory_used_mb": 256,
    "cpu_percent": 15.5,
    "database_size_mb": 42
  },
  "statistics": {
    "requests_total": 15420,
    "inference_requests": 8930,
    "cache_hit_rate": 0.73
  }
}
```

---

#### GET /v1/system/models

List available inference models.

**Authentication**: Required

**Response**: `200 OK`

```json
{
  "models": [
    {
      "id": "qwen2.5-1.5b-instruct",
      "name": "Qwen 2.5 1.5B Instruct",
      "parameters": "1.5B",
      "context_length": 4096,
      "default": true
    },
    {
      "id": "llama3.2-1b-instruct",
      "name": "Llama 3.2 1B Instruct",
      "parameters": "1B",
      "context_length": 4096,
      "default": false
    }
  ]
}
```

---

### Webhooks

#### POST /v1/webhooks/whatsapp

WhatsApp webhook endpoint for incoming messages.

**Authentication**: Signature verification via `X-Hub-Signature-256` header

**Verification Request** (GET):

```http
GET /v1/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=your-token&hub.challenge=challenge123
```

**Response**: The `hub.challenge` value

**Message Webhook** (POST):

```json
{
  "object": "whatsapp_business_account",
  "entry": [{
    "changes": [{
      "value": {
        "messages": [{
          "from": "+1234567890",
          "type": "text",
          "text": { "body": "Hello" }
        }]
      }
    }]
  }]
}
```

**Response**: `200 OK`

---

### Metrics

#### GET /metrics

JSON metrics for monitoring.

**Authentication**: None required

**Response**: `200 OK`

```json
{
  "uptime_seconds": 86400,
  "http": {
    "requests_total": 15420,
    "requests_success": 15100,
    "requests_client_error": 280,
    "requests_server_error": 40,
    "active_requests": 3,
    "response_time_avg_ms": 125
  },
  "inference": {
    "requests_total": 8930,
    "requests_success": 8850,
    "requests_failed": 80,
    "time_avg_ms": 450,
    "tokens_total": 1250000,
    "healthy": true
  }
}
```

---

#### GET /metrics/prometheus

Prometheus-compatible metrics.

**Authentication**: None required

**Response**: `200 OK` (text/plain)

```prometheus
# HELP app_uptime_seconds Application uptime in seconds
# TYPE app_uptime_seconds counter
app_uptime_seconds 86400

# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{status="success"} 15100
http_requests_total{status="client_error"} 280
http_requests_total{status="server_error"} 40

# HELP inference_time_ms_bucket Inference time histogram
# TYPE inference_time_ms_bucket histogram
inference_time_ms_bucket{le="100"} 1200
inference_time_ms_bucket{le="250"} 4500
inference_time_ms_bucket{le="500"} 7200
inference_time_ms_bucket{le="1000"} 8500
inference_time_ms_bucket{le="+Inf"} 8930
```

---

## Error Handling

### Error Response Format

All errors follow this format:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "details": {},
    "request_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

### Error Codes

| HTTP Status | Code | Description |
|-------------|------|-------------|
| 400 | `BAD_REQUEST` | Invalid request body or parameters |
| 401 | `UNAUTHORIZED` | Missing or invalid authentication |
| 403 | `FORBIDDEN` | Authenticated but not authorized |
| 404 | `NOT_FOUND` | Resource not found |
| 422 | `VALIDATION_ERROR` | Request validation failed |
| 429 | `RATE_LIMITED` | Too many requests |
| 500 | `INTERNAL_ERROR` | Server error |
| 502 | `UPSTREAM_ERROR` | External service error |
| 503 | `SERVICE_UNAVAILABLE` | Service temporarily unavailable |

### Validation Errors

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Request validation failed",
    "details": {
      "fields": [
        {"field": "message", "error": "cannot be empty"},
        {"field": "temperature", "error": "must be between 0.0 and 2.0"}
      ]
    }
  }
}
```

---

## OpenAPI Specification

### Interactive Documentation

When the server is running, access interactive API documentation:

- **Swagger UI**: `http://localhost:3000/swagger-ui/`
- **ReDoc**: `http://localhost:3000/redoc/`

### Export OpenAPI Spec

```bash
# Via CLI
pisovereign-cli openapi --output openapi.json

# Via API (if enabled)
curl http://localhost:3000/api-docs/openapi.json
```

### OpenAPI 3.1 Specification

The full specification is available at:
- **Development**: `/api-docs/openapi.json`
- **GitHub Pages**: `/api/openapi.json`

<details>
<summary>Example OpenAPI Excerpt</summary>

```yaml
openapi: 3.1.0
info:
  title: PiSovereign API
  description: Local AI Assistant REST API
  version: 0.1.0
  license:
    name: MIT
    url: https://opensource.org/licenses/MIT

servers:
  - url: http://localhost:3000
    description: Development server

security:
  - bearerAuth: []

paths:
  /v1/chat:
    post:
      summary: Send chat message
      operationId: chat
      tags:
        - Chat
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/ChatRequest'
      responses:
        '200':
          description: Successful response
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ChatResponse'
        '401':
          $ref: '#/components/responses/Unauthorized'

components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      description: API key authentication

  schemas:
    ChatRequest:
      type: object
      required:
        - message
      properties:
        message:
          type: string
          description: User message
          example: "What's the weather?"
        conversation_id:
          type: string
          format: uuid
          description: Continue existing conversation
```

</details>

---

## SDK Examples

### cURL

```bash
# Chat
curl -X POST http://localhost:3000/v1/chat \
  -H "Authorization: Bearer sk-abc123" \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello"}'

# Command
curl -X POST http://localhost:3000/v1/commands \
  -H "Authorization: Bearer sk-abc123" \
  -H "Content-Type: application/json" \
  -d '{"command": "briefing"}'
```

### Python

```python
import requests

API_URL = "http://localhost:3000"
API_KEY = "sk-abc123"

headers = {
    "Authorization": f"Bearer {API_KEY}",
    "Content-Type": "application/json"
}

# Chat
response = requests.post(
    f"{API_URL}/v1/chat",
    headers=headers,
    json={"message": "What's the weather?"}
)
print(response.json()["content"])
```

### JavaScript/TypeScript

```typescript
const API_URL = "http://localhost:3000";
const API_KEY = "sk-abc123";

async function chat(message: string): Promise<string> {
  const response = await fetch(`${API_URL}/v1/chat`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${API_KEY}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ message }),
  });
  
  const data = await response.json();
  return data.content;
}
```
