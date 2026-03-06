# Connectors

## Overview

Connectors integrate external platforms with Clawkson. They provide tools that agents can use during conversations.

## Connector Types

| Type | Description |
|---|---|
| `telegram` | Telegram Bot API integration |
| `gmail` | Gmail API for reading/sending email |
| `slack` | Slack workspace integration |
| `custom` | User-defined connector |

## How Connectors Work

1. **Add a connector** via the Connectors page or API
2. **Configure** with platform-specific credentials
3. **Tools become available** — each connector exposes tools (e.g., `send_email`, `read_inbox`)
4. **Use in conversations** — reference tools with `@toolname` syntax

## LLM Connectors

Users can bring their own LLM inference providers:

| Provider | Description |
|---|---|
| OpenAI | OpenAI API (GPT-4o, etc.) |
| Anthropic | Anthropic API (Claude) |
| Ollama | Local LLM inference |
| Custom | Any OpenAI-compatible API |

LLM connectors are configured in **Settings** and define which model is used for agent inference.

## Adding a Custom Connector

Custom connectors can be defined with:
- A name and description
- A JSON configuration object
- Tool definitions (JSON Schema for parameters)

Details on the connector SDK will be added as the feature matures.
