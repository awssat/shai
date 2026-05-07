# Shai — Project Memory for AI Agents

> **⚠️ Experimental:** This is an active research project. Things may break.

**Shai is a shared memory layer for AI coding agents.** It records everything your agents do in a project—every file change, every tool call, every decision—and makes that history available to any agent you use.

## The Problem: Git Isn't Enough

You commit your code at 2 PM. An agent works for an hour, makes progress, then ruins something at 3 PM. 

**Git can't help you.**

You can't rollback to that intermediate state at 2:45 PM. You either lose the hour of work, or you live with the breakage. Git only sees snapshots—it doesn't track the **journey** between commits.

And even when you have git history, it leaves critical questions unanswered:

- **What prompt caused this change?** Git shows you `auth.rs` was modified, but not *why*. Was it part of a refactoring? A bug fix? A feature request?
- **What else changed with this file?** When `auth.rs` was modified, were there related changes to `database.rs` or `tests/auth_test.rs`? Git shows files in isolation.
- **What was the context?** A commit message like "fix auth bug" doesn't tell you what the bug was, what the agent tried first, or what trade-offs were considered.
- **Which agent did this?** Was it Claude's architecture decision, or Gemini's implementation, or Copilot's debugging?

Git tracks **what** changed. Shai tracks **why, how, and what else** changed with it.

## The Other Problem: Agents Don't Remember

You start a feature with Claude, continue with Gemini, debug with Copilot. Each agent starts blind. They can't see what the other did. You waste time re-explaining context, and they make mistakes because they don't know the history.

## The Solution: Portable Project Memory

Shai gives all your agents the same memory. They can all:
- See what happened in previous sessions (across all agents)
- Understand why files were changed
- Search through the complete project history
- Roll back mistakes intelligently

Switch agents mid-task? No problem. The next agent automatically gets the full context.

## How It Works

Shai runs as a transparent wrapper around your favorite AI agents:

```bash
# Use any agent - they all get the same memory
shai run claude
shai run gemini
shai run copilot
shai run opencode
shai run junie
```

Behind the scenes, Shai:
1. **Records** every tool call and file change to a local database (`.shai/`)
2. **Summarizes** changes into human-readable descriptions (e.g., "Added authentication middleware")
3. **Injects** relevant history into each new agent session
4. **Preserves** your normal workflow—agents work exactly as before

The agent gets smarter immediately, knowing what happened before. You get continuity across all your tools.

## What Shai Tracks

Shai doesn't just store raw diffs—it understands the **relationships** between changes:

- **Prompts → Changes:** "Added authentication" because user asked: "Implement OAuth login"
- **Causal chains:** When `auth.rs` was modified, these files changed too: `user.rs`, `middleware.rs`, `config.toml`
- **Evolution over time:** `auth.rs` was changed 5 times across 3 sessions—here's the complete story
- **Agent attribution:** Claude designed the structure, Gemini implemented it, Copilot debugged the token refresh
- **Rollback granularity:** Not just to last commit—rollback to any specific change within the past hour

**Git tells you a file changed. Shai tells you the story behind it.**

## Quick Start

**Install:**
```bash
curl -fsSL https://raw.githubusercontent.com/awssat/shai/main/install.sh | sh
```

**Use with your existing agents:**
```bash
shai run claude      # Works with Claude Code
shai run gemini     # Works with Gemini CLI
shai run copilot    # Works with GitHub Copilot CLI
shai run opencode   # Works with OpenCode
shai run junie      # Works with JetBrains Junie
```

That's it. The agent now knows your project history.

## Core Commands

| Command | Purpose |
|---|---|
| `shai run <agent>` | Run any AI agent with project memory |
| `shai history` | See everything that happened across all sessions |
| `shai search "<query>"` | Find specific changes in project history |
| `shai log <file>` | See the complete evolution of a specific file |
| `shai summary` | Get a quick digest of recent project activity |
| `shai why <file>` | Understand why a file was changed |
| `shai diff <file>` | Preview a rollback before applying it |
| `shai rollback <file>` | Restore a file to any previous version |
| `shai status` | Check database size and health |
| `shai analytics` | See which files/tools are used most |
| `shai export` / `import` | Share project memory with your team |

## Why This Matters

**Switch agents freely:**
- Morning: Use Claude for architecture design
- Afternoon: Continue with Gemini for implementation
- Evening: Debug with Copilot

Each agent sees what the others did. No context loss.

**Agents learn from each other:**
- Gemini can see that Claude already tried approach X and failed
- Copilot knows that Gemini refactored the authentication module last week
- Claude understands that Copilot's bug fix introduced a new edge case

**Project memory survives:**
- Uninstall an agent? History stays in `.shai/`
- Reinstall an agent? It instantly knows the project context
- Team member joins? Import the memory archive, they're up to speed

## How Agents Get Context

Shai automatically adapts to each agent's native integration method:

| Agent | Integration Method |
|-------|-------------------|
| Claude | `--append-system-prompt-file` flag |
| Gemini | `GEMINI_SYSTEM_MD` environment variable |
| Copilot | `.github/copilot-instructions.md` file |
| OpenCode | `--system` flag |
| Junie | `--skill-location` flag |
| Others | Stdin injection (fallback) |

Each agent gets:
- Last 20 sessions (compact summary)
- Tool descriptions for `shai` commands
- Recent project history
- Ability to query full history via commands

## Example Workflow

```bash
# 2:00 PM - Commit your work
git commit -m "WIP on authentication feature"

# 2:15 PM - Start with Claude
shai run claude
> Add user authentication to the API
[Claude adds auth, records changes to .shai/]

# 2:45 PM - Continue with Gemini
shai run gemini
> Add password reset to the auth system
[Gemini sees Claude's auth work, builds on it]

# 3:15 PM - Debug with Copilot
shai run copilot
> The password reset email isn't sending
[Copilot sees both previous sessions, knows the full context]

# 3:30 PM - Something breaks
# Git can't help - your last commit was 1.5 hours ago
# You'd lose everything since 2:00 PM

# But with Shai, you can investigate:
shai log auth.rs
# See: Claude added OAuth, Gemini added reset, Copilot broke the email template
# pinpoint exactly when it broke

shai why auth.rs
# Shows: "Modified email template to use new variable" - that's the bug!

shai diff auth.rs --steps 1
# Preview: Revert just Copilot's last change, keep Gemini's work

shai rollback auth.rs --steps 1
# Fixed! You're back to 3:15 PM, Gemini's work is safe
# No git reset --hard needed, no work lost

# Want to see what changed together?
shai search "password reset"
# Shows: auth.rs, email.rs, config.toml all modified together
# You know exactly what files are related to this feature
```

## Documentation

- [Complete Documentation](docs/INDEX.md)
- [Command Reference](docs/QUICK_REFERENCE.md)
- [Storage Schema](docs/SCHEMA.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)

## Local & Private

All data stays in your project's `.shai/` directory. Nothing leaves your machine. No cloud services. No API calls.

Your project memory is yours alone—unless you choose to share it via `shai export`.

---

**Shai: Because switching agents shouldn't mean losing context.**
