# PRD — sharecli

## Overview

`sharecli` is a shared CLI process manager for multi-project agent orchestration, allowing agents to share CLI processes across projects.

## Goals

- Manage shared CLI processes across projects
- Support process lifecycle (start, stop, restart)
- Resource pooling and limits
- Session management
- Cross-project process communication

## Epics

### E1 — Process Management
- E1.1 Start/stop CLI processes
- E1.2 Process health monitoring
- E1.3 Process resource limits

### E2 — Session Management
- E2.1 Session creation and cleanup
- E2.2 Session state persistence
- E2.3 Session sharing across agents

### E3 — Configuration
- E3.1 Process configuration (env vars, args)
- E3.2 Resource limits configuration
- E3.3 Health check configuration

## Acceptance Criteria

- Processes can be started and stopped
- Sessions persist across restarts
- Resource limits are enforced
- Health checks work correctly
