# Permissions & Audit Log

## Overview

Clawkson enforces a **deny-by-default** permission system on every tool invocation and every HTTP request an agent makes through a connector proxy. Every invocation — allowed or denied — is recorded in an append-only audit log for compliance, debugging, and analytics.

The design philosophy: agents use a minimal set of tools (primarily `code_execution` + `authenticated_http`) and write arbitrary code inside sandboxed containers. This means permission enforcement must happen at the **infrastructure layer**, not at tool registration time.

**Example use case:** "This agent should only ever read my Gmail and never delete, write, or move messages." This is expressed as a `ConnectorPolicy` with a single GET-only allow rule on `/gmail/v1/users/me/**`.

## Architecture: Three Permission Layers

```
Layer 1: Agent-level ConnectorPolicy     (admin/user configures per agent)
Layer 2: Task/Conversation override      (optional further restriction per conversation)
Layer 3: GuardedTool enforcement         (Rust code, deny-by-default, not bypassable)
```

### Layer 1: ConnectorPolicy

Each agent stores a `connector_policies` field (JSONB array) with one `ConnectorPolicy` per connector. A policy defines:

| Field | Type | Description |
|---|---|---|
| `connector_id` | UUID | Which connector this policy applies to |
| `allow` | `ProxyRule[]` | Allow rules — a request must match at least one |
| `deny` | `ProxyRule[]` | Deny rules — checked first, override allow rules |
| `rate_limit_rpm` | `int?` | Max requests per minute (null = unlimited) |

Each `ProxyRule` contains:

| Field | Type | Description |
|---|---|---|
| `methods` | `HttpMethod[]` | HTTP methods this rule applies to (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS) |
| `path_pattern` | `string` | Glob pattern matched against the URL path (e.g. `/gmail/v1/users/me/messages/**`) |
| `description` | `string` | Human-readable label for the UI |

### Layer 2: TaskPermissionOverride

An optional per-conversation restriction that can only **narrow** the agent's base permissions, never widen them.

| Field | Type | Description |
|---|---|---|
| `allowed_connector_ids` | `UUID[]?` | If set, only these connectors may be used (intersection with agent grants) |
| `allowed_methods` | `HttpMethod[]?` | If set, only these HTTP methods are allowed across all connectors |
| `disable_code_execution` | `bool` | Block `code_execution` tool for this conversation |
| `disable_workspace_write` | `bool` | Block `workspace_write` tool for this conversation |
| `disable_knowledge_access` | `bool` | Block `knowledge_list` and `knowledge_search` for this conversation |

### Layer 3: GuardedTool Enforcement

Every tool in the registry is wrapped with a guard at runtime:

- **`GuardedHttpTool`** wraps `AuthenticatedHttpTool`. Before any HTTP request is forwarded, the guard:
  1. Parses the HTTP method and target URL from the tool arguments.
  2. Checks `TaskPermissionOverride` method and connector restrictions.
  3. Extracts the URL path (strips query string, fragment, and host) using the `url` crate.
  4. Evaluates the `ConnectorPolicy` for the target connector.
  5. If denied, returns a JSON error to the LLM and writes an audit entry.
  6. If allowed, delegates to the inner tool and logs the invocation with execution duration.

- **`GuardedBuiltinTool`** wraps built-in tools (`code_execution`, `workspace_write`, `knowledge_list`, `knowledge_search`). It checks `TaskPermissionOverride` boolean flags before delegation.

Both guards are implemented in `crates/api/src/permission_guard.rs`.

## Policy Evaluation Order

For HTTP proxy requests:

```
1. Deny rules checked first
   └─ If any deny rule matches (method + glob on path) → BLOCKED

2. Allow rules checked next
   └─ Request must match at least one allow rule → ALLOWED

3. No allow rule matched → BLOCKED (deny-by-default)
```

Path patterns use the `glob-match` crate for matching. Examples:

