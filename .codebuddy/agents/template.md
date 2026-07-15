---
# Agent display name (required)
name: My Agent

# Agent description
description: A brief description of what this agent does

# Model override — omit to inherit the parent model
model: gpt-4o

# Allowed tools (comma-separated)
# Available: list_files, search_file, search_content, read_file, read_lints,
#            replace_in_file, write_to_file, execute_command, create_rule,
#            delete_files, web_fetch, use_skill, web_search
tools: read_file, search_content, list_files

# Rules to apply (comma-separated rule names)
rules: security-rule, code-style

# Skills to enable (comma-separated skill names)
skills: code-review

# MCP server whitelist (comma-separated)
mcpTools: MCP server w/ Browser Use

# A2A remote agent whitelist (comma-separated)
a2aAgents: weather_agent, math_agent

# Commands whitelist (comma-separated)
commands: build, deploy

# Execution mode: agentic | ...
agentMode: agentic

# Whether this agent is enabled
enabled: true

# Whether auto-run is enabled (skips user confirmation)
enabledAutoRun: false
---

# My Agent

You are a helpful assistant. Describe the agent's role and behavior here.

This section becomes the system prompt for the agent.
