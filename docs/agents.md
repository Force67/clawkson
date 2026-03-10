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

The `system_prompt` is the per-agent instruction set. It is combined with the platform-level
base prompt (see below) and then prepended as a `system` role message to every LLM call.

### Prompt Layering

The final system prompt sent to the LLM is assembled in three layers:

```
[1] Settings.agent_base_prompt   ← platform-wide steering (admin-only)
[2] agent.system_prompt          ← per-agent persona, task, constraints
[3] <available-skills> block     ← injected at runtime if skills are linked
```

Layers are joined with `\n\n`. Empty layers are skipped entirely — if all three are
empty, no system message is sent.

**Layer 1 — `agent_base_prompt`** is set globally in Settings (`PATCH /api/settings`,
admin only). Use it for guardrails, identity, tool-usage rules, and Docker container
permissions that must apply to every agent.

The canonical source for this prompt is **`SOUL.md`** in the repository root. The server
reads this file at startup and automatically writes the prompt body (everything after the
first `---` separator) to `Settings.agent_base_prompt`. Changes take effect on the next
restart. The file path can be overridden with the `CLAWKSON_SOUL_PATH` environment variable.

**Layer 2 — `system_prompt`** is the user-configured field on each agent. Use it to
define the agent's persona, domain, and specific task instructions.

**Layer 3 — skills** are appended automatically when skills are linked to the agent.

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
| POST | `/api/agents/{id}/container/start` | Start the container |
| POST | `/api/agents/{id}/container/stop` | Stop the container |
| DELETE | `/api/agents/{id}/container` | Remove the container |
| GET | `/api/agents/{id}/container` | Get current container status |
| GET | `/api/agents/{id}/container/logs` | Get container logs (`?tail=N`) |
| POST | `/api/agents/{id}/container/exec` | Execute a command inside the container |

### Workspace File I/O

Each container has a `/workspace` directory bind-mounted from the host. Files placed here are immediately visible inside the container. The following endpoints manage workspace files:

| Method | Path | Description |
|---|---|---|
| GET | `/api/agents/{id}/container/workspace` | List workspace files (`?path=subdir`) |
| POST | `/api/agents/{id}/container/workspace/upload` | Upload files (`multipart/form-data`, optional `path` field) |
| GET | `/api/agents/{id}/container/workspace/download` | Download a file (`?path=outputs/result.csv`) |
| DELETE | `/api/agents/{id}/container/workspace` | Delete a file or directory |
| GET | `/api/agents/{id}/container/workspace/watch` | SSE stream of workspace file changes |

#### Agent-driven file I/O (during a conversation)

When `container_enabled` is true, the agent is automatically given three workspace tools it can call during any conversation:

| Tool | Description |
|---|---|
| `workspace_list` | List files in the workspace (default: root, or any sub-path) |
| `workspace_read` | Read a file's text content back into the conversation |
| `workspace_write` | Write text content to a file in the workspace |

Combined with `code_execution`, the typical flow for file-based tasks is:

1. User uploads a file (e.g. `data.xlsx`) as a chat attachment → it is automatically written to `/workspace/inputs/data.xlsx`.
2. Agent calls `code_execution` (Python/Bash) to process `/workspace/inputs/data.xlsx` and write results to `/workspace/outputs/`.
3. Output files in `/workspace/outputs/` are automatically read back and included in the tool result, so the agent can summarise or further process them without additional tool calls.
4. User can also download outputs via `GET /api/agents/{id}/container/workspace/download?path=outputs/result.csv`.

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
