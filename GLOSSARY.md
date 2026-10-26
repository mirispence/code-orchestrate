# Code Orchestrate

A desktop app that keeps a named index of the directories where you run AI coding agents, and launches an agent in a chosen one.

## Language

**Workspace**:
A directory the user has named and launches Agents in. If its directory moves, the Workspace keeps its former paths, so Sessions run there still belong to it.
_Avoid_: Project (Claude Code uses that word for its own storage folders), folder, repo

**Agent**:
A coding CLI the app can launch in a Workspace, such as Claude Code, Codex, or Antigravity.
_Avoid_: Tool, assistant, bot

**Session**:
One conversation with an Agent in a Workspace. It can be resumed or continued across several runs and stays the same Session; forking it starts a new one. An Agent's subagents belong to the Session that spawned them and are not Sessions themselves.
_Avoid_: Run, chat

**Suggestion**:
A directory where an Agent's Sessions have run that is not yet a Workspace. The user accepts it by naming it, which makes it a Workspace, or dismisses it so it stays hidden.
_Avoid_: Candidate, import

**Repository** (shorthand: **repo**):
The git (source control) repository a Workspace's directory is in. It is shown by its remote as `owner/name`, and its web page can be opened from the Workspace. A Workspace is a directory, not a Repository: several Workspaces can sit in one Repository, and a Workspace need not be in one.
_Avoid_: Remote (that is where the Repository is hosted), project
