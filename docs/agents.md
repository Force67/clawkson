# Agents

## Overview

Agents are the core building blocks of Clawkson. Each agent is a configurable sub-agent that can be wired to an LLM connector, given a system prompt, tuned for specific tasks, and linked to one or more Knowledge Bases for retrieval-augmented generation.

## Agent Properties

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `name` | string | Display name |
| `description` | string | What the agent does |
| `status` | enum | `online`, `offline`, `busy`, `error` |
| `llm_connector_id` | UUID? | The LLM connector this agent uses for inference |
| `system_prompt` | string? | System instruction prepended to every conversation |
| `temperature` | float? | Sampling temperature (0–2). Controls creativity vs. determinism |
| `max_tokens` | int? | Maximum tokens to generate per response |
| `created_at` | datetime | Creation timestamp |
| `updated_at` | datetime | Last update timestamp |

## Creating and Configuring an Agent

Agents can be created via `POST /api/agents` and configured via `PATCH /api/agents/{id}`.

Example create request:
```json
{
  "name": "Research Assistant",
  "description": "Helps with literature review and summarization",
  "system_prompt": "You are a precise research assistant. Always cite your reasoning.",
  "temperature": 0.7,
  "max_tokens": 2048,
  "llm_connector_id": "<uuid of an LLM connector>"
}
```

## LLM Connector Assignment

An agent without an `llm_connector_id` will fall back to the **default** LLM connector configured in Settings. If no connector is available, the agent will return a descriptive error message rather than failing silently.

## System Prompt

The `system_prompt` is prepended as a `system` role message to every LLM call made by this agent. Use it to define the agent's persona, constraints, and capabilities.

## Knowledge Base Linking

Agents can be linked to one or more Knowledge Bases, giving them access to your stored documents and text entries for retrieval-augmented generation.

- **Link:** `POST /api/knowledge/{kb_id}/agents` with `{ "agent_id": "<uuid>" }`
- **Unlink:** `DELETE /api/knowledge/{kb_id}/agents/{agent_id}`
- **List linked agents:** `GET /api/knowledge/{kb_id}/agents`

## Agent Lifecycle

1. **Created** — Agent is defined with a name and description
2. **Configured** — LLM connector, system prompt, and parameters assigned
3. **Online** — Agent is running and ready to receive messages
4. **Busy** — Agent is processing a task
5. **Offline** — Agent is stopped

## Container Management

Each agent can be run inside an isolated Docker container. The following API endpoints manage the container lifecycle:

| Method | Path | Description |
|---|---|---|
| POST | `/api/agents/{id}/start` | Start the container |
| POST | `/api/agents/{id}/stop` | Stop the container |
| GET | `/api/agents/{id}/logs` | Stream container logs (SSE) |
| POST | `/api/agents/{id}/exec` | Execute a command inside the container |
| GET | `/api/agents/{id}/status` | Get current container status |
| POST | `/api/agents/{id}/remove` | Remove the container |

## Orchestration

Agents are orchestrated via [Denkwerk](https://github.com/Force67/denkwerk). The orchestrator manages:
- Agent lifecycle
- Task distribution
- Inter-agent communication
- Container isolation (Docker)

## Security

Each agent runs inside an isolated Docker container with:
- No host filesystem access (except explicitly mounted paths)
- Network isolation
- Resource limits (CPU, memory)
- Root access only within the container
