## Clawkson
We are building clawkson. Clawkson is an ai assistant, similar to openclaw, that is supposed to be useful in daily life.

Clawkson should come with a nice web frontend, written in react+tsx+bun.

The backend is written in rust for max perf.

We mainain an api spec in openapi format, in the openapi.yml file. Pls always ensure it is up to date, and that the api contracts are respected in the backend implementation and frontend.

We also maintain a living documentation in the docs/ folder, which should be updated with any new features or changes to existing features, and please consult the docs first when building new stuff, to ensure we are not just going in any direction.

It should be possible to run the backend on bare metal securily, but the agent has access to a docker container, where it has ROOT, that is isolated from the host system, and can be used to run any code that the agent needs to execute, without risking the security of the host system. The agent can also access a file system, where it can read and write files as needed.

Alongside this, the agent is really split into multiple sub ageents, configurable by the user in the frontend. for orchestration we use: https://github.com/Force67/denkwerk.

Users should also be able to bring their own connectors for LLM inferance.

Lets structure the frontend based around a sidebar with the following items:
- Dashboard: A overview of the agents, their status, and recent activity.
- conversations: A place to view and manage conversations with the agents.
- Knowledge Base: A place to manage the knowledge base that the agents can access.
- Connectors: A way to add platforms such as telegram/gmail etc.
- tools: Tools are provided by the connectors, and then mentionable with the @toolname syntax in the conversations, to trigger the tool.
- settings: A place to manage the settings of the agents, connectors, and tools.
- documentation: render of the documentation in the frontend.