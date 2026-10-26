import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import { WorkspaceDetail } from "./WorkspaceDetail";
import { api, type AgentStatus, type Session, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      agentStatus: vi.fn(),
      launchSession: vi.fn(),
      listWorkspaceSessions: vi.fn(),
      workspaceRepository: vi.fn(),
      openWorkspaceRepository: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

const ws: Workspace = {
  id: 7,
  name: "app",
  path: "C:\\My Projects\\app",
  pinned: false,
  note: null,
  createdAt: 1_700_000_000_000,
  tags: [],
  formerPaths: [],
  missing: false,
};

function status(overrides: Partial<AgentStatus> = {}): AgentStatus {
  return {
    id: "claude-code",
    displayName: "Claude Code",
    capabilities: { continue: true, resume: true, sessionHistory: true },
    executable: "C:\\Users\\me\\.local\\bin\\claude.exe",
    ...overrides,
  };
}

function renderDetail(onOpenSettings = vi.fn()) {
  render(<WorkspaceDetail workspace={ws} onEdit={vi.fn()} onTogglePin={vi.fn()} onOpenSettings={onOpenSettings} onLocate={vi.fn()} onFoundMissing={vi.fn()} />);
  return { onOpenSettings };
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.agentStatus.mockResolvedValue(status());
  mocked.launchSession.mockResolvedValue({ ok: true, value: undefined });
  mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [] });
  mocked.workspaceRepository.mockResolvedValue(null);
  mocked.openWorkspaceRepository.mockResolvedValue({ ok: true, value: null });
});

