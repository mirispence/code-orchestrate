// Missing and moved Workspaces (spec §6.3–§6.6, §8.2).
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
      workspaceStats: vi.fn(),
      listWorkspaceSessions: vi.fn(),
      addWorkspace: vi.fn(),
      repointWorkspace: vi.fn(),
      removeFormerPath: vi.fn(),
      pickFolder: vi.fn(),
      updateWorkspace: vi.fn(),
      setPinned: vi.fn(),
      removeWorkspace: vi.fn(),
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

function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: 1,
    name: "app",
    path: "E:\\old\\app",
    pinned: false,
    note: null,
    createdAt: 1_700_000_000_000,
    tags: [],
    formerPaths: [],
    missing: false,
    ...overrides,
  };
}

function sidebar() {
  return within(screen.getByRole("complementary"));
}

function main() {
  return within(screen.getByRole("main"));
}

async function select(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await sidebar().findByRole("button", { name: new RegExp(name) }));
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([]);
  mocked.workspaceStats.mockResolvedValue([]);
  mocked.listWorkspaceSessions.mockResolvedValue({ ok: true, value: [] });
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

describe("A missing Workspace", () => {
  it("stays listed with a Missing badge", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ missing: true })]);
    render(<App />);

    const row = await sidebar().findByRole("button", { name: /app/ });
    expect(within(row).getByText("Missing")).toBeInTheDocument();
  });

  it("is re-checked when it is selected", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    const user = userEvent.setup();
    render(<App />);
    await sidebar().findByRole("button", { name: /app/ });

    mocked.listWorkspaces.mockResolvedValue([workspace({ missing: true })]);
    await select(user, "app");

    expect(await main().findByText(/Folder not found/)).toBeInTheDocument();
  });

  it("shows a banner and disables New and Continue with a tooltip", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ missing: true })]);
    const user = userEvent.setup();
    render(<App />);
    await select(user, "app");

    expect(await main().findByText(/Folder not found/)).toBeInTheDocument();
    const newSession = main().getByRole("button", { name: "New Session" });
    await waitFor(() => expect(mocked.agentStatus).toHaveBeenCalled());
    expect(newSession).toBeDisabled();
    expect(main().getByRole("button", { name: "Continue last" })).toBeDisabled();
    expect(newSession.parentElement).toHaveAttribute("title", expect.stringMatching(/folder is missing/i));
  });

  it("Locate… re-points it and the banner goes away", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ missing: true })]);
    mocked.pickFolder.mockResolvedValue("E:\\new\\app");
    const moved = workspace({ path: "E:\\new\\app", formerPaths: ["E:\\old\\app"] });
    mocked.repointWorkspace.mockImplementation(async () => {
      mocked.listWorkspaces.mockResolvedValue([moved]);
      return { ok: true, value: { workspace: moved, movedFrom: null } };
    });
    const user = userEvent.setup();
    render(<App />);
    await select(user, "app");

    await user.click(await main().findByRole("button", { name: "Locate…" }));

    expect(mocked.repointWorkspace).toHaveBeenCalledWith(1, "E:\\new\\app");
    await waitFor(() => expect(main().queryByText(/Folder not found/)).not.toBeInTheDocument());
    expect(main().getByText("E:\\new\\app")).toBeInTheDocument();
  });

  it("re-checks when a launch finds the folder gone", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    mocked.launchSession.mockImplementation(async () => {
      mocked.listWorkspaces.mockResolvedValue([workspace({ missing: true })]);
      return {
        ok: false,
        error: { kind: "workspaceMissing", message: "Folder not found", workspaceId: 1, path: "E:\\old\\app" },
      };
    });
    const user = userEvent.setup();
    render(<App />);
    await select(user, "app");
    const button = await main().findByRole("button", { name: "New Session" });
    await waitFor(() => expect(button).toBeEnabled());
    const listsBefore = mocked.listWorkspaces.mock.calls.length;

    await user.click(button);

    await waitFor(() => expect(mocked.listWorkspaces.mock.calls.length).toBeGreaterThan(listsBefore));
    await waitFor(() => expect(main().getByRole("button", { name: "New Session" })).toBeDisabled());
  });
});

