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

## Telegram Connector

The Telegram connector uses the [Bot API](https://core.telegram.org/bots/api) and requires a **bot token** obtained from [@BotFather](https://t.me/BotFather).

### Configuration

| Field | Required | Description |
|---|---|---|
| `bot_token` | ✅ | Bot API token, e.g. `123456789:ABCdef...` |

### Adding via the UI

1. Go to **Connectors** in the sidebar.
2. Click **Add Connector**.
3. Select **Telegram** as the type.
4. Enter a name and your bot token.
5. Click **Add Connector** — the connector will appear in the list as enabled.

## How Connectors Work

1. **Add a connector** via the Connectors page or API
2. **Configure** with platform-specific credentials
3. **Tools become available** — each connector exposes tools (e.g., `send_email`, `read_inbox`)
4. **Use in conversations** — reference tools with `@toolname` syntax

## LLM Connectors

LLM connectors provide inference backends for agents. They are managed separately from platform connectors, via the **Settings → LLM Connectors** section of the UI or the `/api/llm-connectors` endpoints.

### Supported Providers

| Provider | `provider_type` | Notes |
|---|---|---|
| Azure OpenAI | `azure` | Requires deployment name + API version |
| OpenRouter | `open_router` | Unified API for 100+ models |
| OpenAI | `open_ai` | Direct OpenAI API |
| Custom (OpenAI-compatible) | `custom` | Any API following the OpenAI chat completions format |

### Azure OpenAI

Azure OpenAI uses a deployment-specific URL and the `api-key` header instead of `Authorization: Bearer`.

| Field | Required | Description |
|---|---|---|
| `name` | ✅ | Display name |
| `api_key` | ✅ | Azure OpenAI resource key |
| `api_base_url` | ✅ | Resource endpoint, e.g. `https://myresource.openai.azure.com` |
| `model` | ✅ | Model name (e.g. `gpt-4o`) |
| `azure_deployment` | ✅ | Deployment name in Azure portal |
| `azure_api_version` | ✅ | API version, e.g. `2024-02-01` |

### OpenRouter

[OpenRouter](https://openrouter.ai) provides a unified API for models from OpenAI, Anthropic, Mistral, and more.

| Field | Required | Description |
|---|---|---|
| `name` | ✅ | Display name |
| `api_key` | ✅ | OpenRouter API key (from openrouter.ai) |
| `model` | ✅ | Model ID, e.g. `anthropic/claude-3.5-sonnet` |

### OpenAI

| Field | Required | Description |
|---|---|---|
| `name` | ✅ | Display name |
| `api_key` | ✅ | OpenAI API key |
| `model` | ✅ | Model name, e.g. `gpt-4o` |

Clawkson automatically switches between `max_tokens` and `max_completion_tokens` based on provider compatibility, so newer OpenAI models that reject `max_tokens` continue to work without extra user configuration.

### Custom (OpenAI-compatible)

| Field | Required | Description |
|---|---|---|
| `name` | ✅ | Display name |
| `api_key` | ✅ | API key for the custom endpoint |
| `api_base_url` | ✅ | Base URL (e.g. `http://localhost:11434/v1` for Ollama) |
| `model` | ✅ | Model name |

### Security

- API keys are stored **in-memory only** — they are never persisted to disk.
- All API responses mask the key: only the last 4 characters are shown (e.g. `sk-or-••••1234`).
- Restarting the server requires re-entering API keys.

## Adding a Custom Connector

Custom connectors can be defined with:
- A name and description
- A JSON configuration object
- Tool definitions (JSON Schema for parameters)

Details on the connector SDK will be added as the feature matures.