describe("Launching from a Workspace", () => {
  it("New Session launches a new Session in this Workspace", async () => {
    const user = userEvent.setup();
    renderDetail();

    const button = await screen.findByRole("button", { name: "New Session" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    expect(mocked.launchSession).toHaveBeenCalledWith(7, "new");
  });

  it("Continue last continues the last Session in this Workspace", async () => {
    const user = userEvent.setup();
    renderDetail();

    const button = await screen.findByRole("button", { name: "Continue last" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    expect(mocked.launchSession).toHaveBeenCalledWith(7, "continue");
  });

  it("disables both buttons with a pointer to Settings when Claude Code is not found", async () => {
    mocked.agentStatus.mockResolvedValue(status({ executable: null }));
    renderDetail();

    const hint = /Claude Code was not found on PATH\. Set its location in Settings\./;
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "New Session" })).toHaveAccessibleDescription(hint),
    );
    expect(screen.getByRole("button", { name: "New Session" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Continue last" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Continue last" })).toHaveAccessibleDescription(hint);
  });

  it("enables the buttons once Claude Code is found on a reload of the same Workspace", async () => {
    mocked.agentStatus.mockResolvedValue(status({ executable: null }));
    const props = {
      onEdit: vi.fn(),
      onTogglePin: vi.fn(),
      onOpenSettings: vi.fn(),
      onLocate: vi.fn(),
      onFoundMissing: vi.fn(),
    };
    const { rerender } = render(<WorkspaceDetail workspace={ws} {...props} />);
    await waitFor(() => expect(mocked.agentStatus).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("button", { name: "New Session" })).toBeDisabled();

    mocked.agentStatus.mockResolvedValue(status());
    rerender(<WorkspaceDetail workspace={{ ...ws }} {...props} />);

    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());
  });

  it("enables the buttons once Claude Code is found when the window regains focus", async () => {
    mocked.agentStatus.mockResolvedValue(status({ executable: null }));
    renderDetail();
    await waitFor(() => expect(mocked.agentStatus).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("button", { name: "New Session" })).toBeDisabled();

    mocked.agentStatus.mockResolvedValue(status());
    window.dispatchEvent(new Event("focus"));

    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());
  });

  it("applies only the newest of overlapping status checks", async () => {
    let resolveFirst: (s: AgentStatus) => void = () => {};
    mocked.agentStatus.mockReturnValueOnce(new Promise((resolve) => (resolveFirst = resolve)));
    mocked.agentStatus.mockResolvedValueOnce(status());
    renderDetail();

    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());
    resolveFirst(status({ executable: null }));

    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled();
  });

  it("hides Continue last when the Agent cannot continue", async () => {
    mocked.agentStatus.mockResolvedValue(
      status({ capabilities: { continue: false, resume: true, sessionHistory: true } }),
    );
    renderDetail();

    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());
    expect(screen.queryByRole("button", { name: "Continue last" })).not.toBeInTheDocument();
  });

  it("shows a failed launch as an error toast with an Open Settings action", async () => {
    mocked.launchSession.mockResolvedValue({
      ok: false,
      error: {
        kind: "spawnFailed",
        message: "Could not start wt.exe: Access is denied. (os error 5)",
        program: "wt.exe",
        osError: "Access is denied. (os error 5)",
      },
    });
    const user = userEvent.setup();
    const { onOpenSettings } = renderDetail();

    const button = await screen.findByRole("button", { name: "New Session" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    const toast = await screen.findByRole("alert");
    expect(toast).toHaveTextContent("Could not start wt.exe: Access is denied. (os error 5)");
    await user.click(screen.getByRole("button", { name: "Open Settings" }));
    expect(onOpenSettings).toHaveBeenCalled();
  });

  it("lets the error toast be dismissed", async () => {
    mocked.launchSession.mockResolvedValue({
      ok: false,
      error: { kind: "executableNotFound", message: "claude was not found", name: "claude" },
    });
    const user = userEvent.setup();
    renderDetail();

    const button = await screen.findByRole("button", { name: "New Session" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

function session(overrides: Partial<Session> = {}): Session {
  return {
    agentId: "claude-code",
    sessionId: "0f3c9a2e-1111-2222-3333-444455556666",
    firstCwd: "C:\\My Projects\\app",
    title: "Fix the login form",
    startedAt: null,
    lastActivityAt: null,
    activeTimeMs: 90 * 60_000,
    tokens: { input: 1_000, output: 200, cacheWrite: 300, cacheRead: 9_000 },
    sourcePath: "C:\\Users\\me\\.claude\\projects\\p\\0f3c9a2e.jsonl",
    transcriptExists: true,
    ...overrides,
  };
}

function sessionRow(title: string) {
  return screen.getByRole("row", { name: new RegExp(title) });
}

describe("Resuming a past Session", () => {
  it("lists every Session in the order given, with title, short id, active time and tokens", async () => {
    mocked.listWorkspaceSessions.mockResolvedValue({
      ok: true,
      value: [
        session({ sessionId: "aaaaaaaa-newest", title: "Newest" }),
        session({ sessionId: "bbbbbbbb-older", title: "Older", activeTimeMs: null }),
      ],
    });
    renderDetail();

    await screen.findByRole("row", { name: /Newest/ });
    expect(mocked.listWorkspaceSessions).toHaveBeenCalledWith(7);
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => within(r).getAllByRole("cell")[0].textContent)).toEqual(["Newest", "Older"]);
    const newest = sessionRow("Newest");
    expect(newest).toHaveTextContent("aaaaaaaa");
    expect(newest).not.toHaveTextContent("aaaaaaaa-newest");
    expect(newest).toHaveTextContent("1h 30m");
    expect(newest).toHaveTextContent("1.5k");
    expect(sessionRow("Older")).toHaveTextContent("—");
    for (const header of ["Title", "ID", "Started", "Active time", "Tokens"]) {
      expect(screen.getByRole("columnheader", { name: header })).toBeInTheDocument();
    }
  });

  it("Resume resumes that Session", async () => {
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ sessionId: "abc-123", title: "Fix it" })] });
    const user = userEvent.setup();
    renderDetail();

    await screen.findByRole("row", { name: /Fix it/ });
    const button = within(sessionRow("Fix it")).getByRole("button", { name: "Resume" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    expect(mocked.launchSession).toHaveBeenCalledWith(7, { resume: "abc-123" });
  });

  it('disables Resume with "Transcript deleted" when the transcript is gone, keeping its stats', async () => {
    mocked.listWorkspaceSessions.mockResolvedValue({
      ok: true,
      value: [session({ title: "Gone", transcriptExists: false }), session({ sessionId: "kept", title: "Kept" })],
    });
    renderDetail();

    await screen.findByRole("row", { name: /Gone/ });
    await waitFor(() => expect(within(sessionRow("Kept")).getByRole("button", { name: "Resume" })).toBeEnabled());
    const gone = within(sessionRow("Gone")).getByRole("button", { name: "Resume" });
    expect(gone).toBeDisabled();
    expect(gone).toHaveAccessibleDescription("Transcript deleted");
    expect(gone.parentElement).toHaveAttribute("title", "Transcript deleted");
    expect(sessionRow("Gone")).toHaveTextContent("1h 30m");
  });

  it("disables Resume when Claude Code is not found", async () => {
    mocked.agentStatus.mockResolvedValue(status({ executable: null }));
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ title: "Fix it" })] });
    renderDetail();

    await screen.findByRole("row", { name: /Fix it/ });
    const button = within(sessionRow("Fix it")).getByRole("button", { name: "Resume" });
    await waitFor(() => expect(button).toHaveAccessibleDescription(/Claude Code was not found on PATH/));
    expect(button).toBeDisabled();
  });

  it("disables Resume when the Workspace folder is missing", async () => {
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ title: "Fix it" })] });
    render(
      <WorkspaceDetail
        workspace={{ ...ws, missing: true }}
        onEdit={vi.fn()}
        onTogglePin={vi.fn()}
        onOpenSettings={vi.fn()}
        onLocate={vi.fn()}
        onFoundMissing={vi.fn()}
      />,
    );

    await screen.findByRole("row", { name: /Fix it/ });
    const button = within(sessionRow("Fix it")).getByRole("button", { name: "Resume" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/The folder is missing/);
  });

  it("re-reads the Sessions when the stats are reloaded", async () => {
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [] });
    const props = {
      workspace: ws,
      onEdit: vi.fn(),
      onTogglePin: vi.fn(),
      onOpenSettings: vi.fn(),
      onLocate: vi.fn(),
      onFoundMissing: vi.fn(),
    };
    const { rerender } = render(<WorkspaceDetail {...props} stats={null} />);
    await waitFor(() => expect(mocked.listWorkspaceSessions).toHaveBeenCalledTimes(1));

    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ title: "Fresh" })] });
    rerender(
      <WorkspaceDetail
        {...props}
        stats={{ workspaceId: 7, sessionCount: 1, lastUsed: 1, sessionTimeMs: 1, tokens: session().tokens }}
      />,
    );

    expect(await screen.findByRole("row", { name: /Fresh/ })).toBeInTheDocument();
  });
});

