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

### Conversations
- `GET /api/conversations` — List all conversations
- `POST /api/conversations` — Create a new conversation
- `GET /api/conversations/{id}` — Get conversation by ID
- `GET /api/conversations/{id}/messages` — List messages
- `POST /api/conversations/{id}/messages` — Send a message

### Connectors
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

## Error Handling

Errors are returned as JSON with appropriate HTTP status codes. Standard error format will be documented once implemented.
