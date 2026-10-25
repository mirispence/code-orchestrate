import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import { WorkspaceDetail } from "./WorkspaceDetail";
import { api, type AgentStatus, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return { ...actual, api: { agentStatus: vi.fn(), launchSession: vi.fn() } };
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