describe("Agent capabilities (spec §3.2)", () => {
  it("without resume, keeps the Session table but drops the Resume column and buttons", async () => {
    mocked.agentStatus.mockResolvedValue(
      status({ capabilities: { continue: true, resume: false, sessionHistory: true } }),
    );
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ title: "Kept" })] });
    renderDetail();

    await waitFor(() => expect(mocked.agentStatus).toHaveBeenCalled());
    expect(await screen.findByRole("row", { name: /Kept/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Resume" })).not.toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "Resume" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /Resume/ })).not.toBeInTheDocument();
  });

  it("without session history, hides the stat tiles and the Session table", async () => {
    mocked.agentStatus.mockResolvedValue(
      status({ capabilities: { continue: true, resume: true, sessionHistory: false } }),
    );
    mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [session({ title: "Hidden" })] });
    renderDetail();

    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });
});

describe("Header", () => {
  it("names the heading by the Workspace alone when pinned; the star is decorative", async () => {
    render(
      <WorkspaceDetail
        workspace={{ ...ws, pinned: true }}
        onEdit={vi.fn()}
        onTogglePin={vi.fn()}
        onOpenSettings={vi.fn()}
        onLocate={vi.fn()}
        onFoundMissing={vi.fn()}
      />,
    );

    expect(screen.getByRole("heading", { name: "app", level: 2 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Unpin" })).toBeInTheDocument();
    await screen.findByRole("button", { name: "New Session" });
  });
});

describe("Git repository (#32)", () => {
  const repository = { name: "owner/repo", url: "https://github.com/owner/repo" };

  it("shows the repository under the path and opens its page on click", async () => {
    mocked.workspaceRepository.mockResolvedValue(repository);
    const user = userEvent.setup();
    renderDetail();

    const link = await screen.findByRole("button", { name: "owner/repo" });
    expect(link).toHaveAttribute("title", "Open https://github.com/owner/repo");
    await user.click(link);

    expect(mocked.workspaceRepository).toHaveBeenCalledWith(7);
    expect(mocked.openWorkspaceRepository).toHaveBeenCalledWith(7);
  });

  it("shows nothing when the folder has no repository with a page", async () => {
    renderDetail();

    await waitFor(() => expect(mocked.workspaceRepository).toHaveBeenCalledWith(7));
    await screen.findByRole("button", { name: "New Session" });
    expect(screen.queryByRole("button", { name: "owner/repo" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows nothing, and no error, when looking up the repository fails", async () => {
    mocked.workspaceRepository.mockRejectedValue(new Error("git exploded"));
    renderDetail();

    await waitFor(() => expect(mocked.workspaceRepository).toHaveBeenCalled());
    await screen.findByRole("button", { name: "New Session" });
    expect(screen.queryByText(/git exploded/)).not.toBeInTheDocument();
  });

  it("does not look up the repository of a Missing Workspace", async () => {
    mocked.workspaceRepository.mockResolvedValue(repository);
    render(
      <WorkspaceDetail
        workspace={{ ...ws, missing: true }}
        onEdit={vi.fn()}
        onTogglePin={vi.fn()}
        onOpenSettings={vi.fn()}
        onLocate={vi.fn()}
        onFoundMissing={vi.fn()}
      />,
    );

    await screen.findByRole("button", { name: "Locate…" });
    expect(mocked.workspaceRepository).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "owner/repo" })).not.toBeInTheDocument();
  });

  it("shows a failed open as a dismissible error toast", async () => {
    mocked.workspaceRepository.mockResolvedValue(repository);
    mocked.openWorkspaceRepository.mockResolvedValue({
      ok: false,
      error: { kind: "noRepository", message: "This folder no longer has a git remote with a web page" },
    });
    const user = userEvent.setup();
    renderDetail();

    await user.click(await screen.findByRole("button", { name: "owner/repo" }));

    const toast = await screen.findByRole("alert");
    expect(toast).toHaveTextContent("This folder no longer has a git remote with a web page");
    expect(within(toast).queryByRole("button", { name: "Open Settings" })).not.toBeInTheDocument();
    await user.click(within(toast).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("drops the previous Workspace's repository when another is shown", async () => {
    mocked.workspaceRepository.mockResolvedValueOnce(repository).mockReturnValueOnce(new Promise(() => {}));
    const props = { onEdit: vi.fn(), onTogglePin: vi.fn(), onOpenSettings: vi.fn(), onLocate: vi.fn(), onFoundMissing: vi.fn() };
    const { rerender } = render(<WorkspaceDetail workspace={ws} {...props} />);
    await screen.findByRole("button", { name: "owner/repo" });

    rerender(<WorkspaceDetail workspace={{ ...ws, id: 8, name: "other" }} {...props} />);

    expect(screen.queryByRole("button", { name: "owner/repo" })).not.toBeInTheDocument();
    await waitFor(() => expect(mocked.workspaceRepository).toHaveBeenLastCalledWith(8));
  });
});
