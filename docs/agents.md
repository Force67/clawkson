# Agents

## Overview

Agents are the core building blocks of Clawkson. Each agent is a configurable sub-agent that can perform specific tasks autonomously.

## Agent Properties

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `name` | string | Display name |
| `description` | string | What the agent does |
| `status` | enum | `online`, `offline`, `busy`, `error` |
| `created_at` | datetime | Creation timestamp |
| `updated_at` | datetime | Last update timestamp |

## Agent Lifecycle

1. **Created** — Agent is defined with a name and description
2. **Configured** — Tools and connectors are assigned
3. **Online** — Agent is running and ready to receive messages
4. **Busy** — Agent is processing a task
5. **Offline** — Agent is stopped

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
