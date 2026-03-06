# API Reference

The full API specification is maintained in [`openapi.yml`](../openapi.yml).

## Base URL

```
http://localhost:47821/api
```

## Authentication

Not yet implemented. Will be added in a future iteration.

## Endpoints Summary

### Agents
- `GET /api/agents` — List all agents
- `POST /api/agents` — Create a new agent
- `GET /api/agents/{id}` — Get agent by ID
- `PATCH /api/agents/{id}` — Update an agent (name, description, llm_connector_id, system_prompt, temperature, max_tokens, status)
- `DELETE /api/agents/{id}` — Delete an agent

### Conversations
- `GET /api/conversations` — List all conversations
- `POST /api/conversations` — Create a new conversation
- `GET /api/conversations/{id}` — Get conversation by ID
- `GET /api/conversations/{id}/messages` — List messages
- `POST /api/conversations/{id}/messages` — Send a raw message
- `POST /api/conversations/{id}/chat` — Send a user message and get an AI response (blocking)
- `POST /api/conversations/{id}/chat/stream` — Send a user message and stream AI response via SSE

### LLM Connectors
- `GET /api/llm-connectors` — List all LLM connectors (API keys masked)
- `POST /api/llm-connectors` — Create an LLM connector
- `GET /api/llm-connectors/{id}` — Get connector by ID
- `PATCH /api/llm-connectors/{id}` — Update a connector, including provider type, model, endpoint, and credentials
- `DELETE /api/llm-connectors/{id}` — Delete a connector
- `POST /api/llm-connectors/test` — Validate connector settings against the selected provider without saving them

### Settings
- `GET /api/settings` — Get application settings
- `PATCH /api/settings` — Update application settings

### Connectors (platform integrations)
- `GET /api/connectors` — List all connectors
- `POST /api/connectors` — Create a connector
- `GET /api/connectors/{id}` — Get connector by ID

### Knowledge Base
- `GET /api/knowledge` — List all entries
- `POST /api/knowledge` — Create a knowledge entry
- `GET /api/knowledge/{id}` — Get entry by ID

### Tools
- `GET /api/tools` — List all tools
- `POST /api/tools` — Create a tool
- `GET /api/tools/{id}` — Get tool by ID

## Chat Streaming (SSE)

`POST /api/conversations/{id}/chat/stream` returns a Server-Sent Events stream.

Each event contains a JSON payload:
- **Delta**: `{"delta": "text chunk"}` — incremental token from the LLM
- **Done**: `{"done": true, "id": "<message-uuid>"}` — stream finished; `id` is the saved message ID
- **Error**: `{"error": "message"}` — something went wrong

The frontend uses `fetch()` with `ReadableStream` (not `EventSource`) to support POST-based SSE.

## Error Handling

Errors are returned as JSON with appropriate HTTP status codes. Standard error format will be documented once implemented.
