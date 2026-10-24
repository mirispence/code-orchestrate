import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import App from "./App";
import { api, type Suggestion, type Workspace } from "@/lib/api";

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
      listSuggestions: vi.fn(),
      dismissPath: vi.fn(),
      startScan: vi.fn(),
      scanRunning: vi.fn(),
      onScanState: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

function suggestion(overrides: Partial<Suggestion> = {}): Suggestion {
  return {
    path: "E:\\Programming\\my-app",
    sessionCount: 3,
    lastSeen: Date.UTC(2026, 9, 24, 4, 0),
    missing: false,
    ...overrides,
  };
}

const added: Workspace = {
  id: 7,
  name: "My App",
  path: "E:\\Programming\\my-app",
  pinned: false,
  note: null,
  createdAt: 1_700_000_000_000,
  tags: [],
};

/** The listener App registered for scan-state events. */
let emitScanState: (running: boolean) => void = () => {};

function sidebar() {
  return within(screen.getByRole("complementary"));
}

async function openSuggestions(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await sidebar().findByRole("button", { name: /^Suggestions/ }));
  return screen.findByRole("heading", { name: "Suggestions", level: 2 });
}

function row(path: string) {
  return within(screen.getByRole("listitem", { name: path }));
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([]);
  mocked.listSuggestions.mockResolvedValue([]);
  mocked.scanRunning.mockResolvedValue(false);
  mocked.startScan.mockResolvedValue(undefined);
  mocked.onScanState.mockImplementation(async (listener) => {
    emitScanState = listener;
    return () => {};
  });
  mocked.getSettings.mockResolvedValue({
    ok: true,
    value: { values: { theme: "system" }, warnings: {}, fileError: null },
  });
  mocked.agentStatus.mockResolvedValue({
    id: "claude-code",
    displayName: "Claude Code",
    capabilities: { continue: true, resume: true, sessionHistory: true },
    executable: "claude.exe",
  });
});

describe("Suggestions entry", () => {
  it("shows a count badge for stored Suggestions", async () => {
    mocked.listSuggestions.mockResolvedValue([
      suggestion(),
      suggestion({ path: "E:\\other" }),
    ]);
    render(<App />);

    const entry = await sidebar().findByRole("button", { name: "Suggestions (2)" });
    expect(within(entry).getByText("2")).toBeInTheDocument();
  });

  it("has no badge when there are none", async () => {
    render(<App />);
    expect(await sidebar().findByRole("button", { name: "Suggestions" })).toBeInTheDocument();
  });
});

describe("Suggestions inbox", () => {
  it("lists each Suggestion with its path, Session count and last seen", async () => {
    mocked.listSuggestions.mockResolvedValue([suggestion()]);
    const user = userEvent.setup();
    render(<App />);

    await openSuggestions(user);

    const r = row("E:\\Programming\\my-app");
    expect(r.getByText("E:\\Programming\\my-app")).toBeInTheDocument();
    expect(r.getByText("3 Sessions")).toBeInTheDocument();
    expect(r.getByText(/Last seen/)).toHaveTextContent(new Date(Date.UTC(2026, 9, 24, 4, 0)).toLocaleString());
  });

  it("adds a Suggestion as a Workspace with an editable name pre-filled from the folder", async () => {
    mocked.listSuggestions.mockResolvedValueOnce([suggestion()]).mockResolvedValue([]);
    mocked.addWorkspace.mockResolvedValue({ ok: true, value: added });
    const user = userEvent.setup();
    render(<App />);
    await openSuggestions(user);

    const name = row("E:\\Programming\\my-app").getByRole("textbox", { name: "Name" });
    expect(name).toHaveValue("my-app");
    mocked.listWorkspaces.mockResolvedValue([added]);
    await user.clear(name);
    await user.type(name, "My App");
    await user.click(row("E:\\Programming\\my-app").getByRole("button", { name: "Add" }));

    expect(mocked.addWorkspace).toHaveBeenCalledWith("My App", "E:\\Programming\\my-app");
    await waitFor(() => expect(sidebar().getByRole("button", { name: "My App" })).toBeInTheDocument());
    expect(screen.queryByRole("listitem", { name: "E:\\Programming\\my-app" })).not.toBeInTheDocument();
  });

  it("shows why a Suggestion could not be added", async () => {
    mocked.listSuggestions.mockResolvedValue([suggestion()]);
    mocked.addWorkspace.mockResolvedValue({
      ok: false,
      error: { kind: "notFound", message: "Folder not found: E:\\Programming\\my-app", path: "E:\\Programming\\my-app" },
    });
    const user = userEvent.setup();
    render(<App />);
    await openSuggestions(user);

    await user.click(row("E:\\Programming\\my-app").getByRole("button", { name: "Add" }));

    expect(await row("E:\\Programming\\my-app").findByRole("alert")).toHaveTextContent("Folder not found");
  });

  it("dismisses a Suggestion", async () => {
    mocked.listSuggestions.mockResolvedValueOnce([suggestion()]).mockResolvedValue([]);
    mocked.dismissPath.mockResolvedValue({ ok: true, value: null });
    const user = userEvent.setup();
    render(<App />);
    await openSuggestions(user);

    await user.click(row("E:\\Programming\\my-app").getByRole("button", { name: "Dismiss" }));

    expect(mocked.dismissPath).toHaveBeenCalledWith("E:\\Programming\\my-app");
    await waitFor(() =>
      expect(screen.queryByRole("listitem", { name: "E:\\Programming\\my-app" })).not.toBeInTheDocument(),
    );
    expect(screen.getByText(/no suggestions/i)).toBeInTheDocument();
  });

  it("offers only Dismiss for a Suggestion whose directory is gone", async () => {
    mocked.listSuggestions.mockResolvedValue([suggestion({ missing: true })]);
    const user = userEvent.setup();
    render(<App />);
    await openSuggestions(user);

    const r = row("E:\\Programming\\my-app");
    expect(r.getByText("Missing")).toBeInTheDocument();
    expect(r.queryByRole("button", { name: "Add" })).not.toBeInTheDocument();
    expect(r.queryByRole("textbox")).not.toBeInTheDocument();
    expect(r.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("opens from the empty state", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Open Suggestions" }));

    expect(await screen.findByRole("heading", { name: "Suggestions", level: 2 })).toBeInTheDocument();
  });
});

describe("Scanning", () => {
  it("refreshes in the background with a spinner, then reloads when the scan finishes", async () => {
    const user = userEvent.setup();
    render(<App />);
    const refresh = await sidebar().findByRole("button", { name: "Refresh" });
    await waitFor(() => expect(mocked.onScanState).toHaveBeenCalled());
    expect(refresh).not.toHaveAttribute("aria-busy", "true");

    await user.click(refresh);
    expect(mocked.startScan).toHaveBeenCalled();
    act(() => emitScanState(true));
    expect(refresh).toHaveAttribute("aria-busy", "true");

    mocked.listSuggestions.mockResolvedValue([suggestion()]);
    act(() => emitScanState(false));

    expect(await sidebar().findByRole("button", { name: "Suggestions (1)" })).toBeInTheDocument();
    expect(refresh).not.toHaveAttribute("aria-busy", "true");
  });

  it("shows the spinner when it mounts during the startup scan", async () => {
    mocked.scanRunning.mockResolvedValue(true);
    render(<App />);

    const refresh = await sidebar().findByRole("button", { name: "Refresh" });
    await waitFor(() => expect(refresh).toHaveAttribute("aria-busy", "true"));
  });
});
