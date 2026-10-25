import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import App from "./App";
import { api, type AgentStatus, type Workspace, type WorkspaceStats } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      listWorkspaces: vi.fn(),
      workspaceStats: vi.fn(),
      getSettings: vi.fn(),
      setSetting: vi.fn(),
      resetSettings: vi.fn(),
      agentStatus: vi.fn(),
      launchSession: vi.fn(),
      listSuggestions: vi.fn(),
      dismissPath: vi.fn(),
      startScan: vi.fn(),
      scanRunning: vi.fn(),
      onScanState: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

function workspace(id: number, name: string, overrides: Partial<Workspace> = {}): Workspace {
  return {
    id,
    name,
    path: `E:\\${name}`,
    pinned: false,
    note: null,
    createdAt: 0,
    tags: [],
    formerPaths: [],
    missing: false,
    ...overrides,
  };
}

function stats(workspaceId: number, overrides: Partial<WorkspaceStats> = {}): WorkspaceStats {
  return {
    workspaceId,
    sessionCount: 0,
    lastUsed: null,
    sessionTimeMs: null,
    tokens: { input: null, output: null, cacheWrite: null, cacheRead: null },
    ...overrides,
  };
}

function agent(overrides: Partial<AgentStatus> = {}): AgentStatus {
  return {
    id: "claude-code",
    displayName: "Claude Code",
    capabilities: { continue: true, resume: true, sessionHistory: true },
    executable: "claude.exe",
    ...overrides,
  };
}

const sidebar = () => within(screen.getByRole("complementary"));
const names = (list: string) =>
  within(sidebar().getByRole("list", { name: list }))
    .getAllByRole("button")
    .map((b) => b.getAttribute("data-workspace-name"));

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([]);
  mocked.workspaceStats.mockResolvedValue([]);
  mocked.listSuggestions.mockResolvedValue([]);
  mocked.scanRunning.mockResolvedValue(false);
  mocked.onScanState.mockResolvedValue(() => {});
  mocked.getSettings.mockResolvedValue({
    ok: true,
    value: {
      values: {
        theme: "system",
        "terminal.preset": "windowsTerminal",
        "terminal.windowsTerminal.target": "newWindow",
        "terminal.custom": null,
        "agents.claude-code.executable": "",
        "agents.claude-code.transcriptRoot": "",
      },
      warnings: {},
      fileError: null,
      choices: {},
    },
  });
  mocked.agentStatus.mockResolvedValue(agent());
});

describe("Sidebar stats", () => {
  beforeEach(() => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace(1, "alpha"),
      workspace(2, "beta"),
      workspace(3, "gamma"),
      workspace(4, "delta"),
      workspace(5, "pinned-old", { pinned: true }),
      workspace(6, "pinned-new", { pinned: true }),
      workspace(7, "pinned-idle", { pinned: true }),
    ]);
    mocked.workspaceStats.mockResolvedValue([
      stats(1),
      stats(2, { sessionCount: 1, lastUsed: new Date(2026, 0, 1).getTime() }),
      stats(3, { sessionCount: 4, lastUsed: new Date(2026, 5, 1).getTime() }),
      stats(4),
      stats(5, { sessionCount: 2, lastUsed: new Date(2025, 0, 1).getTime() }),
      stats(6, { sessionCount: 2, lastUsed: new Date(2026, 0, 1).getTime() }),
      stats(7),
    ]);
  });

  it("sorts Pinned and All by last used, newest first, with Session-less Workspaces last alphabetically", async () => {
    render(<App />);

    await waitFor(() => expect(names("All")).toEqual(["gamma", "beta", "alpha", "delta"]));
    expect(names("Pinned")).toEqual(["pinned-new", "pinned-old", "pinned-idle"]);
  });

  it("shows last used and the Session count on each row", async () => {
    render(<App />);

    const gamma = await sidebar().findByRole("button", { name: "gamma" });
    await waitFor(() => expect(gamma).toHaveAccessibleDescription(/Jun 1 · 4 Sessions/));
    expect(sidebar().getByRole("button", { name: "beta" })).toHaveAccessibleDescription(/ · 1 Session$/);
    expect(sidebar().getByRole("button", { name: "alpha" })).toHaveAccessibleDescription("No Sessions");
  });
});

describe("Stat tiles", () => {
  beforeEach(() => {
    mocked.listWorkspaces.mockResolvedValue([workspace(1, "app")]);
    mocked.workspaceStats.mockResolvedValue([
      stats(1, {
        sessionCount: 14,
        lastUsed: new Date(2025, 8, 30, 12, 0).getTime(),
        sessionTimeMs: 83 * 60_000,
        tokens: { input: 12_000, output: 300, cacheWrite: 1_000_000, cacheRead: 40_000_000 },
      }),
    ]);
  });

  async function open() {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await sidebar().findByRole("button", { name: "app" }));
    return { user, tiles: within(await screen.findByRole("group", { name: "Stats" })) };
  }

  function tile(tiles: ReturnType<typeof within>, label: string) {
    return tiles.getByText(label, { selector: "dt" }).nextElementSibling?.textContent;
  }

  it("shows Sessions, Last used, Session time and headline Tokens excluding cache reads", async () => {
    const { tiles } = await open();

    expect(tile(tiles, "Sessions")).toBe("14");
    expect(tile(tiles, "Last used")).toMatch(/Sep 30, 2025/);
    expect(tile(tiles, "Session time")).toBe("1h 23m");
    // 12,000 + 300 + 1,000,000.
    expect(tile(tiles, "Tokens")).toBe("1M");
  });

  it("expands Tokens into its four fields", async () => {
    const { user, tiles } = await open();

    await user.click(tiles.getByRole("button", { name: /Token breakdown/ }));

    const breakdown = within(screen.getByRole("list", { name: "Token breakdown" }));
    expect(breakdown.getByText("Input").nextElementSibling).toHaveTextContent("12k");
    expect(breakdown.getByText("Output").nextElementSibling).toHaveTextContent("300");
    expect(breakdown.getByText("Cache write").nextElementSibling).toHaveTextContent("1M");
    expect(breakdown.getByText("Cache read").nextElementSibling).toHaveTextContent("40M");
  });

  it("shows a token field the Agent does not report as a dash, never 0", async () => {
    mocked.workspaceStats.mockResolvedValue([
      stats(1, { sessionCount: 1, tokens: { input: 5, output: 5, cacheWrite: null, cacheRead: null } }),
    ]);
    const { user, tiles } = await open();

    expect(tile(tiles, "Tokens")).toBe("10");
    await user.click(tiles.getByRole("button", { name: /Token breakdown/ }));
    const breakdown = within(screen.getByRole("list", { name: "Token breakdown" }));
    expect(breakdown.getByText("Cache read").nextElementSibling).toHaveTextContent("—");
  });

  it("hides the tiles when the Agent keeps no Session history", async () => {
    mocked.agentStatus.mockResolvedValue(
      agent({ capabilities: { continue: true, resume: true, sessionHistory: false } }),
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(await sidebar().findByRole("button", { name: "app" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled());

    expect(screen.queryByRole("group", { name: "Stats" })).not.toBeInTheDocument();
  });
});
