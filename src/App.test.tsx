import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import App from "./App";
import { api, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      listWorkspaces: vi.fn(),
      addWorkspace: vi.fn(),
      pickFolder: vi.fn(),
      getSettings: vi.fn(),
      setSetting: vi.fn(),
      resetSettings: vi.fn(),
      agentStatus: vi.fn(),
      launchSession: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: 1,
    name: "code-orchestrate",
    path: "E:\\Programming\\code-orchestrate",
    pinned: false,
    note: null,
    createdAt: 1_700_000_000_000,
    ...overrides,
  };
}

function allList() {
  return screen.getByRole("list", { name: "All" });
}

async function clickAddWorkspace(user: ReturnType<typeof userEvent.setup>) {
  const sidebar = screen.getByRole("complementary");
  await user.click(within(sidebar).getByRole("button", { name: "Add Workspace" }));
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([]);
  mocked.getSettings.mockResolvedValue({
    ok: true,
    value: {
      values: {
        theme: "system",
        "terminal.preset": "windowsTerminal",
        "terminal.windowsTerminal.target": "newWindow",
        "terminal.custom": null,
      },
      warnings: {},
      fileError: null,
      choices: {},
    },
  });
  mocked.agentStatus.mockResolvedValue({
    id: "claude-code",
    displayName: "Claude Code",
    capabilities: { continue: true, resume: true, sessionHistory: true },
    executable: "claude.exe",
  });
});

describe("App shell", () => {
  it("opens to an empty state that points to Add Workspace and Suggestions", async () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Code Orchestrate" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: /no workspaces yet/i })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /add workspace/i }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: /suggestions/i }).length).toBeGreaterThan(0);
  });
});

describe("Workspace list", () => {
  it("lists saved Workspaces in the sidebar's All list", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "alpha", path: "E:\\alpha" }),
      workspace({ id: 2, name: "beta", path: "E:\\beta" }),
    ]);
    render(<App />);

    await waitFor(() => expect(within(allList()).getAllByRole("listitem")).toHaveLength(2));
    expect(within(allList()).getByText("alpha")).toBeInTheDocument();
    expect(within(allList()).getByText("beta")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /no workspaces yet/i })).not.toBeInTheDocument();
  });

  it("shows a Workspace's name and path when it is selected", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /code-orchestrate/ }));

    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { name: "code-orchestrate" })).toBeInTheDocument();
    expect(within(main).getByText("E:\\Programming\\code-orchestrate")).toBeInTheDocument();
  });
});

describe("Add Workspace", () => {
  it("picks a folder, pre-fills the name from its last segment, and lists the new Workspace", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\Programming\\my-project\\");
    const added = workspace({ id: 9, name: "My Project", path: "E:\\Programming\\my-project" });
    mocked.addWorkspace.mockResolvedValue({ ok: true, value: added });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);

    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    const name = within(dialog).getByLabelText("Name");
    expect(name).toHaveValue("my-project");
    expect(within(dialog).getByText("E:\\Programming\\my-project\\")).toBeInTheDocument();

    await user.clear(name);
    await user.type(name, "My Project");
    mocked.listWorkspaces.mockResolvedValue([added]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(mocked.addWorkspace).toHaveBeenCalledWith("My Project", "E:\\Programming\\my-project\\");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(allList()).getByText("My Project")).toBeInTheDocument();
  });

  it("does nothing when the folder picker is cancelled", async () => {
    mocked.pickFolder.mockResolvedValue(null);
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);

    await waitFor(() => expect(mocked.pickFolder).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("rejects a duplicate inline, linking to the existing Workspace", async () => {
    const existing = workspace({ id: 3, name: "Orchestrate", path: "E:\\code" });
    mocked.listWorkspaces.mockResolvedValue([existing]);
    mocked.pickFolder.mockResolvedValue("e:\\CODE\\");
    mocked.addWorkspace.mockResolvedValue({
      ok: false,
      error: {
        kind: "pathTaken",
        message: "Already a Workspace: Orchestrate",
        workspaceId: 3,
        workspaceName: "Orchestrate",
      },
    });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent("Already a Workspace: Orchestrate");

    await user.click(within(alert).getByRole("button", { name: "Orchestrate" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(screen.getByRole("main")).getByRole("heading", { name: "Orchestrate" })).toBeInTheDocument();
  });

  it("shows why a path was rejected", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\gone");
    mocked.addWorkspace.mockResolvedValue({
      ok: false,
      error: { kind: "notFound", message: "Folder not found: E:\\gone", path: "E:\\gone" },
    });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Folder not found: E:\\gone");
  });

  it("needs a name before saving", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\x");
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.clear(within(dialog).getByLabelText("Name"));

    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
  });
});

describe("Loading Workspaces", () => {
  it("shows a load failure with a retry, not an empty state", async () => {
    mocked.listWorkspaces.mockRejectedValueOnce(new Error("disk I/O error"));
    mocked.listWorkspaces.mockResolvedValueOnce([workspace({ name: "alpha" })]);
    const user = userEvent.setup();
    render(<App />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("disk I/O error");
    expect(screen.queryByRole("heading", { name: /no workspaces yet/i })).not.toBeInTheDocument();

    await user.click(within(alert).getByRole("button", { name: "Retry" }));

    expect(await within(allList()).findByText("alpha")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