describe("Editing a Workspace's path", () => {
  async function openEdit(user: ReturnType<typeof userEvent.setup>) {
    await select(user, "app");
    await user.click(main().getByRole("button", { name: "Edit" }));
    return within(await screen.findByRole("dialog", { name: "Edit Workspace" }));
  }

  beforeEach(() => {
    mocked.updateWorkspace.mockImplementation(async (id, edit) => ({
      ok: true,
      value: workspace({ id, name: edit.name, tags: edit.tags }),
    }));
  });

  it("re-points through Locate… in the dialog", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    mocked.pickFolder.mockResolvedValue("E:\\new\\app");
    const moved = workspace({ path: "E:\\new\\app", formerPaths: ["E:\\old\\app"] });
    mocked.repointWorkspace.mockResolvedValue({ ok: true, value: { workspace: moved, movedFrom: null } });
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openEdit(user);

    await user.click(dialog.getByRole("button", { name: "Locate…" }));
    expect(dialog.getByLabelText("Path")).toHaveValue("E:\\new\\app");
    await user.click(dialog.getByRole("button", { name: "Save" }));

    expect(mocked.repointWorkspace).toHaveBeenCalledWith(1, "E:\\new\\app");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("does not re-point when the path is unchanged", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openEdit(user);

    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(mocked.updateWorkspace).toHaveBeenCalled());
    expect(mocked.repointWorkspace).not.toHaveBeenCalled();
  });

  it("shows a rejected path inline and keeps the dialog open", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    mocked.repointWorkspace.mockResolvedValue({
      ok: false,
      error: { kind: "pathTaken", message: "Already a Workspace: Other", workspaceId: 2, workspaceName: "Other" },
    });
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openEdit(user);

    const path = dialog.getByLabelText("Path");
    await user.clear(path);
    await user.type(path, "E:\\other");
    await user.click(dialog.getByRole("button", { name: "Save" }));

    expect(await dialog.findByRole("alert")).toHaveTextContent("Already a Workspace: Other");
    expect(mocked.updateWorkspace).not.toHaveBeenCalled();
  });

  it("lists former paths and removes one on Save", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ formerPaths: ["E:\\older", "E:\\oldest"] })]);
    mocked.removeFormerPath.mockResolvedValue({ ok: true, value: workspace() });
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openEdit(user);

    const list = dialog.getByRole("list", { name: "Former paths" });
    expect(within(list).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["E:\\older", "E:\\oldest"]);
    await user.click(dialog.getByRole("button", { name: "Remove former path E:\\older" }));
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
    await user.click(dialog.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(mocked.removeFormerPath).toHaveBeenCalledWith(1, "E:\\older"));
    expect(mocked.removeFormerPath).toHaveBeenCalledTimes(1);
  });
});

describe("Taking another Workspace's former path", () => {
  it("adding there succeeds with a notice naming the former owner", async () => {
    const other = workspace({ id: 2, name: "Other", path: "E:\\new\\other", formerPaths: ["E:\\old\\app"] });
    mocked.listWorkspaces.mockResolvedValue([other]);
    mocked.pickFolder.mockResolvedValue("E:\\old\\app");
    const added = workspace({ id: 3, name: "app" });
    mocked.addWorkspace.mockResolvedValue({
      ok: true,
      value: { workspace: added, movedFrom: { workspaceId: 2, workspaceName: "Other" } },
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(sidebar().getByRole("button", { name: "Add Workspace" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Add Workspace" }));
    await user.click(dialog.getByRole("button", { name: "Save" }));

    const notice = await screen.findByRole("status");
    expect(notice).toHaveTextContent("E:\\old\\app was a former path of Other");
  });
});
