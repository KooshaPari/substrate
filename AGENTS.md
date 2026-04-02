# AGENTS.md — sharecli

Extends shelf-level AGENTS.md rules for sharecli.

## Project Identity

- **Name**: sharecli
- **Language**: Rust

## Project-Specific Rules

### Test-First Mandate

- **For NEW modules**: test file MUST exist before implementation file
- **For BUG FIXES**: failing test MUST be written before the fix
- **For REFACTORS**: existing tests must pass before AND after

### Quality Gates

All PRs must pass:
- Format check
- Linting
- Tests
- Type checking (if applicable)

### Commit Messages

Format: `<type>(<scope>): <description>`

Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`

## Naming Conventions

- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`

## Error Handling

- Use language-appropriate error handling patterns
- Never use unwrap/expect in production code
- Log all errors with structured logging
