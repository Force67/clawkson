import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import styles from './Docs.module.css'

export function DocsPage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Documentation"
        description="Living documentation for the Clawkson platform."
      />

      <div className={styles.docContent}>
        <Card>
          <article className={styles.article}>
            <h2>Welcome to Clawkson</h2>
            <p>
              Clawkson is an AI assistant platform designed to be useful in daily life.
              It provides a multi-agent architecture where each agent can be configured
              independently and orchestrated via{' '}
              <a href="https://github.com/Force67/denkwerk" target="_blank" rel="noreferrer">
                Denkwerk
              </a>.
            </p>

            <h3>Architecture Overview</h3>
            <ul>
              <li><strong>Frontend:</strong> React + TypeScript + Bun (this app)</li>
              <li><strong>Backend:</strong> Rust (Axum) for maximum performance</li>
              <li><strong>Orchestration:</strong> Denkwerk for multi-agent coordination</li>
              <li><strong>API Spec:</strong> OpenAPI format, maintained in <code>openapi.yml</code></li>
            </ul>

            <h3>Core Concepts</h3>

            <h4>Agents</h4>
            <p>
              Agents are the core building blocks. Each agent is a sub-agent that can be
              configured by the user. They run inside isolated Docker containers for
              security.
            </p>

            <h4>Conversations</h4>
            <p>
              Users interact with agents through conversations. The <code>@toolname</code> syntax
              can be used to invoke specific tools within a conversation.
            </p>

            <h4>Knowledge Base</h4>
            <p>
              A shared knowledge store that agents can access. Entries can be tagged and
              searched.
            </p>

            <h4>Connectors</h4>
            <p>
              Platforms like Telegram, Gmail, Slack etc. Connectors provide tools that
              agents can use.
            </p>

            <h4>LLM Connectors</h4>
            <p>
              Users can bring their own LLM inference connectors — OpenAI, Anthropic,
              local Ollama instances, or any OpenAI-compatible API.
            </p>

            <h3>Getting Started</h3>
            <ol>
              <li>Configure an LLM connector in <strong>Settings</strong></li>
              <li>Add connectors for platforms you want to integrate</li>
              <li>Create agents and assign them tools</li>
              <li>Start a conversation and chat with your agents</li>
            </ol>
          </article>
        </Card>
      </div>
    </div>
  )
}