| Pattern | Matches | Does not match |
|---|---|---|
| `/gmail/v1/users/me/messages/**` | `/gmail/v1/users/me/messages/123` | `/gmail/v1/users/me/labels/` |
| `/api/v1/*` | `/api/v1/items` | `/api/v1/items/123` |
| `/**` | Everything | — |
| `/bot*/sendMessage` | `/bot12345/sendMessage` | `/bot12345/deleteMessage` |

## Audit Log

### Database Schema

The `tool_audit_log` table (migration `0019_permissions_and_audit.sql`) stores every invocation:

| Column | Type | Description |
|---|---|---|
| `id` | UUID | Primary key |
| `conversation_id` | UUID | FK → conversations |
| `agent_id` | UUID | FK → agents |
| `user_id` | UUID | FK → users |
| `tool_name` | TEXT | Tool or proxy endpoint invoked |
| `http_method` | TEXT? | HTTP method (for proxy requests) |
| `target_path` | TEXT? | URL path (for proxy requests, no credentials) |
| `connector_id` | UUID? | FK → connectors (nullable, SET NULL on delete) |
| `decision` | TEXT | `"allowed"` or `"denied"` |
| `denial_reason` | TEXT? | Human-readable reason when denied |
| `duration_ms` | BIGINT? | Execution duration (null if denied before execution) |
| `created_at` | TIMESTAMPTZ | Timestamp |

Indexes exist on `conversation_id`, `agent_id`, `user_id`, `decision`, and `connector_id` for efficient querying.

### API Endpoints

All endpoints require authentication. Conversation and agent endpoints verify ownership (or admin role).

| Method | Path | Description |
|---|---|---|
| GET | `/api/audit-log/conversations/{conv_id}` | List audit entries for a conversation (paginated: `limit`, `offset`) |
| GET | `/api/audit-log/conversations/{conv_id}/stats` | Get allowed/denied counts for a conversation |
| GET | `/api/audit-log/agents/{agent_id}` | List audit entries for an agent (paginated) |
| GET | `/api/audit-log/denied` | List denied entries for the current user |

## Policy Presets

Eight built-in presets are compiled into the binary and served from `GET /api/policy-presets`. Users can select a preset in the UI as a starting point and customise from there.

| Preset | Connector | Description |
|---|---|---|
| `gmail_read_only` | Gmail | GET-only on `/gmail/v1/users/me/**`, 60 RPM |
| `gmail_read_send` | Gmail | Read + send (POST to `/messages/send`), deny DELETE, 30 RPM |
| `azure_devops_read_only` | Azure DevOps | GET-only on `/**`, 120 RPM |
| `azure_devops_work_items` | Azure DevOps | Read all + create/update work items, deny DELETE, 60 RPM |
| `telegram_read_send` | Telegram | Read updates + send messages, 30 RPM |
| `slack_read_only` | Slack | Read conversations, block `chat.postMessage` and `chat.delete`, 60 RPM |
| `custom_read_only` | Custom | GET-only on `/**`, no rate limit |
| `custom_full_access` | Custom | All methods on `/**`, no rate limit |

## Relevant Source Files

| File | Purpose |
|---|---|
| `crates/core/src/models.rs` (lines 225–365) | Data types: `HttpMethod`, `ProxyRule`, `ConnectorPolicy`, `PolicyPreset`, `TaskPermissionOverride`, `AuditDecision`, `ToolAuditEntry` |
| `crates/db/migrations/0019_permissions_and_audit.sql` | DB migration: `connector_policies` column + `tool_audit_log` table |
| `crates/db/src/tool_audit.rs` | CRUD: `insert`, `list_by_conversation`, `list_by_agent`, `list_denied_for_user`, `stats_by_conversation` |
| `crates/api/src/proxy.rs` | Policy evaluation engine with unit tests |
| `crates/api/src/permission_guard.rs` | `GuardedHttpTool` + `GuardedBuiltinTool` wrappers |
| `crates/api/src/routes/audit.rs` | Audit log HTTP endpoints + policy presets endpoint |
